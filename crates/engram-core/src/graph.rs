/// High-level graph API wrapping the storage engine.
///
/// Maintains in-memory adjacency indexes for fast traversal,
/// a hash index for node lookup by label, a property store
/// for key-value metadata, and a persisted edge type registry.

use crate::index::bitmap::BitmapIndex;
use crate::index::embedding::Embedder;
use crate::index::fulltext::{FullTextIndex, SearchHit};
use crate::index::hash::HashIndex;
use crate::index::hnsw::HnswIndex;
use crate::index::hybrid;
use crate::index::query::{self, CmpOp, Query};
use crate::index::temporal::{TemporalIndex, TimeAxis};
use crate::storage::brain_file::BrainFile;
use crate::storage::error::{Result, StorageError};
use crate::storage::node::{hash_label, Node};
use crate::storage::props::PropertyStore;
use crate::storage::type_registry::TypeRegistry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// Source type for provenance tracking
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum SourceType {
    User = 0,
    Sensor = 1,
    Llm = 2,
    Api = 3,
    Derived = 4,
    Correction = 5,
}

/// Provenance information attached to mutations
#[derive(Debug, Clone)]
pub struct Provenance {
    pub source_type: SourceType,
    pub source_id: String,
}

impl Provenance {
    pub fn user(id: &str) -> Self {
        Provenance {
            source_type: SourceType::User,
            source_id: id.to_string(),
        }
    }

    pub fn to_source_hash(&self) -> u64 {
        hash_label(&self.source_id)
    }
}

/// Result of a graph traversal
#[derive(Debug)]
pub struct TraversalResult {
    /// Node IDs visited, in BFS order
    pub nodes: Vec<u64>,
    /// Edges traversed (from_node_id, to_node_id, edge_slot)
    pub edges: Vec<(u64, u64, u64)>,
    /// Depth at which each node was found
    pub depths: HashMap<u64, u32>,
}

/// The main graph interface
pub struct Graph {
    brain: BrainFile,
    /// label_hash -> node slots
    label_index: HashIndex,
    /// node_id -> list of outgoing edge slots
    adj_out: HashMap<u64, Vec<u64>>,
    /// node_id -> list of incoming edge slots
    adj_in: HashMap<u64, Vec<u64>>,
    /// Persisted edge type registry
    type_registry: TypeRegistry,
    /// Persisted property store
    props: PropertyStore,
    /// Full-text BM25 index on labels + properties
    fulltext: FullTextIndex,
    /// Temporal index for bi-temporal range queries
    temporal: TemporalIndex,
    /// Bitmap index for node_type filtering
    type_bitmap: BitmapIndex,
    /// Bitmap index for memory_tier filtering
    tier_bitmap: BitmapIndex,
    /// Bitmap index for sensitivity filtering
    sensitivity_bitmap: BitmapIndex,
    /// Node type name registry (like edge types but for nodes)
    node_type_names: Vec<String>,
    node_type_lookup: HashMap<String, u32>,
    /// HNSW vector index for nearest-neighbor search
    hnsw: HnswIndex,
    /// Optional embedding model for automatic text-to-vector conversion
    embedder: Option<Box<dyn Embedder>>,
}

impl Graph {
    /// Create a new graph with a fresh .brain file.
    pub fn create(path: &Path) -> Result<Self> {
        Self::create_with_capacity(path, 1024, 4096)
    }

    pub fn create_with_capacity(path: &Path, nodes: u64, edges: u64) -> Result<Self> {
        let brain = BrainFile::create_with_capacity(path, nodes, edges)?;
        Ok(Graph {
            brain,
            label_index: HashIndex::new(),
            adj_out: HashMap::new(),
            adj_in: HashMap::new(),
            type_registry: TypeRegistry::new(path),
            props: PropertyStore::new(path),
            fulltext: FullTextIndex::new(),
            temporal: TemporalIndex::new(),
            type_bitmap: BitmapIndex::new(),
            tier_bitmap: BitmapIndex::new(),
            sensitivity_bitmap: BitmapIndex::new(),
            node_type_names: Vec::new(),
            node_type_lookup: HashMap::new(),
            hnsw: HnswIndex::new(path),
            embedder: None,
        })
    }

