/// High-level graph API wrapping the storage engine.
///
/// Maintains in-memory adjacency indexes for fast traversal,
/// a hash index for node lookup by label, a property store
/// for key-value metadata, and a persisted edge type registry.

use crate::events::{EventBus, GraphEvent, ThresholdDirection};
use crate::index::bitmap::BitmapIndex;
use crate::index::embedding::Embedder;
use crate::index::fulltext::{FullTextIndex, SearchHit};
use crate::index::hash::HashIndex;
use crate::index::hnsw::HnswIndex;
use crate::index::hybrid;
use crate::index::query::{self, CmpOp, Query};
use crate::index::temporal::{TemporalIndex, TimeAxis};
use crate::learning::cooccurrence::CooccurrenceTracker;
use crate::learning::confidence::{confidence_cap, initial_confidence};
use crate::learning::contradiction::{self, ConflictCheckResult, ConflictKind, Contradiction};
use crate::learning::correction::{self, CorrectionResult};
use crate::learning::decay;
use crate::learning::evidence::{
    CooccurrenceEvidence, ContradictingFact, EnrichedResult, Evidence, SupportingFact,
};
use crate::learning::inference::{Bindings, InferenceResult, ProofResult, ProofStep, RuleFiring};
use crate::learning::reinforce;
use crate::learning::rules::{Action, ConfidenceExpr, Condition, ConditionOp, Rule};
use crate::learning::tier::{self, TierSweepResult};
use crate::storage::brain_file::BrainFile;
use crate::storage::error::{Result, StorageError};
use crate::storage::node::{hash_label, labels_eq, Node, LABEL_OVERFLOW_KEY};
use crate::storage::props::PropertyStore;
use crate::storage::type_registry::TypeRegistry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

mod store;
mod search;
mod learning;
mod reasoning;
mod index;
mod maintenance;

/// Resolve a rule variable or literal to a label string.
/// Quoted strings like `"Russia"` are literals -- return the unquoted value.
/// Unquoted strings like `Country` are variables -- look up in bindings.
pub(crate) fn resolve_var(var: &str, bindings: &Bindings) -> Option<String> {
    if var.starts_with('"') && var.ends_with('"') && var.len() >= 2 {
        Some(var[1..var.len()-1].to_string())
    } else {
        bindings.get(var).cloned()
    }
}

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
    pub(crate) brain: BrainFile,
    /// label_hash -> node slots
    pub(crate) label_index: HashIndex,
    /// node_id -> list of outgoing edge slots
    pub(crate) adj_out: HashMap<u64, Vec<u64>>,
    /// node_id -> list of incoming edge slots
    pub(crate) adj_in: HashMap<u64, Vec<u64>>,
    /// Persisted edge type registry
    pub(crate) type_registry: TypeRegistry,
    /// Persisted property store
    pub(crate) props: PropertyStore,
    /// Full-text BM25 index on labels + properties
    pub(crate) fulltext: FullTextIndex,
    /// Temporal index for bi-temporal range queries
    pub(crate) temporal: TemporalIndex,
    /// Bitmap index for node_type filtering
    pub(crate) type_bitmap: BitmapIndex,
    /// Bitmap index for memory_tier filtering
    pub(crate) tier_bitmap: BitmapIndex,
    /// Bitmap index for sensitivity filtering
    pub(crate) sensitivity_bitmap: BitmapIndex,
    /// Node type name registry (like edge types but for nodes)
    pub(crate) node_type_names: Vec<String>,
    pub(crate) node_type_lookup: HashMap<String, u32>,
    /// HNSW vector index for nearest-neighbor search
    pub(crate) hnsw: HnswIndex,
    /// Optional embedding model for automatic text-to-vector conversion
    pub(crate) embedder: Option<Box<dyn Embedder>>,
    /// Co-occurrence tracker for passive frequency statistics
    pub(crate) cooccurrence: CooccurrenceTracker,
    /// Source type per node slot (for confidence cap lookups)
    pub(crate) source_types: HashMap<u64, SourceType>,
    /// Compute planner for GPU/NPU-accelerated similarity search
    pub(crate) compute_planner: Option<engram_compute::planner::ComputePlanner>,
    /// Event bus for graph change notifications (v1.1.0)
    pub(crate) event_bus: Option<EventBus>,
    /// Confidence thresholds for ThresholdCrossed events
    pub(crate) confidence_thresholds: Vec<f32>,
}

