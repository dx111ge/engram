use super::*;

impl Graph {
    /// BFS traversal from a starting node, up to max_depth hops.
    /// Traverse the graph from a start node using BFS.
    ///
    /// `direction` controls which edges to follow:
    /// - `"out"` -- outgoing edges only (traditional BFS)
    /// - `"in"` -- incoming edges only (reverse traversal)
    /// - `"both"` -- both directions (default, full neighborhood)
    pub fn traverse(
        &self,
        start_label: &str,
        max_depth: u32,
        min_confidence: f32,
    ) -> Result<TraversalResult> {
        self.traverse_directed(start_label, max_depth, min_confidence, "both")
    }

    pub fn traverse_directed(
        &self,
        start_label: &str,
        max_depth: u32,
        min_confidence: f32,
        direction: &str,
    ) -> Result<TraversalResult> {
        let start_id = self.find_node_id(start_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;

        let follow_out = direction == "out" || direction == "both";
        let follow_in = direction == "in" || direction == "both";

        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
        let mut result = TraversalResult {
            nodes: Vec::new(),
            edges: Vec::new(),
            depths: HashMap::new(),
        };

        visited.insert(start_id);
        queue.push_back((start_id, 0));
        result.nodes.push(start_id);
        result.depths.insert(start_id, 0);

        while let Some((node_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Follow outgoing edges
            if follow_out {
                if let Some(edge_slots) = self.adj_out.get(&node_id) {
                    for &edge_slot in edge_slots {
                        let edge = self.brain.read_edge(edge_slot)?;
                        if edge.is_deleted() || edge.confidence < min_confidence {
                            continue;
                        }

                        let target = edge.to_node;

                        if let Some(target_slot) = self.find_slot_by_id(target) {
                            let target_node = self.brain.read_node(target_slot)?;
                            if !target_node.is_active() || target_node.confidence < min_confidence {
                                continue;
                            }
                        }

                        result.edges.push((node_id, target, edge_slot));

                        if visited.insert(target) {
                            queue.push_back((target, depth + 1));
                            result.nodes.push(target);
                            result.depths.insert(target, depth + 1);
                        }
                    }
                }
            }

            // Follow incoming edges
            if follow_in {
                if let Some(edge_slots) = self.adj_in.get(&node_id) {
                    for &edge_slot in edge_slots {
                        let edge = self.brain.read_edge(edge_slot)?;
                        if edge.is_deleted() || edge.confidence < min_confidence {
                            continue;
                        }

                        let source = edge.from_node;

                        if let Some(source_slot) = self.find_slot_by_id(source) {
                            let source_node = self.brain.read_node(source_slot)?;
                            if !source_node.is_active() || source_node.confidence < min_confidence {
                                continue;
                            }
                        }

                        result.edges.push((source, node_id, edge_slot));

                        if visited.insert(source) {
                            queue.push_back((source, depth + 1));
                            result.nodes.push(source);
                            result.depths.insert(source, depth + 1);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Search by vector similarity (nearest neighbors).
    /// Uses GPU/NPU via compute planner when available and vector count is large enough.
    pub fn search_vector(&self, query_vector: &[f32], limit: usize) -> Result<Vec<NodeSearchResult>> {
        // Try GPU/NPU brute-force for large vector sets when a planner is configured
        if let Some(ref planner) = self.compute_planner {
            let vec_count = self.hnsw.vector_count();
            let backend = planner.select_similarity_backend(vec_count);
            if backend != engram_compute::planner::Backend::Cpu {
                if let Some(ranked) = self.hnsw.brute_force_with_planner(query_vector, limit, planner) {
                    let mut results = Vec::with_capacity(ranked.len());
                    for (slot, distance) in ranked {
                        let node = self.brain.read_node(slot)?;
                        if node.is_active() {
                            results.push(NodeSearchResult {
                                slot,
                                node_id: node.id,
                                label: self.full_label(slot)?,
                                confidence: node.confidence,
                                score: 1.0 - distance as f64,
                            });
                        }
                    }
                    return Ok(results);
                }
                // Fall through to HNSW if GPU/NPU failed
            }
        }

        let hits = self.hnsw.search(query_vector, limit);
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let node = self.brain.read_node(hit.slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot: hit.slot,
                    node_id: node.id,
                    label: self.full_label(hit.slot)?,
                    confidence: node.confidence,
                    score: 1.0 - hit.distance as f64, // convert distance to similarity
                });
            }
        }
        Ok(results)
    }

    /// Hybrid search: combines BM25 keyword search with vector similarity using RRF.
    pub fn search_hybrid(
        &self,
        query_text: &str,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<NodeSearchResult>> {
        let keyword_hits = self.fulltext.search(query_text, limit * 2);
        let keyword_results: Vec<(u64, f64)> = keyword_hits.iter().map(|h| (h.slot, h.score)).collect();

        let vector_hits = self.hnsw.search(query_vector, limit * 2);
        let vector_results: Vec<(u64, f32)> = vector_hits.iter().map(|h| (h.slot, h.distance)).collect();

        let fused = hybrid::reciprocal_rank_fusion(&keyword_results, &vector_results, limit);
        let mut results = Vec::with_capacity(fused.len());
        for hit in fused {
            let node = self.brain.read_node(hit.slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot: hit.slot,
                    node_id: node.id,
                    label: self.full_label(hit.slot)?,
                    confidence: node.confidence,
                    score: hit.score,
                });
            }
        }
        Ok(results)
    }

    /// Hybrid search using text only -- embeds the query automatically if an embedder is set.
    pub fn search_hybrid_text(&self, query_text: &str, limit: usize) -> Result<Vec<NodeSearchResult>> {
        match &self.embedder {
            Some(embedder) => {
                let query_vector = embedder.embed(query_text).map_err(|e| StorageError::InvalidFile {
                    reason: e.to_string(),
                })?;
                self.search_hybrid(query_text, &query_vector, limit)
            }
            None => {
                // Fallback to keyword-only search if no embedder
                self.search_text(query_text, limit)
            }
        }
    }

    /// Full-text search across labels and properties.
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<NodeSearchResult>> {
        let hits = self.fulltext.search(query, limit);
        self.hits_to_results(&hits)
    }

    /// Execute a parsed query, returning matching nodes.
    pub fn search(&self, query_str: &str, limit: usize) -> std::result::Result<Vec<NodeSearchResult>, String> {
        let q = query::parse(query_str).map_err(|e| e.to_string())?;
        let slots = self.execute_query(&q, limit).map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        for &slot in &slots {
            if let Ok(node) = self.brain.read_node(slot) {
                if node.is_active() {
                    results.push(NodeSearchResult {
                        slot,
                        node_id: node.id,
                        label: self.full_label(slot).map_err(|e| e.to_string())?,
                        confidence: node.confidence,
                        score: 0.0, // filters don't produce scores
                    });
                }
            }
        }
        results.truncate(limit);
        Ok(results)
    }

    /// Get most recently created nodes.
    pub fn recent(&self, n: usize) -> Result<Vec<NodeSearchResult>> {
        let slots = self.temporal.most_recent(TimeAxis::Created, n);
        let mut results = Vec::new();
        for slot in slots {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot,
                    node_id: node.id,
                    label: self.full_label(slot)?,
                    confidence: node.confidence,
                    score: 0.0,
                });
            }
        }
        Ok(results)
    }

    /// Prefix-based autocomplete: returns active nodes whose label starts with `prefix` (case-insensitive).
    /// Priority: exact match > starts_with > word starts_with > contains, up to `limit`.
    pub fn autocomplete(&self, prefix: &str, limit: usize) -> Result<Vec<NodeSearchResult>> {
        let (node_count, _) = self.brain.stats();
        let prefix_lower = prefix.to_lowercase();
        let mut exact = Vec::new();
        let mut starts = Vec::new();
        let mut word_starts = Vec::new();
        let mut contains = Vec::new();

        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if !node.is_active() { continue; }
            let label = self.full_label(slot)?;
            let label_lower = label.to_lowercase();
            let hit = NodeSearchResult {
                slot, node_id: node.id, label, confidence: node.confidence, score: 0.0,
            };
            if label_lower == prefix_lower {
                exact.push(NodeSearchResult { score: 1.0, ..hit });
            } else if label_lower.starts_with(&prefix_lower) {
                starts.push(NodeSearchResult { score: 0.9, ..hit });
            } else if label_lower.split_whitespace().any(|w| w.starts_with(&prefix_lower)) {
                if word_starts.len() < limit { word_starts.push(NodeSearchResult { score: 0.7, ..hit }); }
            } else if label_lower.contains(&prefix_lower) {
                if contains.len() < limit { contains.push(NodeSearchResult { score: 0.5, ..hit }); }
            }
        }

        // Merge in priority order
        let mut results = exact;
        results.append(&mut starts);
        results.append(&mut word_starts);
        results.append(&mut contains);
        results.truncate(limit);
        Ok(results)
    }