    /// Open an existing graph.
    pub fn open(path: &Path) -> Result<Self> {
        let brain = BrainFile::open(path)?;
        let type_registry = TypeRegistry::load(path)?;
        let props = PropertyStore::load(path)?;
        let hnsw = HnswIndex::load(path);
        let mut graph = Graph {
            brain,
            label_index: HashIndex::new(),
            adj_out: HashMap::new(),
            adj_in: HashMap::new(),
            type_registry,
            props,
            fulltext: FullTextIndex::new(),
            temporal: TemporalIndex::new(),
            type_bitmap: BitmapIndex::new(),
            tier_bitmap: BitmapIndex::new(),
            sensitivity_bitmap: BitmapIndex::new(),
            node_type_names: Vec::new(),
            node_type_lookup: HashMap::new(),
            hnsw,
            embedder: None,
        };
        graph.rebuild_indexes()?;
        Ok(graph)
    }

    /// Set the embedding model for automatic vector generation.
    pub fn set_embedder(&mut self, embedder: Box<dyn Embedder>) {
        self.embedder = Some(embedder);
    }

    /// Store a new node with a label. Returns node ID.
    pub fn store(&mut self, label: &str, provenance: &Provenance) -> Result<u64> {
        let node_id = self.brain.store_node(label)?;
        let (node_count, _) = self.brain.stats();
        let slot = node_count - 1;

        // Update source_id from provenance
        self.brain
            .update_node_field(slot, |n| n.source_id = provenance.to_source_hash())?;

        self.label_index.insert(hash_label(label), slot);

        // Update search indexes
        let node = self.brain.read_node(slot)?;
        self.fulltext.add_document(slot, label);
        self.temporal.insert(slot, node.created_at, node.event_time);
        self.type_bitmap.insert(node.node_type, slot);
        self.tier_bitmap.insert(node.memory_tier as u32, slot);
        self.sensitivity_bitmap.insert(node.sensitivity as u32, slot);

        // Auto-embed if an embedder is configured
        if let Some(ref embedder) = self.embedder {
            if let Ok(vec) = embedder.embed(label) {
                self.hnsw.insert(slot, vec);
            }
        }

        Ok(node_id)
    }

    /// Store a node with explicit confidence.
    pub fn store_with_confidence(
        &mut self,
        label: &str,
        confidence: f32,
        provenance: &Provenance,
    ) -> Result<u64> {
        let node_id = self.store(label, provenance)?;
        let (node_count, _) = self.brain.stats();
        let slot = node_count - 1;
        self.brain.update_node_field(slot, |n| {
            n.confidence = confidence.clamp(0.0, 1.0);
        })?;
        Ok(node_id)
    }

