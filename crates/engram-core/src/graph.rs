/// High-level graph API wrapping the storage engine.
///
/// Maintains in-memory adjacency indexes for fast traversal,
/// a hash index for node lookup by label, a property store
/// for key-value metadata, and a persisted edge type registry.

use crate::index::hash::HashIndex;
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
        })
    }

    /// Open an existing graph.
    pub fn open(path: &Path) -> Result<Self> {
        let brain = BrainFile::open(path)?;
        let type_registry = TypeRegistry::load(path)?;
        let props = PropertyStore::load(path)?;
        let mut graph = Graph {
            brain,
            label_index: HashIndex::new(),
            adj_out: HashMap::new(),
            adj_in: HashMap::new(),
            type_registry,
            props,
        };
        graph.rebuild_indexes()?;
        Ok(graph)
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

    /// Get the edge type name for an edge type ID.
    pub fn edge_type_name(&self, type_id: u32) -> String {
        self.type_registry.name_or_default(type_id)
    }

    /// Get stats: (node_count, edge_count)
    pub fn stats(&self) -> (u64, u64) {
        self.brain.stats()
    }

    /// Flush and checkpoint everything: mmap, WAL, types, properties.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.brain.checkpoint()?;
        self.type_registry.flush()?;
        self.props.flush()?;
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

    fn rebuild_indexes(&mut self) -> Result<()> {
        let (node_count, edge_count) = self.brain.stats();

        self.label_index = HashIndex::with_capacity(node_count as usize);
        self.adj_out = HashMap::with_capacity(node_count as usize);
        self.adj_in = HashMap::with_capacity(node_count as usize);

        // Rebuild label index
        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                self.label_index.insert(node.label_hash, slot);
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
}
