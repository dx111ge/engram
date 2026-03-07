/// HNSW (Hierarchical Navigable Small World) index for approximate nearest neighbor search.
///
/// Pure Rust implementation. Stores embedding vectors in-memory, persisted
/// to a `.brain.vectors` sidecar file on checkpoint.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use std::io::{BufReader, BufWriter, Read, Write};
use std::fs::File;
use std::path::{Path, PathBuf};

/// HNSW tuning parameters
const M: usize = 16;           // max connections per layer
const M_MAX0: usize = 32;      // max connections at layer 0
const EF_CONSTRUCTION: usize = 200;
/// Level multiplier: 1/ln(M). Computed at runtime since ln() isn't const.
fn ml() -> f64 {
    1.0 / (M as f64).ln()
}

/// HNSW index
pub struct HnswIndex {
    path: PathBuf,
    /// slot -> vector
    vectors: HashMap<u64, Vec<f32>>,
    /// layer -> (slot -> neighbors with distances)
    layers: Vec<HashMap<u64, Vec<(u64, f32)>>>,
    /// Entry point slot (highest layer node)
    entry_point: Option<u64>,
    /// Maximum layer assigned to each node
    node_layer: HashMap<u64, usize>,
    /// Embedding dimensions (0 = not yet determined)
    dim: usize,
    /// Random seed for level generation
    rng_state: u64,
}

/// A nearest neighbor result
#[derive(Debug, Clone)]
pub struct NearestNeighbor {
    pub slot: u64,
    pub distance: f32,
}

/// Min-heap entry (closest first)
#[derive(Clone)]
struct HeapEntry {
    slot: u64,
    dist: f32,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool { self.dist == other.dist }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior with BinaryHeap (max-heap)
        other.dist.partial_cmp(&self.dist).unwrap_or(Ordering::Equal)
    }
}

/// Max-heap entry (farthest first) for candidate pruning
#[derive(Clone)]
struct MaxHeapEntry {
    slot: u64,
    dist: f32,
}

impl PartialEq for MaxHeapEntry {
    fn eq(&self, other: &Self) -> bool { self.dist == other.dist }
}
impl Eq for MaxHeapEntry {}
impl PartialOrd for MaxHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for MaxHeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.dist.partial_cmp(&other.dist).unwrap_or(Ordering::Equal)
    }
}

impl HnswIndex {
    pub fn new(brain_path: &Path) -> Self {
        HnswIndex {
            path: brain_path.with_extension("brain.vectors"),
            vectors: HashMap::new(),
            layers: Vec::new(),
            entry_point: None,
            node_layer: HashMap::new(),
            dim: 0,
            rng_state: 42,
        }
    }

    /// Load vectors from sidecar file, then rebuild the HNSW graph.
    pub fn load(brain_path: &Path) -> Self {
        let path = brain_path.with_extension("brain.vectors");
        let mut index = HnswIndex {
            path,
            vectors: HashMap::new(),
            layers: Vec::new(),
            entry_point: None,
            node_layer: HashMap::new(),
            dim: 0,
            rng_state: 42,
        };

        if index.path.exists() {
            if let Ok(vectors) = read_vectors(&index.path) {
                // Re-insert all vectors to rebuild the HNSW graph
                for (slot, vec) in vectors {
                    index.insert(slot, vec);
                }
            }
        }

        index
    }

    /// Insert a vector for a node slot.
    pub fn insert(&mut self, slot: u64, vector: Vec<f32>) {
        if vector.is_empty() {
            return;
        }

        if self.dim == 0 {
            self.dim = vector.len();
        } else if vector.len() != self.dim {
            return; // dimension mismatch
        }

        let level = self.random_level();

        // Ensure we have enough layers
        while self.layers.len() <= level {
            self.layers.push(HashMap::new());
        }

        // Register the node at its assigned level
        self.node_layer.insert(slot, level);
        for l in 0..=level {
            self.layers[l].entry(slot).or_default();
        }

        self.vectors.insert(slot, vector.clone());

        if self.entry_point.is_none() {
            self.entry_point = Some(slot);
            return;
        }

        let ep = self.entry_point.unwrap();
        let ep_level = *self.node_layer.get(&ep).unwrap_or(&0);

        let mut current = ep;

        // Traverse from top to the node's level + 1 (greedy search)
        for l in (level + 1..=ep_level).rev() {
            current = self.greedy_closest(current, &vector, l);
        }

        // Insert at each layer from min(level, ep_level) down to 0
        let start_level = level.min(ep_level);
        for l in (0..=start_level).rev() {
            let max_conn = if l == 0 { M_MAX0 } else { M };
            let neighbors = self.search_layer(&vector, current, EF_CONSTRUCTION, l);

            // Select M best neighbors
            let selected: Vec<(u64, f32)> = neighbors
                .iter()
                .take(max_conn)
                .map(|e| (e.slot, e.dist))
                .collect();

            // Add bidirectional connections
            for &(neighbor, dist) in &selected {
                self.layers[l].entry(slot).or_default().push((neighbor, dist));
                self.layers[l].entry(neighbor).or_default().push((slot, dist));

                // Prune neighbor's connections if over limit
                let neighbor_conns = self.layers[l].get_mut(&neighbor).unwrap();
                if neighbor_conns.len() > max_conn {
                    neighbor_conns.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
                    neighbor_conns.truncate(max_conn);
                }
            }

            if !selected.is_empty() {
                current = selected[0].0;
            }
        }

        // Update entry point if this node has a higher level
        if level > ep_level {
            self.entry_point = Some(slot);
        }
    }