    /// Create a relationship between two nodes. Returns edge ID.
    pub fn relate(
        &mut self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        provenance: &Provenance,
    ) -> Result<u64> {
        let from_node = self.find_node_id(from_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;
        let to_node = self.find_node_id(to_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;

        let edge_type = self.type_registry.get_or_create(relationship);
        let edge_id = self.brain.store_edge(from_node, to_node, edge_type)?;

        let (_, edge_count) = self.brain.stats();
        let edge_slot = edge_count - 1;

        // Update edge source_id
        self.brain
            .update_edge_field(edge_slot, |e| e.source_id = provenance.to_source_hash())?;

        self.adj_out.entry(from_node).or_default().push(edge_slot);
        self.adj_in.entry(to_node).or_default().push(edge_slot);

        Ok(edge_id)
    }

    /// Create a relationship with explicit confidence.
    pub fn relate_with_confidence(
        &mut self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        confidence: f32,
        provenance: &Provenance,
    ) -> Result<u64> {
        let edge_id = self.relate(from_label, to_label, relationship, provenance)?;
        let (_, edge_count) = self.brain.stats();
        let edge_slot = edge_count - 1;
        self.brain.update_edge_field(edge_slot, |e| {
            e.confidence = confidence.clamp(0.0, 1.0);
        })?;
        Ok(edge_id)
    }

    /// BFS traversal from a starting node, up to max_depth hops.
    pub fn traverse(
        &self,
        start_label: &str,
        max_depth: u32,
        min_confidence: f32,
    ) -> Result<TraversalResult> {
        let start_id = self.find_node_id(start_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;

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

            if let Some(edge_slots) = self.adj_out.get(&node_id) {
                for &edge_slot in edge_slots {
                    let edge = self.brain.read_edge(edge_slot)?;
                    if edge.confidence < min_confidence {
                        continue;
                    }

                    let target = edge.to_node;

                    // Check target node is still active
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

        Ok(result)
    }

    /// Soft-delete a node by label. Sets confidence to 0, marks as deleted.
    pub fn delete(&mut self, label: &str, provenance: &Provenance) -> Result<bool> {
        let target_hash = hash_label(label);
        let slots = self.label_index.get(target_hash).to_vec();

        for slot in slots {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && node.label() == label {
                let now = current_timestamp();
                self.brain.update_node_field(slot, |n| {
                    n.soft_delete(now);
                    n.source_id = provenance.to_source_hash();
                })?;
                self.label_index.remove(target_hash, slot);
                self.props.remove_all(slot);
                self.fulltext.remove_document(slot);
                self.temporal.remove(slot);
                self.hnsw.remove(slot);
                return Ok(true);
            }
        }
        Ok(false)
    }

    // --- Property operations ---

    /// Set a property on a node.
    pub fn set_property(&mut self, label: &str, key: &str, value: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        self.props.set(slot, key, value);
        // Re-index: rebuild fulltext for this slot with label + all prop values
        self.reindex_fulltext(slot)?;
        Ok(true)
    }

    /// Get a property value from a node.
    pub fn get_property(&self, label: &str, key: &str) -> Result<Option<String>> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(None),
        };
        Ok(self.props.get(slot, key).map(|s| s.to_string()))
    }

    /// Get all properties for a node.
    pub fn get_properties(&self, label: &str) -> Result<Option<HashMap<String, String>>> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(None),
        };
        Ok(self.props.get_all(slot).cloned())
    }

    // --- Query helpers ---

    /// Find a node by label, return its ID.
    pub fn find_node_id(&self, label: &str) -> Result<Option<u64>> {
        let target_hash = hash_label(label);
        for &slot in self.label_index.get(target_hash) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && node.label() == label {
                return Ok(Some(node.id));
            }
        }
        Ok(None)
    }