impl Graph {
    /// Get the path of the underlying .brain file.
    pub fn path(&self) -> &Path {
        self.brain.path()
    }

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
            cooccurrence: CooccurrenceTracker::new(path),
            source_types: HashMap::new(),
            compute_planner: None,
            event_bus: None,
            confidence_thresholds: vec![0.3, 0.5, 0.7, 0.9],
        })
    }

    /// Open an existing graph.
    pub fn open(path: &Path) -> Result<Self> {
        let brain = BrainFile::open(path)?;
        let type_registry = TypeRegistry::load(path)?;
        let props = PropertyStore::load(path)?;
        let hnsw = HnswIndex::load(path);
        let cooccurrence = CooccurrenceTracker::load(path)
            .unwrap_or_else(|_| CooccurrenceTracker::new(path));
        let (node_type_names, node_type_lookup) = Self::load_node_type_names(path);
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
            node_type_names,
            node_type_lookup,
            hnsw,
            embedder: None,
            cooccurrence,
            source_types: HashMap::new(),
            compute_planner: None,
            event_bus: None,
            confidence_thresholds: vec![0.3, 0.5, 0.7, 0.9],
        };
        graph.rebuild_indexes()?;
        Ok(graph)
    }

    /// Set the embedding model for automatic vector generation.
    pub fn set_embedder(&mut self, embedder: Box<dyn Embedder>) {
        self.embedder = Some(embedder);
    }

    /// Set the compute planner for GPU/NPU-accelerated similarity search.
    pub fn set_compute_planner(&mut self, planner: engram_compute::planner::ComputePlanner) {
        self.compute_planner = Some(planner);
    }

    /// Enable or disable int8 vector quantization.
    /// When enabled, HNSW search uses quantized vectors for traversal
    /// and reranks final results with full f32 for accuracy.
    /// Reduces vector memory by ~4x with ~1% accuracy loss.
    pub fn set_vector_quantization(&mut self, mode: crate::index::hnsw::QuantizationMode) {
        self.hnsw.set_quantization(mode);
    }

    /// Set the event bus for graph change notifications.
    pub fn set_event_bus(&mut self, bus: EventBus) {
        self.event_bus = Some(bus);
    }

    /// Get a reference to the event bus (for subscribing).
    pub fn event_bus(&self) -> Option<&EventBus> {
        self.event_bus.as_ref()
    }

    /// Emit an event to subscribers. No-op if no event bus is set.
    pub(crate) fn emit(&self, event: GraphEvent) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(event);
        }
    }

    /// Check if a confidence change crosses any configured threshold.
    pub(crate) fn check_threshold_crossing(
        &self,
        node_id: u64,
        label: &str,
        old_conf: f32,
        new_conf: f32,
    ) {
        for &threshold in &self.confidence_thresholds {
            if old_conf < threshold && new_conf >= threshold {
                self.emit(GraphEvent::ThresholdCrossed {
                    node_id,
                    label: Arc::from(label),
                    old_confidence: old_conf,
                    new_confidence: new_conf,
                    direction: ThresholdDirection::Up,
                });
            } else if old_conf >= threshold && new_conf < threshold {
                self.emit(GraphEvent::ThresholdCrossed {
                    node_id,
                    label: Arc::from(label),
                    old_confidence: old_conf,
                    new_confidence: new_conf,
                    direction: ThresholdDirection::Down,
                });
            }
        }
    }

    /// Get the current vector quantization mode.
    pub fn vector_quantization_mode(&self) -> crate::index::hnsw::QuantizationMode {
        self.hnsw.quantization_mode()
    }

    /// Get approximate vector memory usage in bytes.
    pub fn vector_memory_bytes(&self) -> usize {
        self.hnsw.memory_bytes()
    }

    /// Get the full label for a node slot. Returns the overflow label from
    /// properties if the label was too long for inline storage (>47 bytes),
    /// otherwise returns the inline label.
    pub fn full_label(&self, slot: u64) -> Result<String> {
        let node = self.brain.read_node(slot)?;
        if node.has_label_overflow() {
            if let Some(label) = self.props.get(slot, LABEL_OVERFLOW_KEY) {
                return Ok(label.to_string());
            }
        }
        Ok(node.label().to_string())
    }

    /// Check if a node at the given slot matches the given label (case-insensitive).
    /// Handles overflow labels transparently.
    pub(crate) fn slot_label_eq(&self, slot: u64, label: &str) -> Result<bool> {
        let node = self.brain.read_node(slot)?;
        if node.has_label_overflow() {
            if let Some(full) = self.props.get(slot, LABEL_OVERFLOW_KEY) {
                return Ok(labels_eq(full, label));
            }
        }
        Ok(labels_eq(node.label(), label))
    }

    /// Find a node by label, return its ID.
    pub fn find_node_id(&self, label: &str) -> Result<Option<u64>> {
        let target_hash = hash_label(label);
        for &slot in self.label_index.get(target_hash) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && self.slot_label_eq(slot, label)? {
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
            if node.is_active() && self.slot_label_eq(slot, label)? {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    /// Get a node by its internal node_id.
    pub fn get_node_by_id(&self, node_id: u64) -> Result<Option<&Node>> {
        if let Some(slot) = self.find_slot_by_id(node_id) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    /// Get a node's label by its storage slot.
    pub fn get_node_label_by_slot(&self, slot: u64) -> Option<String> {
        self.full_label(slot).ok()
    }

    pub fn label_for_id(&self, node_id: u64) -> Result<String> {
        if let Some(slot) = self.find_slot_by_id(node_id) {
            self.full_label(slot)
        } else {
            Ok(format!("node_{node_id}"))
        }
    }

    // --- Private helpers ---

    pub(crate) fn find_slot_by_label(&self, label: &str) -> Result<Option<u64>> {
        let target_hash = hash_label(label);
        for &slot in self.label_index.get(target_hash) {
            let node = self.brain.read_node(slot)?;
            if node.is_active() && self.slot_label_eq(slot, label)? {
                return Ok(Some(slot));
            }
        }
        Ok(None)
    }

    pub(crate) fn find_slot_by_id(&self, node_id: u64) -> Option<u64> {
        // node IDs start at 1, slots start at 0
        let slot = node_id.checked_sub(1)?;
        let (count, _) = self.brain.stats();
        if slot < count {
            Some(slot)
        } else {
            None
        }
    }

    pub(crate) fn get_or_create_node_type(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.node_type_lookup.get(name) {
            return id;
        }
        let id = self.node_type_names.len() as u32;
        self.node_type_names.push(name.to_string());
        self.node_type_lookup.insert(name.to_string(), id);
        id
    }

    pub(crate) fn node_type_id(&self, name: &str) -> Option<u32> {
        self.node_type_lookup.get(name).copied()
    }

    pub(crate) fn reindex_fulltext(&mut self, slot: u64) -> Result<()> {
        self.fulltext.remove_document(slot);
        let mut text = self.full_label(slot)?;
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

    pub(crate) fn hits_to_results(&self, hits: &[SearchHit]) -> Result<Vec<NodeSearchResult>> {
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
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

    /// Get the confidence cap for a node based on its source type.
    pub(crate) fn source_cap(&self, slot: u64) -> f32 {
        match self.source_types.get(&slot) {
            Some(st) => confidence_cap(*st),
            None => 0.95, // default cap (user-level)
        }
    }

    pub(crate) fn rebuild_indexes(&mut self) -> Result<()> {
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
                let mut text = self.full_label(slot)?;
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

        // Rebuild adjacency lists (skip deleted edges)
        for edge_slot in 0..edge_count {
            let edge = self.brain.read_edge(edge_slot)?;
            if edge.is_deleted() {
                continue;
            }
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

    /// Persist node type name mapping to `.brain.nodetypes` sidecar.
    pub(crate) fn flush_node_type_names(&self) -> Result<()> {
        if self.node_type_names.is_empty() {
            return Ok(());
        }
        let path = self.brain.path().with_extension("brain.nodetypes");
        let content = self.node_type_names.join("\n");
        std::fs::write(&path, content.as_bytes()).map_err(|e| StorageError::InvalidFile {
            reason: format!("node type names flush failed: {e}"),
        })?;
        Ok(())
    }

    /// Load node type name mapping from `.brain.nodetypes` sidecar.
    fn load_node_type_names(path: &Path) -> (Vec<String>, HashMap<String, u32>) {
        let sidecar = path.with_extension("brain.nodetypes");
        match std::fs::read_to_string(&sidecar) {
            Ok(content) if !content.is_empty() => {
                let names: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let lookup: HashMap<String, u32> = names.iter().enumerate()
                    .map(|(i, n)| (n.clone(), i as u32))
                    .collect();
                (names, lookup)
            }
            _ => (Vec::new(), HashMap::new()),
        }
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

/// Snapshot of a node for export purposes.
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub label: String,
    pub node_type: Option<String>,
    pub confidence: f32,
    pub memory_tier: u8,
    pub properties: HashMap<String, String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub edge_out_count: u32,
    pub edge_in_count: u32,
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

pub(crate) fn current_timestamp() -> i64 {
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
    fn delete_edge_removes_from_queries() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("alpha", &prov).unwrap();
        g.store("beta", &prov).unwrap();
        g.store("gamma", &prov).unwrap();
        g.relate("alpha", "beta", "links_to", &prov).unwrap();
        g.relate("alpha", "gamma", "links_to", &prov).unwrap();

        // Both edges visible
        assert_eq!(g.edges_from("alpha").unwrap().len(), 2);
        assert_eq!(g.all_edges().unwrap().len(), 2);

        // Delete one edge
        let deleted = g.delete_edge("alpha", "beta", "links_to", &prov).unwrap();
        assert!(deleted);

        // Only one edge remains
        let edges = g.edges_from("alpha").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, "gamma");

        // edges_to also reflects deletion
        assert_eq!(g.edges_to("beta").unwrap().len(), 0);
        assert_eq!(g.edges_to("gamma").unwrap().len(), 1);

        // all_edges also filtered
        assert_eq!(g.all_edges().unwrap().len(), 1);

        // Deleting non-existent edge returns false
        let not_deleted = g.delete_edge("alpha", "beta", "links_to", &prov).unwrap();
        assert!(!not_deleted);

        // edge_exists reflects the deletion
        assert!(!g.edge_exists("alpha", "beta", "links_to").unwrap());
        assert!(g.edge_exists("alpha", "gamma", "links_to").unwrap());
    }

    #[test]
    fn delete_edge_by_slot_works() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("x", &prov).unwrap();
        g.store("y", &prov).unwrap();
        g.relate("x", "y", "connects", &prov).unwrap();

        assert_eq!(g.edges_from("x").unwrap().len(), 1);

        // Edge is at slot 0
        let deleted = g.delete_edge_by_slot(0, &prov).unwrap();
        assert!(deleted);
        assert_eq!(g.edges_from("x").unwrap().len(), 0);

        // Double-delete returns false
        let again = g.delete_edge_by_slot(0, &prov).unwrap();
        assert!(!again);
    }

    #[test]
    fn delete_edge_persists_across_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        {
            let mut g = Graph::create(&path).unwrap();
            let prov = test_provenance();
            g.store("a", &prov).unwrap();
            g.store("b", &prov).unwrap();
            g.relate("a", "b", "rel", &prov).unwrap();
            g.delete_edge("a", "b", "rel", &prov).unwrap();
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            assert_eq!(g.edges_from("a").unwrap().len(), 0);
            assert_eq!(g.all_edges().unwrap().len(), 0);
        }
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
    fn source_based_initial_confidence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();

        // User source -> 0.80 initial
        let user_prov = Provenance::user("test");
        g.store("user-fact", &user_prov).unwrap();
        let node = g.get_node("user-fact").unwrap().unwrap();
        assert!((node.confidence - 0.80).abs() < f32::EPSILON);

        // Sensor source -> 0.95 initial
        let sensor_prov = Provenance {
            source_type: SourceType::Sensor,
            source_id: "temp-sensor".into(),
        };
        g.store("sensor-fact", &sensor_prov).unwrap();
        let node = g.get_node("sensor-fact").unwrap().unwrap();
        assert!((node.confidence - 0.95).abs() < f32::EPSILON);

        // LLM source -> 0.30 initial
        let llm_prov = Provenance {
            source_type: SourceType::Llm,
            source_id: "gpt-4".into(),
        };
        g.store("llm-fact", &llm_prov).unwrap();
        let node = g.get_node("llm-fact").unwrap().unwrap();
        assert!((node.confidence - 0.30).abs() < f32::EPSILON);
    }

    #[test]
    fn reinforce_access_boosts_confidence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("fact-a", &prov).unwrap();
        let before = g.get_node("fact-a").unwrap().unwrap().confidence;

        g.reinforce_access("fact-a").unwrap();
        let after = g.get_node("fact-a").unwrap().unwrap().confidence;
        assert!(after > before);

        let node = g.get_node("fact-a").unwrap().unwrap();
        assert_eq!(node.access_count, 1);
    }

    #[test]
    fn reinforce_confirm_boosts_more() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("fact-b", &prov).unwrap();
        let before = g.get_node("fact-b").unwrap().unwrap().confidence;

        g.reinforce_confirm("fact-b", &prov).unwrap();
        let after = g.get_node("fact-b").unwrap().unwrap().confidence;
        assert!(after - before > 0.05); // confirmation boost is 0.10
    }

    #[test]
    fn reinforce_respects_source_cap() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let llm_prov = Provenance {
            source_type: SourceType::Llm,
            source_id: "model".into(),
        };

        g.store("llm-fact", &llm_prov).unwrap();
        // LLM cap is 0.70, initial is 0.30
        // Even with many confirmations, should not exceed 0.70
        for _ in 0..10 {
            g.reinforce_confirm("llm-fact", &llm_prov).unwrap();
        }
        let node = g.get_node("llm-fact").unwrap().unwrap();
        assert!(node.confidence <= 0.70 + f32::EPSILON);
    }

    #[test]
    fn correction_zeros_and_propagates() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("wrong-fact", &prov).unwrap();
        g.store("derived-a", &prov).unwrap();
        g.store("derived-b", &prov).unwrap();
        g.relate("wrong-fact", "derived-a", "supports", &prov).unwrap();
        g.relate("wrong-fact", "derived-b", "supports", &prov).unwrap();

        let before_a = g.get_node("derived-a").unwrap().unwrap().confidence;
        let before_b = g.get_node("derived-b").unwrap().unwrap().confidence;

        let result = g.correct("wrong-fact", &prov, 2).unwrap().unwrap();

        // Corrected node is zeroed
        let slot = result.corrected_slot;
        let raw_node = g.brain.read_node(slot).unwrap();
        assert_eq!(raw_node.confidence, 0.0);

        // Neighbors got penalized
        assert!(!result.propagated.is_empty());
        let after_a = g.get_node("derived-a").unwrap().unwrap().confidence;
        let after_b = g.get_node("derived-b").unwrap().unwrap().confidence;
        assert!(after_a < before_a);
        assert!(after_b < before_b);
    }

    #[test]
    fn cooccurrence_tracking() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();

        g.record_cooccurrence("migration", "missing-index");
        g.record_cooccurrence("migration", "missing-index");
        g.record_cooccurrence("migration", "missing-index");
        g.record_cooccurrence("deploy", "latency-spike");

        let (count, _prob) = g.get_cooccurrence("migration", "missing-index").unwrap();
        assert_eq!(count, 3);

        let pairs = g.cooccurrences_for("migration");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "missing-index");
        assert_eq!(pairs[0].1, 3);
    }

    #[test]
    fn cooccurrence_persists() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let prov = test_provenance();

        {
            let mut g = Graph::create(&path).unwrap();
            g.store("a", &prov).unwrap();
            g.record_cooccurrence("deploy", "error");
            g.record_cooccurrence("deploy", "error");
            g.checkpoint().unwrap();
        }

        {
            let g = Graph::open(&path).unwrap();
            let (count, _) = g.get_cooccurrence("deploy", "error").unwrap();
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn property_contradiction_detected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.set_property("server-01", "ip", "10.0.0.1").unwrap();

        // Setting a different value should flag a contradiction
        let conflicts = g.check_property_contradiction("server-01", "ip", "10.0.0.2").unwrap();
        assert!(conflicts.has_conflicts());
        assert_eq!(conflicts.contradictions.len(), 1);
        assert!(conflicts.contradictions[0].reason.contains("10.0.0.1"));

        // Same value should NOT flag
        let no_conflict = g.check_property_contradiction("server-01", "ip", "10.0.0.1").unwrap();
        assert!(!no_conflict.has_conflicts());
    }

    #[test]
    fn set_property_checked_flags_but_writes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.set_property("server-01", "ip", "10.0.0.1").unwrap();

        // Change value: should flag contradiction but still write
        let (ok, conflicts) = g.set_property_checked("server-01", "ip", "10.0.0.2").unwrap();
        assert!(ok);
        assert!(conflicts.has_conflicts());

        // Value should be updated despite contradiction
        assert_eq!(
            g.get_property("server-01", "ip").unwrap(),
            Some("10.0.0.2".to_string())
        );
    }

    #[test]
    fn evidence_surfaces_cooccurrences() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("migration", &prov).unwrap();
        g.record_cooccurrence("migration", "missing-index");
        g.record_cooccurrence("migration", "missing-index");

        let results = g.search_with_evidence("migration", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].evidence.cooccurrences.len(), 1);
        assert_eq!(results[0].evidence.cooccurrences[0].count, 2);
    }

    #[test]
    fn evidence_surfaces_supporting_facts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("server-01", &prov).unwrap();
        g.store("postgresql", &prov).unwrap();
        g.relate("server-01", "postgresql", "runs", &prov).unwrap();

        let results = g.search_with_evidence("server", 10).unwrap();
        assert!(!results.is_empty());
        // server-01 should have postgresql as supporting fact
        let server_result = results.iter().find(|r| r.label == "server-01").unwrap();
        assert_eq!(server_result.evidence.supporting.len(), 1);
        assert_eq!(server_result.evidence.supporting[0].label, "postgresql");
        assert_eq!(server_result.evidence.supporting[0].relationship, "runs");
    }

    #[test]
    fn tier_sweep_promotes_and_demotes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        // High confidence + many accesses -> should promote to core
        g.store_with_confidence("popular", 0.95, &prov).unwrap();
        for _ in 0..12 {
            g.reinforce_access("popular").unwrap();
        }

        // Low confidence -> should demote to archival
        g.store_with_confidence("forgotten", 0.15, &prov).unwrap();

        let result = g.sweep_tiers().unwrap();
        assert!(result.promoted_to_core > 0 || result.demoted_to_archival > 0);

        let popular_node = g.get_node("popular").unwrap().unwrap();
        assert_eq!(popular_node.memory_tier, 0); // TIER_CORE

        let forgotten_node = g.get_node("forgotten").unwrap().unwrap();
        assert_eq!(forgotten_node.memory_tier, 2); // TIER_ARCHIVAL
    }

    #[test]
    fn manual_tier_override() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("important", &prov).unwrap();
        g.set_tier("important", 0).unwrap(); // force core

        let node = g.get_node("important").unwrap().unwrap();
        assert_eq!(node.memory_tier, 0);

        let core = g.core_nodes().unwrap();
        assert_eq!(core.len(), 1);
        assert_eq!(core[0].label, "important");
    }

    #[test]
    fn forward_chain_transitive_inference() {
        use crate::learning::rules;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        // Setup: cat is_a feline, feline is_a animal
        g.store("cat", &prov).unwrap();
        g.store("feline", &prov).unwrap();
        g.store("animal", &prov).unwrap();
        g.relate("cat", "feline", "is_a", &prov).unwrap();
        g.relate("feline", "animal", "is_a", &prov).unwrap();

        // Rule: transitive is_a
        let rule = rules::parse_rule(r#"
rule transitive_type
when edge(A, "is_a", B)
when edge(B, "is_a", C)
then edge(A, "is_a", C, min(A, C))
"#).unwrap();

        let result = g.forward_chain(&[rule], &prov).unwrap();
        assert!(result.rules_fired > 0);
        assert!(result.edges_created > 0);

        // cat should now have a derived is_a edge to animal
        assert!(g.edge_exists("cat", "animal", "is_a").unwrap());
    }

    #[test]
    fn forward_chain_confidence_flag() {
        use crate::learning::rules;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store_with_confidence("weak-fact", 0.15, &prov).unwrap();
        g.store_with_confidence("strong-fact", 0.95, &prov).unwrap();

        let rule = rules::parse_rule(r#"
rule stale_warning
when confidence(node, "<", 0.2)
then flag(node, "low confidence")
"#).unwrap();

        let result = g.forward_chain(&[rule], &prov).unwrap();
        assert_eq!(result.flags_raised, 1);

        // Check that the flag was set as a property
        let flag = g.get_property("weak-fact", "_flag").unwrap();
        assert_eq!(flag, Some("low confidence".to_string()));

        // Strong fact should NOT be flagged
        let no_flag = g.get_property("strong-fact", "_flag").unwrap();
        assert_eq!(no_flag, None);
    }

    #[test]
    fn prove_direct_edge() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.relate("a", "b", "connects", &prov).unwrap();

        let proof = g.prove("a", "b", "connects", 3).unwrap();
        assert!(proof.supported);
        assert_eq!(proof.chain.len(), 1);
    }

    #[test]
    fn prove_transitive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();
        g.store("c", &prov).unwrap();
        g.relate("a", "b", "is_a", &prov).unwrap();
        g.relate("b", "c", "is_a", &prov).unwrap();

        let proof = g.prove("a", "c", "is_a", 3).unwrap();
        assert!(proof.supported);
        assert_eq!(proof.chain.len(), 2); // a->b, b->c
    }

    #[test]
    fn prove_unsupported() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        g.store("a", &prov).unwrap();
        g.store("b", &prov).unwrap();

        let proof = g.prove("a", "b", "is_a", 3).unwrap();
        assert!(!proof.supported);
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

    // -- Label overflow tests --

    #[test]
    fn short_label_no_overflow() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let id = g.store("short-label", &prov).unwrap();
        let label = g.label_for_id(id).unwrap();
        assert_eq!(label, "short-label");

        // No overflow flag
        let node = g.get_node("short-label").unwrap().unwrap();
        assert!(!node.has_label_overflow());
    }

    #[test]
    fn label_exactly_47_bytes_no_overflow() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let label_47 = "a".repeat(47); // exactly at limit
        let id = g.store(&label_47, &prov).unwrap();
        let full = g.label_for_id(id).unwrap();
        assert_eq!(full, label_47);

        let node = g.get_node(&label_47).unwrap().unwrap();
        assert!(!node.has_label_overflow());
    }

    #[test]
    fn label_overflow_stores_full_label_in_props() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_label = "this-is-a-very-long-label-that-exceeds-the-48-byte-inline-limit-by-quite-a-lot";
        assert!(long_label.len() > 47);

        let id = g.store(long_label, &prov).unwrap();

        // Full label retrieved correctly
        let full = g.label_for_id(id).unwrap();
        assert_eq!(full, long_label);

        // Overflow flag set
        let node = g.get_node_by_id(id).unwrap().unwrap();
        assert!(node.has_label_overflow());

        // Inline label is truncated prefix
        assert_eq!(node.label().len(), 47);
        assert!(long_label.starts_with(node.label()));
    }

    #[test]
    fn find_node_by_overflow_label() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_label = "overflow-label-that-is-definitely-longer-than-forty-seven-bytes-total";
        let id = g.store(long_label, &prov).unwrap();

        // Find by full label
        let found = g.find_node_id(long_label).unwrap();
        assert_eq!(found, Some(id));

        // get_node by full label
        let node = g.get_node(long_label).unwrap();
        assert!(node.is_some());
    }

    #[test]
    fn overflow_label_dedup_on_re_store() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_label = "a-label-that-overflows-the-inline-storage-area-which-is-only-47-bytes";
        let id1 = g.store(long_label, &prov).unwrap();
        let id2 = g.store(long_label, &prov).unwrap();
        assert_eq!(id1, id2); // same node, not duplicated
    }

    #[test]
    fn overflow_label_case_insensitive_dedup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let label_lower = "this-is-a-very-long-case-insensitive-label-that-overflows-inline";
        let label_upper = "THIS-IS-A-VERY-LONG-CASE-INSENSITIVE-LABEL-THAT-OVERFLOWS-INLINE";
        let id1 = g.store(label_lower, &prov).unwrap();
        let id2 = g.store(label_upper, &prov).unwrap();
        assert_eq!(id1, id2); // case-insensitive dedup
    }

    #[test]
    fn short_label_dedup_on_re_store() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let id1 = g.store("short-label", &prov).unwrap();
        let id2 = g.store("short-label", &prov).unwrap();
        assert_eq!(id1, id2);

        // Case-insensitive dedup for short labels too
        let id3 = g.store("SHORT-LABEL", &prov).unwrap();
        assert_eq!(id1, id3);
    }

    #[test]
    fn relate_with_overflow_labels() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_from = "source-node-with-a-label-exceeding-the-forty-seven-byte-limit";
        let long_to = "target-node-with-a-label-also-exceeding-the-forty-seven-byte-limit";
        g.store(long_from, &prov).unwrap();
        g.store(long_to, &prov).unwrap();
        g.relate(long_from, long_to, "connects_to", &prov).unwrap();

        let edges = g.edges_from(long_from).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, long_to);
        assert_eq!(edges[0].relationship, "connects_to");

        let edges_in = g.edges_to(long_to).unwrap();
        assert_eq!(edges_in.len(), 1);
        assert_eq!(edges_in[0].from, long_from);
    }

    #[test]
    fn delete_node_with_overflow_label() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_label = "deletable-node-with-overflow-label-exceeding-forty-seven-bytes";
        g.store(long_label, &prov).unwrap();
        assert!(g.find_node_id(long_label).unwrap().is_some());

        g.delete(long_label, &prov).unwrap();
        assert!(g.find_node_id(long_label).unwrap().is_none());
    }

    #[test]
    fn traverse_with_overflow_labels() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_a = "start-node-with-an-overflow-label-longer-than-47-bytes-for-testing";
        let short_b = "short-target";
        g.store(long_a, &prov).unwrap();
        g.store(short_b, &prov).unwrap();
        g.relate(long_a, short_b, "links_to", &prov).unwrap();

        let result = g.traverse(long_a, 1, 0.0).unwrap();
        assert!(result.nodes.len() >= 2);

        // Verify the long label appears in the labels
        let labels: Vec<String> = result.nodes.iter()
            .filter_map(|&nid| g.label_for_id(nid).ok())
            .collect();
        assert!(labels.contains(&long_a.to_string()));
        assert!(labels.contains(&short_b.to_string()));
    }

    #[test]
    fn fulltext_search_overflow_label() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        let long_label = "unique-searchable-entity-with-overflow-label-longer-than-47-bytes";
        g.store(long_label, &prov).unwrap();

        let results = g.search_text("unique-searchable-entity", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].label, long_label);
    }

    #[test]
    fn reset_clears_graph() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = test_provenance();

        // Store 10 nodes
        for i in 0..10 {
            g.store(&format!("node-{i}"), &prov).unwrap();
        }
        // Store 5 edges
        for i in 0..5 {
            g.relate(&format!("node-{i}"), &format!("node-{}", i + 5), "links_to", &prov).unwrap();
        }

        let (nodes, edges) = g.stats();
        assert_eq!(nodes, 10);
        assert_eq!(edges, 5);

        // Reset
        g.reset().unwrap();

        let (nodes, edges) = g.stats();
        assert_eq!(nodes, 0);
        assert_eq!(edges, 0);
    }
}