    /// Iterate all active nodes, returning (label, node_type_id, confidence, memory_tier) for each.
    pub fn all_nodes(&self) -> Result<Vec<NodeSnapshot>> {
        let (node_count, _) = self.brain.stats();
        let mut result = Vec::new();
        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                let label = self.full_label(slot)?;
                let node_type_name = self.node_type_names.get(node.node_type as usize).cloned();
                let properties = self.props.get_all(slot).cloned().unwrap_or_default();
                result.push(NodeSnapshot {
                    label,
                    node_type: node_type_name,
                    confidence: node.confidence,
                    memory_tier: node.memory_tier,
                    properties,
                    created_at: node.created_at,
                    updated_at: node.updated_at,
                    edge_out_count: node.edge_out_count,
                    edge_in_count: node.edge_in_count,
                });
            }
        }
        Ok(result)
    }

    /// Iterate all edges, returning (from_label, to_label, relationship, confidence).
    pub fn all_edges(&self) -> Result<Vec<EdgeView>> {
        let (_, edge_count) = self.brain.stats();
        let mut result = Vec::new();
        for slot in 0..edge_count {
            let edge = self.brain.read_edge(slot)?;
            if edge.is_deleted() {
                continue;
            }
            let from_label = self.label_for_id(edge.from_node)?;
            let to_label = self.label_for_id(edge.to_node)?;
            let rel_name = self.type_registry.name_or_default(edge.edge_type);
            result.push(EdgeView {
                from: from_label,
                to: to_label,
                relationship: rel_name,
                confidence: edge.confidence,
                valid_from: timestamp_to_date(edge.valid_from),
                valid_to: timestamp_to_date(edge.valid_to),
            });
        }
        Ok(result)
    }

    /// Get all core-tier nodes (always in LLM context).
    pub fn core_nodes(&self) -> Result<Vec<NodeSearchResult>> {
        let slots = self.tier_bitmap.slots_for(0); // TIER_CORE = 0
        let mut results = Vec::with_capacity(slots.len());
        for slot in slots {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot,
                    node_id: node.id,
                    label: self.full_label(slot)?,
                    confidence: node.confidence,
                    score: 0.0,
                });
            }
        }
        Ok(results)
    }

    pub(crate) fn execute_query(&self, query: &Query, limit: usize) -> Result<Vec<u64>> {
        match query {
            Query::FullText(text) => {
                let hits = self.fulltext.search(text, limit);
                Ok(hits.into_iter().map(|h| h.slot).collect())
            }
            Query::Label(label) => {
                match self.find_slot_by_label(label)? {
                    Some(slot) => Ok(vec![slot]),
                    None => Ok(Vec::new()),
                }
            }
            Query::NodeType(name) => {
                if let Some(type_id) = self.node_type_id(name) {
                    Ok(self.type_bitmap.slots_for(type_id))
                } else {
                    Ok(Vec::new())
                }
            }
            Query::Tier(name) => {
                let tier_val = match name.as_str() {
                    "core" => 0u32,
                    "active" => 1,
                    "archival" => 2,
                    _ => return Ok(Vec::new()),
                };
                Ok(self.tier_bitmap.slots_for(tier_val))
            }
            Query::Sensitivity(name) => {
                let sens_val = match name.as_str() {
                    "public" => 0u32,
                    "internal" => 1,
                    "confidential" => 2,
                    "restricted" => 3,
                    _ => return Ok(Vec::new()),
                };
                Ok(self.sensitivity_bitmap.slots_for(sens_val))
            }
            Query::Confidence { op, value } => {
                let (count, _) = self.brain.stats();
                let mut slots = Vec::new();
                for slot in 0..count {
                    let node = self.brain.read_node(slot)?;
                    if !node.is_active() { continue; }
                    let pass = match op {
                        CmpOp::Gt => node.confidence > *value,
                        CmpOp::Gte => node.confidence >= *value,
                        CmpOp::Lt => node.confidence < *value,
                        CmpOp::Lte => node.confidence <= *value,
                        CmpOp::Eq => (node.confidence - value).abs() < f32::EPSILON,
                    };
                    if pass { slots.push(slot); }
                }
                Ok(slots)
            }
            Query::CreatedRange { from, to } => {
                let f = from.unwrap_or(i64::MIN);
                let t = to.unwrap_or(i64::MAX);
                Ok(self.temporal.range(TimeAxis::Created, f, t))
            }
            Query::EventRange { from, to } => {
                let f = from.unwrap_or(i64::MIN);
                let t = to.unwrap_or(i64::MAX);
                Ok(self.temporal.range(TimeAxis::Event, f, t))
            }
            Query::Property { key, value } => {
                let (count, _) = self.brain.stats();
                let mut slots = Vec::new();
                for slot in 0..count {
                    let node = self.brain.read_node(slot)?;
                    if !node.is_active() { continue; }
                    if let Some(v) = self.props.get(slot, key) {
                        if v == value {
                            slots.push(slot);
                        }
                    }
                }
                Ok(slots)
            }
            Query::And(left, right) => {
                let left_slots: HashSet<u64> = self.execute_query(left, usize::MAX)?.into_iter().collect();
                let right_slots = self.execute_query(right, usize::MAX)?;
                Ok(right_slots.into_iter().filter(|s| left_slots.contains(s)).collect())
            }
            Query::Or(left, right) => {
                let mut slots: Vec<u64> = self.execute_query(left, usize::MAX)?;
                let right_slots = self.execute_query(right, usize::MAX)?;
                let existing: HashSet<u64> = slots.iter().copied().collect();
                for s in right_slots {
                    if !existing.contains(&s) {
                        slots.push(s);
                    }
                }
                Ok(slots)
            }
        }
    }

    /// Find all paths between two nodes using DFS, up to max_depth hops.
    /// Returns paths as vectors of node labels.
    ///
    /// Optional filters:
    /// - `min_depth`: skip paths with fewer hops than this value
    /// - `via_label`: only return paths that pass through the named node
    pub fn find_all_paths(
        &self,
        from_label: &str,
        to_label: &str,
        max_depth: u32,
        min_depth: Option<u32>,
        via_label: Option<&str>,
        skip_types: Option<&[&str]>,
    ) -> Result<Vec<Vec<String>>> {
        let from_id = self.find_node_id(from_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;
        let to_id = self.find_node_id(to_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;

        // Resolve via node ID if provided
        let via_id: Option<u64> = match via_label {
            Some(label) => self.find_node_id(label)?,
            None => None,
        };

        if from_id == to_id {
            return Ok(vec![vec![from_label.to_string()]]);
        }

        let min_d = min_depth.unwrap_or(0);

        let mut all_paths = Vec::new();
        // DFS with path tracking (bounded by max_depth)
        let mut stack: Vec<(u64, Vec<u64>)> = vec![(from_id, vec![from_id])];

        while let Some((current, path)) = stack.pop() {
            if path.len() as u32 > max_depth + 1 {
                continue;
            }

            // Get neighbors (both directions)
            let mut neighbors = Vec::new();
            if let Some(edge_slots) = self.adj_out.get(&current) {
                for &edge_slot in edge_slots {
                    if let Ok(edge) = self.brain.read_edge(edge_slot) {
                        if !edge.is_deleted() {
                            neighbors.push(edge.to_node);
                        }
                    }
                }
            }
            if let Some(edge_slots) = self.adj_in.get(&current) {
                for &edge_slot in edge_slots {
                    if let Ok(edge) = self.brain.read_edge(edge_slot) {
                        if !edge.is_deleted() {
                            neighbors.push(edge.from_node);
                        }
                    }
                }
            }

            for neighbor in neighbors {
                if neighbor == to_id {
                    // Found a path!
                    let mut full_path = path.clone();
                    full_path.push(to_id);
                    let hops = full_path.len() - 1;

                    // Apply min_depth filter
                    if (hops as u32) < min_d {
                        continue;
                    }

                    // Apply via filter: path must contain the via node
                    if let Some(vid) = via_id {
                        if !full_path.contains(&vid) {
                            continue;
                        }
                    }

                    // Convert IDs to labels
                    let label_path: Vec<String> = full_path.iter()
                        .filter_map(|&id| self.label_for_id(id).ok())
                        .collect();
                    if label_path.len() == full_path.len() {
                        all_paths.push(label_path);
                    }
                    // Cap at 20 paths to avoid combinatorial explosion
                    if all_paths.len() >= 20 {
                        return Ok(all_paths);
                    }
                } else if !path.contains(&neighbor) && (path.len() as u32) < max_depth + 1 {
                    // Skip intermediary nodes of excluded types (but never skip the destination)
                    if let Some(types) = skip_types {
                        if let Some(slot) = self.find_slot_by_id(neighbor) {
                            if let Ok(node) = self.brain.read_node(slot) {
                                if let Some(type_name) = self.node_type_names.get(node.node_type as usize) {
                                    if types.iter().any(|t| t.eq_ignore_ascii_case(type_name)) {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    let mut new_path = path.clone();
                    new_path.push(neighbor);
                    stack.push((neighbor, new_path));
                }
            }
        }

        Ok(all_paths)
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use tempfile::TempDir;

    fn test_provenance() -> Provenance {
        Provenance::user("test")
    }

    #[test]
    fn test_find_all_paths_basic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = Graph::create(&path).unwrap();
        let prov = test_provenance();
        // Create A -> B -> C and A -> D -> C
        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();
        graph.store("D", &prov).unwrap();
        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();
        graph.relate("A", "D", "knows", &prov).unwrap();
        graph.relate("D", "C", "knows", &prov).unwrap();

        let paths = graph.find_all_paths("A", "C", 5, None, None, None).unwrap();
        assert!(paths.len() >= 2, "should find at least 2 paths, got {}", paths.len());
        for path in &paths {
            assert_eq!(path.first().unwrap(), "A");
            assert_eq!(path.last().unwrap(), "C");
        }
    }

    #[test]
    fn test_find_all_paths_no_path() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = Graph::create(&path).unwrap();
        let prov = test_provenance();
        graph.store("X", &prov).unwrap();
        graph.store("Y", &prov).unwrap();
        // No edges between X and Y

        let paths = graph.find_all_paths("X", "Y", 5, None, None, None).unwrap();
        assert!(paths.is_empty(), "disconnected nodes should return no paths");
    }

    #[test]
    fn test_find_paths_with_via() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = Graph::create(&path).unwrap();
        let prov = test_provenance();
        // Create A -> B -> C and A -> D -> C
        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();
        graph.store("D", &prov).unwrap();
        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();
        graph.relate("A", "D", "knows", &prov).unwrap();
        graph.relate("D", "C", "knows", &prov).unwrap();

        // Via B should only return paths through B
        let paths = graph.find_all_paths("A", "C", 5, None, Some("B"), None).unwrap();
        assert!(!paths.is_empty(), "should find at least 1 path via B");
        for path in &paths {
            assert!(path.contains(&"B".to_string()), "every path must contain B");
        }

        // Via D should only return paths through D
        let paths_d = graph.find_all_paths("A", "C", 5, None, Some("D"), None).unwrap();
        assert!(!paths_d.is_empty(), "should find at least 1 path via D");
        for path in &paths_d {
            assert!(path.contains(&"D".to_string()), "every path must contain D");
        }
    }

    #[test]
    fn test_find_paths_with_min_depth() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = Graph::create(&path).unwrap();
        let prov = test_provenance();
        // Create A -> B -> C and A -> C (direct)
        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();
        graph.relate("A", "C", "knows", &prov).unwrap();
        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();

        // Without min_depth, should find both 1-hop and 2-hop paths
        let all_paths = graph.find_all_paths("A", "C", 5, None, None, None).unwrap();
        assert!(all_paths.len() >= 2, "should find at least 2 paths, got {}", all_paths.len());

        // min_depth=2 should skip the direct A->C path (1 hop)
        let filtered = graph.find_all_paths("A", "C", 5, Some(2), None, None).unwrap();
        for path in &filtered {
            let hops = path.len() - 1;
            assert!(hops >= 2, "path should have at least 2 hops, got {}", hops);
        }
        assert!(!filtered.is_empty(), "should find at least 1 path with >= 2 hops");
    }

    #[test]
    fn test_find_paths_skip_types() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = Graph::create(&path).unwrap();
        let prov = test_provenance();
        // Create A -> FactNode -> B  (FactNode is type "Fact")
        graph.store("A", &prov).unwrap();
        graph.store("FactNode", &prov).unwrap();
        let _ = graph.set_node_type("FactNode", "Fact");
        graph.store("B", &prov).unwrap();
        graph.relate("A", "FactNode", "mentions", &prov).unwrap();
        graph.relate("FactNode", "B", "about", &prov).unwrap();

        // Without skip: should find path A -> FactNode -> B
        let paths = graph.find_all_paths("A", "B", 5, None, None, None).unwrap();
        assert!(!paths.is_empty(), "should find path without skip_types");

        // With skip_types=["Fact"]: FactNode is intermediary, should be skipped -> no path
        let paths_skip = graph.find_all_paths("A", "B", 5, None, None, Some(&["Fact"])).unwrap();
        assert!(paths_skip.is_empty(), "should find no path when Fact nodes are skipped");

        // skip_types should never skip the destination
        let paths_to_fact = graph.find_all_paths("A", "FactNode", 5, None, None, Some(&["Fact"])).unwrap();
        assert!(!paths_to_fact.is_empty(), "should still find path when destination is a Fact");
    }
}