    /// Get a node by label.
    pub fn get_node(&self, label: &str) -> Result<Option<&Node>> {
        let target_hash = hash_label(label);
        for &slot in self.label_index.get(target_hash) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && node.label() == label {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    /// Get all outgoing edges from a node.
    pub fn edges_from(&self, label: &str) -> Result<Vec<EdgeView>> {
        let node_id = match self.find_node_id(label)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let mut edges = Vec::new();
        if let Some(edge_slots) = self.adj_out.get(&node_id) {
            for &slot in edge_slots {
                let edge = self.brain.read_edge(slot)?;
                let target_label = self.label_for_id(edge.to_node)?;
                let rel_name = self.type_registry.name_or_default(edge.edge_type);
                edges.push(EdgeView {
                    from: label.to_string(),
                    to: target_label,
                    relationship: rel_name,
                    confidence: edge.confidence,
                });
            }
        }
        Ok(edges)
    }

    /// Get all incoming edges to a node.
    pub fn edges_to(&self, label: &str) -> Result<Vec<EdgeView>> {
        let node_id = match self.find_node_id(label)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let mut edges = Vec::new();
        if let Some(edge_slots) = self.adj_in.get(&node_id) {
            for &slot in edge_slots {
                let edge = self.brain.read_edge(slot)?;
                let source_label = self.label_for_id(edge.from_node)?;
                let rel_name = self.type_registry.name_or_default(edge.edge_type);
                edges.push(EdgeView {
                    from: source_label,
                    to: label.to_string(),
                    relationship: rel_name,
                    confidence: edge.confidence,
                });
            }
        }
        Ok(edges)
    }

    // --- Vector operations ---

    /// Store an embedding vector for a node slot directly.
    pub fn store_vector(&mut self, label: &str, vector: Vec<f32>) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        self.hnsw.insert(slot, vector);
        Ok(true)
    }

    /// Embed a node's text using the configured embedder and store the vector.
    pub fn embed_node(&mut self, label: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };

        let embedder = match &self.embedder {
            Some(e) => e,
            None => return Err(StorageError::InvalidFile {
                reason: "no embedder configured".into(),
            }),
        };

        // Build text: label + all property values
        let node = self.brain.read_node(slot)?;
        let mut text = node.label().to_string();
        if let Some(props) = self.props.get_all(slot) {
            for (k, v) in props {
                text.push(' ');
                text.push_str(k);
                text.push(' ');
                text.push_str(v);
            }
        }

        let vector = embedder.embed(&text).map_err(|e| StorageError::InvalidFile {
            reason: e.to_string(),
        })?;
        self.hnsw.insert(slot, vector);
        Ok(true)
    }