    /// Remove a vector (lazy deletion — just removes from vectors map).
    pub fn remove(&mut self, slot: u64) {
        self.vectors.remove(&slot);
        // Note: connections still exist but search will skip deleted nodes
    }

    /// Search for k nearest neighbors.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<NearestNeighbor> {
        self.search_ef(query, k, k.max(50))
    }

    /// Search with explicit ef parameter.
    pub fn search_ef(&self, query: &[f32], k: usize, ef: usize) -> Vec<NearestNeighbor> {
        if self.entry_point.is_none() || query.len() != self.dim {
            return Vec::new();
        }

        let ep = self.entry_point.unwrap();
        let ep_level = *self.node_layer.get(&ep).unwrap_or(&0);

        let mut current = ep;

        // Greedy search from top layer down to layer 1
        for l in (1..=ep_level).rev() {
            current = self.greedy_closest(current, query, l);
        }

        // Search at layer 0 with ef candidates
        let mut results = self.search_layer(query, current, ef, 0);

        // Filter out deleted nodes and take top k
        results.retain(|e| self.vectors.contains_key(&e.slot));
        results.truncate(k);

        results
            .into_iter()
            .map(|e| NearestNeighbor {
                slot: e.slot,
                distance: e.dist,
            })
            .collect()
    }

    /// Number of indexed vectors.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Embedding dimensions.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Persist vectors to sidecar file.
    pub fn flush(&self) -> std::io::Result<()> {
        if self.vectors.is_empty() {
            // Don't create empty files
            if self.path.exists() {
                std::fs::remove_file(&self.path)?;
            }
            return Ok(());
        }
        write_vectors(&self.path, &self.vectors, self.dim)
    }

    // --- Internal ---

    fn random_level(&mut self) -> usize {
        // xorshift64
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;

        let uniform = (self.rng_state as f64) / (u64::MAX as f64);
        let level = (-uniform.ln() * ml()) as usize;
        level.min(16) // cap at reasonable max
    }

    fn greedy_closest(&self, start: u64, query: &[f32], layer: usize) -> u64 {
        let mut current = start;
        let mut current_dist = self.distance(query, current);

        loop {
            let mut changed = false;
            if let Some(neighbors) = self.layers.get(layer).and_then(|l| l.get(&current)) {
                for &(neighbor, _) in neighbors {
                    if !self.vectors.contains_key(&neighbor) {
                        continue;
                    }
                    let dist = self.distance(query, neighbor);
                    if dist < current_dist {
                        current = neighbor;
                        current_dist = dist;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        current
    }

    fn search_layer(&self, query: &[f32], entry: u64, ef: usize, layer: usize) -> Vec<HeapEntry> {
        let entry_dist = self.distance(query, entry);
        let mut visited: HashSet<u64> = HashSet::new();
        visited.insert(entry);

        // candidates: min-heap (closest first)
        let mut candidates: BinaryHeap<HeapEntry> = BinaryHeap::new();
        candidates.push(HeapEntry { slot: entry, dist: entry_dist });

        // results: max-heap (farthest first for pruning)
        let mut results: BinaryHeap<MaxHeapEntry> = BinaryHeap::new();
        results.push(MaxHeapEntry { slot: entry, dist: entry_dist });

        while let Some(candidate) = candidates.pop() {
            let farthest = results.peek().map(|e| e.dist).unwrap_or(f32::MAX);
            if candidate.dist > farthest && results.len() >= ef {
                break;
            }

            if let Some(neighbors) = self.layers.get(layer).and_then(|l| l.get(&candidate.slot)) {
                for &(neighbor, _) in neighbors {
                    if !visited.insert(neighbor) {
                        continue;
                    }
                    if !self.vectors.contains_key(&neighbor) {
                        continue;
                    }

                    let dist = self.distance(query, neighbor);
                    let farthest = results.peek().map(|e| e.dist).unwrap_or(f32::MAX);

                    if dist < farthest || results.len() < ef {
                        candidates.push(HeapEntry { slot: neighbor, dist });
                        results.push(MaxHeapEntry { slot: neighbor, dist });
                        if results.len() > ef {
                            results.pop(); // remove farthest
                        }
                    }
                }
            }
        }

        // Convert results to sorted vec (closest first)
        let mut sorted: Vec<HeapEntry> = results
            .into_iter()
            .map(|e| HeapEntry { slot: e.slot, dist: e.dist })
            .collect();
        sorted.sort_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap_or(Ordering::Equal));
        sorted
    }

    /// Cosine distance (1 - cosine_similarity). Lower = more similar.
    fn distance(&self, query: &[f32], slot: u64) -> f32 {
        match self.vectors.get(&slot) {
            Some(vec) => cosine_distance(query, vec),
            None => f32::MAX,
        }
    }
}

/// Cosine distance: 1.0 - cosine_similarity
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < f32::EPSILON {
        return 1.0;
    }
    1.0 - (dot / denom)
}

/// Write vectors to binary file.
/// Format: dim(u32) + count(u64) + [slot(u64) + values(f32 * dim)] * count
fn write_vectors(path: &Path, vectors: &HashMap<u64, Vec<f32>>, dim: usize) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut w = BufWriter::new(file);

    w.write_all(&(dim as u32).to_le_bytes())?;
    w.write_all(&(vectors.len() as u64).to_le_bytes())?;

    for (&slot, vec) in vectors {
        w.write_all(&slot.to_le_bytes())?;
        for &val in vec {
            w.write_all(&val.to_le_bytes())?;
        }
    }

    w.flush()?;
    Ok(())
}