    /// Search by vector similarity (nearest neighbors).
    pub fn search_vector(&self, query_vector: &[f32], limit: usize) -> Result<Vec<NodeSearchResult>> {
        let hits = self.hnsw.search(query_vector, limit);
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let node = self.brain.read_node(hit.slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot: hit.slot,
                    node_id: node.id,
                    label: node.label().to_string(),
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
                    label: node.label().to_string(),
                    confidence: node.confidence,
                    score: hit.score,
                });
            }
        }
        Ok(results)
    }

    /// Hybrid search using text only — embeds the query automatically if an embedder is set.
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

    // --- Search operations ---

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
                        label: node.label().to_string(),
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
                    label: node.label().to_string(),
                    confidence: node.confidence,
                    score: 0.0,
                });
            }
        }
        Ok(results)
    }

    /// Set a node's type by name.
    pub fn set_node_type(&mut self, label: &str, type_name: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let old_node = self.brain.read_node(slot)?;
        let old_type = old_node.node_type;
        let new_type = self.get_or_create_node_type(type_name);

        self.type_bitmap.remove(old_type, slot);
        self.brain.update_node_field(slot, |n| n.node_type = new_type)?;
        self.type_bitmap.insert(new_type, slot);
        Ok(true)
    }

    /// Get the edge type name for an edge type ID.
    pub fn edge_type_name(&self, type_id: u32) -> String {
        self.type_registry.name_or_default(type_id)
    }

    /// Get stats: (node_count, edge_count)
    pub fn stats(&self) -> (u64, u64) {
        self.brain.stats()
    }

    /// Flush and checkpoint everything: mmap, WAL, types, properties, vectors.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.brain.checkpoint()?;
        self.type_registry.flush()?;
        self.props.flush()?;
        self.hnsw.flush().map_err(|e| StorageError::InvalidFile {
            reason: format!("vector flush failed: {e}"),
        })?;
        Ok(())
    }

    // --- Private helpers ---

    fn find_slot_by_label(&self, label: &str) -> Result<Option<u64>> {
        let target_hash = hash_label(label);
        for &slot in self.label_index.get(target_hash) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && node.label() == label {
                return Ok(Some(slot));
            }
        }
        Ok(None)
    }

    fn find_slot_by_id(&self, node_id: u64) -> Option<u64> {
        // node IDs start at 1, slots start at 0
        let slot = node_id.checked_sub(1)?;
        let (count, _) = self.brain.stats();
        if slot < count {
            Some(slot)
        } else {
            None
        }
    }

    fn label_for_id(&self, node_id: u64) -> Result<String> {
        if let Some(slot) = self.find_slot_by_id(node_id) {
            let node = self.brain.read_node(slot)?;
            Ok(node.label().to_string())
        } else {
            Ok(format!("node_{node_id}"))
        }
    }

    fn get_or_create_node_type(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.node_type_lookup.get(name) {
            return id;
        }
        let id = self.node_type_names.len() as u32;
        self.node_type_names.push(name.to_string());
        self.node_type_lookup.insert(name.to_string(), id);
        id
    }

    fn node_type_id(&self, name: &str) -> Option<u32> {
        self.node_type_lookup.get(name).copied()
    }

    fn reindex_fulltext(&mut self, slot: u64) -> Result<()> {
        self.fulltext.remove_document(slot);
        let node = self.brain.read_node(slot)?;
        let mut text = node.label().to_string();
        if let Some(props) = self.props.get_all(slot) {
            for (k, v) in props {
                text.push(' ');
                text.push_str(k);
                text.push(' ');
                text.push_str(v);
            }
        }
        self.fulltext.add_document(slot, &text);
        Ok(())
    }

    fn hits_to_results(&self, hits: &[SearchHit]) -> Result<Vec<NodeSearchResult>> {
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let node = self.brain.read_node(hit.slot)?;
            if node.is_active() {
                results.push(NodeSearchResult {
                    slot: hit.slot,
                    node_id: node.id,
                    label: node.label().to_string(),
                    confidence: node.confidence,
                    score: hit.score,
                });
            }
        }
        Ok(results)
    }

    fn execute_query(&self, query: &Query, limit: usize) -> Result<Vec<u64>> {
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

    fn rebuild_indexes(&mut self) -> Result<()> {
        let (node_count, edge_count) = self.brain.stats();

        self.label_index = HashIndex::with_capacity(node_count as usize);
        self.adj_out = HashMap::with_capacity(node_count as usize);
        self.adj_in = HashMap::with_capacity(node_count as usize);
        self.fulltext = FullTextIndex::new();
        self.temporal = TemporalIndex::with_capacity(node_count as usize);
        self.type_bitmap = BitmapIndex::new();
        self.tier_bitmap = BitmapIndex::new();
        self.sensitivity_bitmap = BitmapIndex::new();

        // Rebuild all node indexes
        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                self.label_index.insert(node.label_hash, slot);
                self.temporal.insert(slot, node.created_at, node.event_time);
                self.type_bitmap.insert(node.node_type, slot);
                self.tier_bitmap.insert(node.memory_tier as u32, slot);
                self.sensitivity_bitmap.insert(node.sensitivity as u32, slot);

                // Fulltext: label + properties
                let mut text = node.label().to_string();
                if let Some(props) = self.props.get_all(slot) {
                    for (k, v) in props {
                        text.push(' ');
                        text.push_str(k);
                        text.push(' ');
                        text.push_str(v);
                    }
                }
                self.fulltext.add_document(slot, &text);
            }
        }

        // Rebuild adjacency lists
        for edge_slot in 0..edge_count {
            let edge = self.brain.read_edge(edge_slot)?;
            self.adj_out
                .entry(edge.from_node)
                .or_default()
                .push(edge_slot);
            self.adj_in
                .entry(edge.to_node)
                .or_default()
                .push(edge_slot);
        }

        Ok(())
    }
}

/// A search result
#[derive(Debug, Clone)]
pub struct NodeSearchResult {
    pub slot: u64,
    pub node_id: u64,
    pub label: String,
    pub confidence: f32,
    pub score: f64,
}

impl std::fmt::Display for NodeSearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.score > 0.0 {
            write!(f, "{} (id: {}, confidence: {:.2}, score: {:.3})", self.label, self.node_id, self.confidence, self.score)
        } else {
            write!(f, "{} (id: {}, confidence: {:.2})", self.label, self.node_id, self.confidence)
        }
    }
}

/// A human-readable view of an edge
#[derive(Debug, Clone)]
pub struct EdgeView {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f32,
}

impl std::fmt::Display for EdgeView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -[{}]-> {} (confidence: {:.2})",
            self.from, self.relationship, self.to, self.confidence
        )
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_provenance() -> Provenance {
        Provenance::user("test")
    }

    #[test]
    fn store_and_find_node() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();

        let id = g.store("server-01", &test_provenance()).unwrap();
        assert_eq!(id, 1);

        let found = g.find_node_id("server-01").unwrap();
        assert_eq!(found, Some(1));

        let missing = g.find_node_id("server-99").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn relate_and_query_edges() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.store("postgresql", &prov).unwrap();
        g.relate("server-01", "postgresql", "runs", &prov).unwrap();

        let edges = g.edges_from("server-01").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, "postgresql");
        assert_eq!(edges[0].relationship, "runs");

        let edges_in = g.edges_to("postgresql").unwrap();
        assert_eq!(edges_in.len(), 1);
        assert_eq!(edges_in[0].from, "server-01");
    }

    #[test]
    fn multi_hop_traversal() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.store("c", &prov).unwrap();
        g.store("d", &prov).unwrap();

        g.relate("a", "b", "connects", &prov).unwrap();
        g.relate("b", "c", "connects", &prov).unwrap();
        g.relate("c", "d", "connects", &prov).unwrap();

        // 1-hop from a: should find b
        let result = g.traverse("a", 1, 0.0).unwrap();
        assert!(result.nodes.contains(&1)); // a
        assert!(result.nodes.contains(&2)); // b
        assert!(!result.nodes.contains(&3)); // c not reachable in 1 hop

        // 3-hop from a: should find everything
        let result = g.traverse("a", 3, 0.0).unwrap();
        assert_eq!(result.nodes.len(), 4);

        // Check depths
        assert_eq!(result.depths[&1], 0); // a at depth 0
        assert_eq!(result.depths[&2], 1); // b at depth 1
        assert_eq!(result.depths[&3], 2); // c at depth 2
        assert_eq!(result.depths[&4], 3); // d at depth 3
    }

    #[test]
    fn traversal_respects_min_confidence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.store("c", &prov).unwrap();

        g.relate_with_confidence("a", "b", "strong", 0.9, &prov).unwrap();
        g.relate_with_confidence("a", "c", "weak", 0.2, &prov).unwrap();

        let result = g.traverse("a", 1, 0.5).unwrap();
        assert!(result.nodes.contains(&2)); // b (0.9 >= 0.5)
        assert!(!result.nodes.contains(&3)); // c (0.2 < 0.5)
    }

    #[test]
    fn soft_delete_removes_from_index() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("to-delete", &prov).unwrap();
        assert!(g.find_node_id("to-delete").unwrap().is_some());

        let deleted = g.delete("to-delete", &prov).unwrap();
        assert!(deleted);

        assert!(g.find_node_id("to-delete").unwrap().is_none());
    }

    #[test]
    fn soft_delete_excludes_from_traversal() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.store("c", &prov).unwrap();
        g.relate("a", "b", "link", &prov).unwrap();
        g.relate("a", "c", "link", &prov).unwrap();

        g.delete("b", &prov).unwrap();

        let result = g.traverse("a", 1, 0.0).unwrap();
        assert!(!result.nodes.contains(&2)); // b is deleted
        assert!(result.nodes.contains(&3)); // c is still reachable
    }

    #[test]
    fn persistence_with_indexes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let prov = Provenance::user("test");

        {
            let mut g = Graph::create(&path).unwrap();
            g.store("server-01", &prov).unwrap();
            g.store("postgresql", &prov).unwrap();
            g.relate("server-01", "postgresql", "runs", &prov).unwrap();
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            let found = g.find_node_id("server-01").unwrap();
            assert!(found.is_some());

            let edges = g.edges_from("server-01").unwrap();
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].to, "postgresql");
            assert_eq!(edges[0].relationship, "runs"); // type name persisted!
        }
    }

    #[test]
    fn get_node_returns_details() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store_with_confidence("my-node", 0.95, &prov).unwrap();

        let node = g.get_node("my-node").unwrap().unwrap();
        assert_eq!(node.label(), "my-node");
        assert_eq!(node.confidence, 0.95);
    }

    #[test]
    fn property_set_and_get() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.set_property("server-01", "role", "database").unwrap();
        g.set_property("server-01", "os", "linux").unwrap();

        assert_eq!(g.get_property("server-01", "role").unwrap(), Some("database".to_string()));
        assert_eq!(g.get_property("server-01", "os").unwrap(), Some("linux".to_string()));
        assert_eq!(g.get_property("server-01", "missing").unwrap(), None);
        assert_eq!(g.get_property("nonexistent", "role").unwrap(), None);
    }

    #[test]
    fn property_get_all() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("node-a", &prov).unwrap();
        g.set_property("node-a", "key1", "val1").unwrap();
        g.set_property("node-a", "key2", "val2").unwrap();

        let all = g.get_properties("node-a").unwrap().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["key1"], "val1");
        assert_eq!(all["key2"], "val2");
    }

    #[test]
    fn property_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let prov = Provenance::user("test");

        {
            let mut g = Graph::create(&path).unwrap();
            g.store("server-01", &prov).unwrap();
            g.set_property("server-01", "role", "web").unwrap();
            g.set_property("server-01", "port", "8080").unwrap();
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            assert_eq!(g.get_property("server-01", "role").unwrap(), Some("web".to_string()));
            assert_eq!(g.get_property("server-01", "port").unwrap(), Some("8080".to_string()));
        }
    }

    #[test]
    fn delete_cleans_up_properties() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("temp", &prov).unwrap();
        g.set_property("temp", "key", "value").unwrap();
        g.delete("temp", &prov).unwrap();

        // Properties are cleaned up on delete
        // (node is gone from index, props removed)
        assert_eq!(g.get_property("temp", "key").unwrap(), None);
    }

    #[test]
    fn fulltext_search_labels() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("postgresql-primary", &prov).unwrap();
        g.store("postgresql-replica", &prov).unwrap();
        g.store("nginx-proxy", &prov).unwrap();

        let results = g.search_text("postgresql", 10).unwrap();
        assert_eq!(results.len(), 2);
        let labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"postgresql-primary"));
        assert!(labels.contains(&"postgresql-replica"));
    }

    #[test]
    fn fulltext_search_properties() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.store("server-02", &prov).unwrap();
        g.set_property("server-01", "role", "database").unwrap();
        g.set_property("server-02", "role", "webserver").unwrap();

        let results = g.search_text("database", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "server-01");
    }

    #[test]
    fn query_by_confidence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store_with_confidence("high", 0.95, &prov).unwrap();
        g.store_with_confidence("low", 0.3, &prov).unwrap();
        g.store_with_confidence("mid", 0.6, &prov).unwrap();

        let results = g.search("confidence>0.5", 10).unwrap();
        assert_eq!(results.len(), 2);
        let labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"high"));
        assert!(labels.contains(&"mid"));
    }

    #[test]
    fn query_by_property() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.set_property("a", "env", "prod").unwrap();
        g.set_property("b", "env", "staging").unwrap();

        let results = g.search("prop:env=prod", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "a");
    }

    #[test]
    fn query_by_tier() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("node-a", &prov).unwrap();
        g.store("node-b", &prov).unwrap();
        // Default tier is active(1)

        let results = g.search("tier:active", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = g.search("tier:core", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn query_and_combination() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store_with_confidence("good-a", 0.9, &prov).unwrap();
        g.store_with_confidence("bad-b", 0.2, &prov).unwrap();
        g.set_property("good-a", "env", "prod").unwrap();
        g.set_property("bad-b", "env", "prod").unwrap();

        let results = g.search("prop:env=prod AND confidence>0.5", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "good-a");
    }

    #[test]
    fn search_persists_across_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let prov = Provenance::user("test");

        {
            let mut g = Graph::create(&path).unwrap();
            g.store("postgresql", &prov).unwrap();
            g.set_property("postgresql", "role", "database").unwrap();
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            let results = g.search_text("database", 10).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].label, "postgresql");
        }
    }

    #[test]
    fn vector_store_and_search() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("cat", &prov).unwrap();
        g.store("dog", &prov).unwrap();
        g.store("car", &prov).unwrap();

        // Manually assign vectors — cat and dog are similar, car is different
        g.store_vector("cat", vec![1.0, 0.0, 0.0, 0.1]).unwrap();
        g.store_vector("dog", vec![0.9, 0.1, 0.0, 0.1]).unwrap();
        g.store_vector("car", vec![0.0, 0.0, 1.0, 0.0]).unwrap();

        let results = g.search_vector(&[1.0, 0.0, 0.0, 0.1], 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].label, "cat"); // exact match
        assert_eq!(results[1].label, "dog"); // similar
    }

    #[test]
    fn hybrid_search_combines_sources() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("database-server", &prov).unwrap();
        g.store("web-server", &prov).unwrap();
        g.store("cache-server", &prov).unwrap();

        // Vectors: database-server and cache-server are similar
        g.store_vector("database-server", vec![1.0, 0.0, 0.0]).unwrap();
        g.store_vector("web-server", vec![0.0, 1.0, 0.0]).unwrap();
        g.store_vector("cache-server", vec![0.9, 0.1, 0.0]).unwrap();

        // Hybrid: keyword "database" + vector close to database-server
        let results = g.search_hybrid("database", &[1.0, 0.0, 0.0], 5).unwrap();
        assert!(!results.is_empty());
        // database-server should rank high (matches both keyword and vector)
        assert_eq!(results[0].label, "database-server");
    }

    struct TestEmbedder;
    impl crate::index::embedding::Embedder for TestEmbedder {
        fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, crate::index::embedding::EmbedError> {
            // Simple deterministic 4D embedding from text hash
            let hash = hash_label(text);
            let v: Vec<f32> = (0..4).map(|i| ((hash >> (i * 8)) & 0xFF) as f32 / 255.0).collect();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            Ok(v.iter().map(|x| x / norm.max(f32::EPSILON)).collect())
        }
        fn dim(&self) -> usize { 4 }
        fn model_id(&self) -> &str { "test-embedder" }
    }

    #[test]
    fn auto_embed_on_store() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        g.set_embedder(Box::new(TestEmbedder));
        let prov = test_provenance();

        g.store("hello-world", &prov).unwrap();
        g.store("goodbye-world", &prov).unwrap();

        // Should be able to search by vector since auto-embedding happened
        let query_vec = <TestEmbedder as crate::index::embedding::Embedder>::embed(&TestEmbedder, "hello-world").unwrap();
        let results = g.search_vector(&query_vec, 5).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn vector_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let prov = Provenance::user("test");

        {
            let mut g = Graph::create(&path).unwrap();
            g.store("node-a", &prov).unwrap();
            g.store_vector("node-a", vec![1.0, 0.0, 0.0]).unwrap();
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            let results = g.search_vector(&[1.0, 0.0, 0.0], 5).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].label, "node-a");
        }
    }

    #[test]
    fn delete_removes_vector() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("ephemeral", &prov).unwrap();
        g.store_vector("ephemeral", vec![1.0, 0.0, 0.0]).unwrap();
        g.delete("ephemeral", &prov).unwrap();

        let results = g.search_vector(&[1.0, 0.0, 0.0], 5).unwrap();
        assert!(results.is_empty() || results.iter().all(|r| r.label != "ephemeral"));
    }
}