/// Read vectors from binary file.
fn read_vectors(path: &Path) -> std::io::Result<Vec<(u64, Vec<f32>)>> {
    let file = File::open(path)?;
    let mut r = BufReader::new(file);

    let mut buf4 = [0u8; 4];
    let mut buf8 = [0u8; 8];

    r.read_exact(&mut buf4)?;
    let dim = u32::from_le_bytes(buf4) as usize;

    r.read_exact(&mut buf8)?;
    let count = u64::from_le_bytes(buf8) as usize;

    let mut vectors = Vec::with_capacity(count);
    for _ in 0..count {
        r.read_exact(&mut buf8)?;
        let slot = u64::from_le_bytes(buf8);

        let mut vec = vec![0.0f32; dim];
        for v in &mut vec {
            r.read_exact(&mut buf4)?;
            *v = f32::from_le_bytes(buf4);
        }

        vectors.push((slot, vec));
    }

    Ok(vectors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_vec(base: f32, dim: usize) -> Vec<f32> {
        (0..dim).map(|i| base + i as f32 * 0.1).collect()
    }

    #[test]
    fn cosine_distance_identical() {
        let a = vec![1.0, 0.0, 0.0];
        assert!(cosine_distance(&a, &a) < 0.001);
    }

    #[test]
    fn cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_distance_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 2.0).abs() < 0.001);
    }

    #[test]
    fn insert_and_search() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut idx = HnswIndex::new(&path);

        // Insert 3 vectors in 4D
        idx.insert(0, vec![1.0, 0.0, 0.0, 0.0]);
        idx.insert(1, vec![0.9, 0.1, 0.0, 0.0]);
        idx.insert(2, vec![0.0, 1.0, 0.0, 0.0]);

        // Search for something close to slot 0
        let results = idx.search(&[1.0, 0.05, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        // Slot 0 or 1 should be closest
        assert!(results[0].slot == 0 || results[0].slot == 1);
    }

    #[test]
    fn search_empty_index() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let idx = HnswIndex::new(&path);

        let results = idx.search(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn insert_many_and_recall() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut idx = HnswIndex::new(&path);

        // Insert 100 vectors
        for i in 0..100u64 {
            let vec = make_vec(i as f32, 8);
            idx.insert(i, vec);
        }

        assert_eq!(idx.len(), 100);

        // Search for something close to slot 50
        let query = make_vec(50.0, 8);
        let results = idx.search(&query, 10);
        assert!(!results.is_empty());
        // Slot 50 should be among the top results (cosine distance is very
        // small between adjacent make_vec values, so exact rank may vary)
        let top_slots: Vec<u64> = results.iter().map(|r| r.slot).collect();
        assert!(top_slots.contains(&50), "slot 50 not in top results: {:?}", top_slots);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        {
            let mut idx = HnswIndex::new(&path);
            idx.insert(0, vec![1.0, 0.0, 0.0]);
            idx.insert(1, vec![0.0, 1.0, 0.0]);
            idx.insert(2, vec![0.0, 0.0, 1.0]);
            idx.flush().unwrap();
        }

        {
            let idx = HnswIndex::load(&path);
            assert_eq!(idx.len(), 3);

            let results = idx.search(&[1.0, 0.1, 0.0], 1);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].slot, 0);
        }
    }

    #[test]
    fn remove_excludes_from_search() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut idx = HnswIndex::new(&path);

        idx.insert(0, vec![1.0, 0.0]);
        idx.insert(1, vec![0.9, 0.1]);
        idx.remove(0);

        let results = idx.search(&[1.0, 0.0], 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slot, 1);
    }
}
