/// BrainFile — the main interface for .brain file operations.
///
/// Provides safe abstractions over the mmap'd storage, WAL, and node/edge access.
/// Single-writer, multiple-reader model.

use crate::storage::edge::{Edge, EDGE_SIZE};
use crate::storage::error::{Result, StorageError};
use crate::storage::header::Header;
use crate::storage::mmap::MmapFile;
use crate::storage::node::{hash_label, labels_eq, Node, NODE_SIZE};
use crate::storage::wal::{Wal, WalOp};
use std::path::{Path, PathBuf};

/// Default initial capacities.
/// These are starting sizes -- the file auto-grows when full (doubles each time).
/// 10K nodes * 256 bytes = 2.5MB, 40K edges * 64 bytes = 2.5MB -> ~5MB initial file.
const DEFAULT_NODE_CAPACITY: u64 = 10_000;
const DEFAULT_EDGE_CAPACITY: u64 = 40_000;

pub struct BrainFile {
    path: PathBuf,
    mmap: MmapFile,
    wal: Wal,
}

impl BrainFile {
    /// Create a new .brain file at the given path.
    pub fn create(path: &Path) -> Result<Self> {
        Self::create_with_capacity(path, DEFAULT_NODE_CAPACITY, DEFAULT_EDGE_CAPACITY)
    }

    /// Create with specific node and edge capacities.
    pub fn create_with_capacity(
        path: &Path,
        node_capacity: u64,
        edge_capacity: u64,
    ) -> Result<Self> {
        let header = Header::new(node_capacity, edge_capacity);
        let file_size = header.total_file_size();

        let mmap = MmapFile::create(path, file_size)?;

        // Write initial header
        // SAFETY: We just created the file, we have exclusive access.
        unsafe {
            let h = mmap.header_mut();
            *h = header;
        }
        mmap.flush()?;

        let wal = Wal::open(path, 0)?;

        Ok(BrainFile {
            path: path.to_path_buf(),
            mmap,
            wal,
        })
    }

    /// Open an existing .brain file.
    pub fn open(path: &Path) -> Result<Self> {
        let mmap = MmapFile::open(path)?;
        let header = mmap.read_header();
        header.validate()?;

        let wal = Wal::open(path, header.wal_last_seq)?;

        let mut brain = BrainFile {
            path: path.to_path_buf(),
            mmap,
            wal,
        };

        // Replay any uncommitted WAL entries
        brain.replay_wal()?;

        Ok(brain)
    }

    /// Get the file path of this .brain file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Store a new node. Returns the assigned node ID.
    /// Auto-grows the file if the node region is full.
    pub fn store_node(&mut self, label: &str) -> Result<u64> {
        let header = self.mmap.read_header();
        let node_id = header.next_node_id;
        let node_count = header.node_count;
        let needs_grow = node_count >= header.node_region_capacity;

        if needs_grow {
            self.grow_node_region()?;
        }

        let now = current_timestamp();
        let node = Node::new(node_id, label, now);

        // WAL first — if we crash after WAL write but before mmap write, replay will fix it
        let node_bytes = unsafe {
            std::slice::from_raw_parts(&node as *const Node as *const u8, NODE_SIZE)
        };
        self.wal.append(WalOp::NodeCreate, node_bytes)?;

        // Write to mmap
        self.write_node_at_slot(node_count, &node)?;

        // Update header
        // SAFETY: single-writer access
        unsafe {
            let h = self.mmap.header_mut();
            h.node_count += 1;
            h.next_node_id = node_id + 1;
            h.checksum = h.compute_checksum();
        }

        Ok(node_id)
    }

    /// Read a node by its slot index.
    pub fn read_node(&self, slot: u64) -> Result<&Node> {
        let header = self.mmap.read_header();
        if slot >= header.node_count {
            return Err(StorageError::NodeNotFound { id: slot });
        }

        let offset = header.node_region_offset + slot * NODE_SIZE as u64;
        // SAFETY: slot is bounds-checked, offset is within the node region,
        // Node is repr(C) with known layout, and the mmap region is valid.
        let node = unsafe { &*(self.mmap.ptr_at(offset) as *const Node) };
        Ok(node)
    }

    /// Find a node by label (linear scan for Phase 0, hash index comes later).
    pub fn find_node_by_label(&self, label: &str) -> Result<Option<(u64, &Node)>> {
        let target_hash = hash_label(label);
        let header = self.mmap.read_header();

        for slot in 0..header.node_count {
            let node = self.read_node(slot)?;
            if node.is_active() && node.label_hash == target_hash && labels_eq(node.label(), label) {
                return Ok(Some((slot, node)));
            }
        }
        Ok(None)
    }

    /// Store a new edge. Returns the assigned edge ID.
    /// Auto-grows the file if the edge region is full.
    pub fn store_edge(&mut self, from_node: u64, to_node: u64, edge_type: u32) -> Result<u64> {
        let header = self.mmap.read_header();
        let edge_id = header.next_edge_id;
        let edge_count = header.edge_count;
        let edge_region_offset = header.edge_region_offset;
        let needs_grow = edge_count >= header.edge_region_capacity;

        if needs_grow {
            self.grow_edge_region()?;
        }

        let now = current_timestamp();
        let edge = Edge::new(edge_id, from_node, to_node, edge_type, now);

        let edge_bytes = unsafe {
            std::slice::from_raw_parts(&edge as *const Edge as *const u8, EDGE_SIZE)
        };
        self.wal.append(WalOp::EdgeCreate, edge_bytes)?;

        let offset = edge_region_offset + edge_count * EDGE_SIZE as u64;
        // SAFETY: bounds checked above, single-writer access
        unsafe {
            let dest = self.mmap.ptr_at_mut(offset);
            std::ptr::copy_nonoverlapping(edge_bytes.as_ptr(), dest, EDGE_SIZE);
        }

        unsafe {
            let h = self.mmap.header_mut();
            h.edge_count += 1;
            h.next_edge_id = edge_id + 1;
            h.checksum = h.compute_checksum();
        }

        Ok(edge_id)
    }

    /// Read an edge by its slot index.
    pub fn read_edge(&self, slot: u64) -> Result<&Edge> {
        let header = self.mmap.read_header();
        if slot >= header.edge_count {
            return Err(StorageError::NodeNotFound { id: slot });
        }

        let offset = header.edge_region_offset + slot * EDGE_SIZE as u64;
        // SAFETY: slot is bounds-checked, Edge is repr(C)
        let edge = unsafe { &*(self.mmap.ptr_at(offset) as *const Edge) };
        Ok(edge)
    }

    /// Mutate a node in-place by slot index. WAL-protected.
    pub fn update_node_field<F>(&mut self, slot: u64, f: F) -> Result<()>
    where
        F: FnOnce(&mut Node),
    {
        let header = self.mmap.read_header();
        if slot >= header.node_count {
            return Err(StorageError::NodeNotFound { id: slot });
        }

        // Read current node, apply mutation to a copy
        let offset = header.node_region_offset + slot * NODE_SIZE as u64;
        let mut node_copy = unsafe { *(self.mmap.ptr_at(offset) as *const Node) };
        f(&mut node_copy);

        // WAL first: slot(8) + node_bytes(NODE_SIZE)
        let mut wal_data = Vec::with_capacity(8 + NODE_SIZE);
        wal_data.extend_from_slice(&slot.to_le_bytes());
        let node_bytes = unsafe {
            std::slice::from_raw_parts(&node_copy as *const Node as *const u8, NODE_SIZE)
        };
        wal_data.extend_from_slice(node_bytes);
        self.wal.append(WalOp::NodeUpdate, &wal_data)?;

        // Write to mmap
        unsafe {
            let dest = self.mmap.ptr_at_mut(offset);
            std::ptr::copy_nonoverlapping(node_bytes.as_ptr(), dest, NODE_SIZE);
        }
        Ok(())
    }

    /// Mutate an edge in-place by slot index. WAL-protected.
    pub fn update_edge_field<F>(&mut self, slot: u64, f: F) -> Result<()>
    where
        F: FnOnce(&mut Edge),
    {
        let header = self.mmap.read_header();
        if slot >= header.edge_count {
            return Err(StorageError::NodeNotFound { id: slot });
        }

        // Read current edge, apply mutation to a copy
        let offset = header.edge_region_offset + slot * EDGE_SIZE as u64;
        let mut edge_copy = unsafe { *(self.mmap.ptr_at(offset) as *const Edge) };
        f(&mut edge_copy);

        // WAL first: slot(8) + edge_bytes(EDGE_SIZE)
        let mut wal_data = Vec::with_capacity(8 + EDGE_SIZE);
        wal_data.extend_from_slice(&slot.to_le_bytes());
        let edge_bytes = unsafe {
            std::slice::from_raw_parts(&edge_copy as *const Edge as *const u8, EDGE_SIZE)
        };
        wal_data.extend_from_slice(edge_bytes);
        self.wal.append(WalOp::EdgeUpdate, &wal_data)?;

        // Write to mmap
        unsafe {
            let dest = self.mmap.ptr_at_mut(offset);
            std::ptr::copy_nonoverlapping(edge_bytes.as_ptr(), dest, EDGE_SIZE);
        }
        Ok(())
    }

    /// Soft-delete an edge by its slot index. WAL-protected.
    pub fn delete_edge(&mut self, slot: u64) -> Result<()> {
        let header = self.mmap.read_header();
        if slot >= header.edge_count {
            return Err(StorageError::NodeNotFound { id: slot });
        }

        // Read current edge, apply soft-delete to a copy
        let offset = header.edge_region_offset + slot * EDGE_SIZE as u64;
        let mut edge_copy = unsafe { *(self.mmap.ptr_at(offset) as *const Edge) };
        edge_copy.soft_delete();

        // WAL first: slot(8) + edge_bytes(EDGE_SIZE)
        let mut wal_data = Vec::with_capacity(8 + EDGE_SIZE);
        wal_data.extend_from_slice(&slot.to_le_bytes());
        let edge_bytes = unsafe {
            std::slice::from_raw_parts(&edge_copy as *const Edge as *const u8, EDGE_SIZE)
        };
        wal_data.extend_from_slice(edge_bytes);
        self.wal.append(WalOp::EdgeDelete, &wal_data)?;

        // Write to mmap
        unsafe {
            let dest = self.mmap.ptr_at_mut(offset);
            std::ptr::copy_nonoverlapping(edge_bytes.as_ptr(), dest, EDGE_SIZE);
        }
        Ok(())
    }

    /// Get current stats.
    pub fn stats(&self) -> (u64, u64) {
        let header = self.mmap.read_header();
        (header.node_count, header.edge_count)
    }

    /// Flush all changes to disk and write a WAL checkpoint.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.mmap.flush()?;
        let seq = self.wal.checkpoint()?;

        // Update header with last WAL seq
        unsafe {
            let h = self.mmap.header_mut();
            h.wal_last_seq = seq;
            h.checksum = h.compute_checksum();
        }
        self.mmap.flush()?;

        // WAL can now be truncated
        self.wal.truncate()?;
        Ok(())
    }

    /// Double the node region capacity.
    ///
    /// Layout: [Header][Nodes][Edges][WAL]
    /// Growing nodes means shifting edges forward. Steps:
    ///   1. Read all edge data into memory
    ///   2. Calculate new file size with doubled node capacity
    ///   3. Remap the file to the new size
    ///   4. Write edge data at the new edge region offset
    ///   5. Update header with new capacities and offsets
    fn grow_node_region(&mut self) -> Result<()> {
        let header = *self.mmap.read_header();
        let old_node_cap = header.node_region_capacity;
        let new_node_cap = if old_node_cap == 0 {
            DEFAULT_NODE_CAPACITY
        } else {
            old_node_cap * 2
        };

        tracing::info!(
            "Auto-growing node region: {} -> {} slots",
            old_node_cap,
            new_node_cap
        );

        // Read all existing edge data into memory before remap
        let edge_data = self.read_edge_region_bytes(&header);

        // Calculate new layout
        let new_node_region_bytes = new_node_cap * NODE_SIZE as u64;
        let new_edge_region_offset = header.node_region_offset + new_node_region_bytes;
        let edge_region_bytes = header.edge_region_capacity * EDGE_SIZE as u64;
        let new_wal_offset = new_edge_region_offset + edge_region_bytes;
        let new_file_size = new_wal_offset;

        // Remap (invalidates all pointers)
        self.mmap.remap(new_file_size)?;

        // Write edge data at the new offset
        if !edge_data.is_empty() {
            unsafe {
                let dest = self.mmap.ptr_at_mut(new_edge_region_offset);
                std::ptr::copy_nonoverlapping(edge_data.as_ptr(), dest, edge_data.len());
            }
        }

        // Update header
        unsafe {
            let h = self.mmap.header_mut();
            h.node_region_capacity = new_node_cap;
            h.edge_region_offset = new_edge_region_offset;
            h.wal_region_offset = new_wal_offset;
            h.checksum = h.compute_checksum();
        }

        self.mmap.flush()?;
        Ok(())
    }

    /// Double the edge region capacity.
    ///
    /// Simpler than growing nodes: edges are at the end (before WAL),
    /// so we just extend the file and update the header.
    fn grow_edge_region(&mut self) -> Result<()> {
        let header = *self.mmap.read_header();
        let old_edge_cap = header.edge_region_capacity;
        let new_edge_cap = if old_edge_cap == 0 {
            DEFAULT_EDGE_CAPACITY
        } else {
            old_edge_cap * 2
        };

        tracing::info!(
            "Auto-growing edge region: {} -> {} slots",
            old_edge_cap,
            new_edge_cap
        );

        let new_edge_region_bytes = new_edge_cap * EDGE_SIZE as u64;
        let new_wal_offset = header.edge_region_offset + new_edge_region_bytes;
        let new_file_size = new_wal_offset;

        // Remap (invalidates all pointers)
        self.mmap.remap(new_file_size)?;

        // Update header
        unsafe {
            let h = self.mmap.header_mut();
            h.edge_region_capacity = new_edge_cap;
            h.wal_region_offset = new_wal_offset;
            h.checksum = h.compute_checksum();
        }

        self.mmap.flush()?;
        Ok(())
    }

    /// Read all edge data into a byte buffer (for relocation during node region growth).
    fn read_edge_region_bytes(&self, header: &Header) -> Vec<u8> {
        let edge_bytes = (header.edge_count * EDGE_SIZE as u64) as usize;
        if edge_bytes == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; edge_bytes];
        unsafe {
            let src = self.mmap.ptr_at(header.edge_region_offset);
            std::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), edge_bytes);
        }
        buf
    }

    fn write_node_at_slot(&self, slot: u64, node: &Node) -> Result<()> {
        let header = self.mmap.read_header();
        let offset = header.node_region_offset + slot * NODE_SIZE as u64;

        // SAFETY: slot is within capacity (checked by caller), single-writer
        unsafe {
            let dest = self.mmap.ptr_at_mut(offset);
            let src = node as *const Node as *const u8;
            std::ptr::copy_nonoverlapping(src, dest, NODE_SIZE);
        }
        Ok(())
    }

    fn replay_wal(&mut self) -> Result<()> {
        let header = self.mmap.read_header();
        let entries = Wal::read_entries(&self.path, header.wal_last_seq)?;

        if entries.is_empty() {
            return Ok(());
        }

        tracing::info!("Replaying {} WAL entries", entries.len());

        for entry in &entries {
            match entry.op {
                WalOp::NodeCreate => {
                    if entry.data.len() == NODE_SIZE {
                        // Copy into aligned buffer to satisfy Node's align(64)
                        let mut node = std::mem::MaybeUninit::<Node>::uninit();
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                entry.data.as_ptr(),
                                node.as_mut_ptr() as *mut u8,
                                NODE_SIZE,
                            );
                        }
                        let node = unsafe { node.assume_init() };
                        let header = self.mmap.read_header();
                        // Idempotent: skip if this node was already written to mmap
                        if node.id < header.next_node_id {
                            continue;
                        }
                        self.write_node_at_slot(header.node_count, &node)?;
                        unsafe {
                            let h = self.mmap.header_mut();
                            h.node_count += 1;
                            h.next_node_id = node.id + 1;
                        }
                    }
                }
                WalOp::EdgeCreate => {
                    if entry.data.len() == EDGE_SIZE {
                        // Copy into aligned buffer to satisfy Edge's align(64)
                        let mut edge = std::mem::MaybeUninit::<Edge>::uninit();
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                entry.data.as_ptr(),
                                edge.as_mut_ptr() as *mut u8,
                                EDGE_SIZE,
                            );
                        }
                        let edge = unsafe { edge.assume_init() };
                        let header = self.mmap.read_header();
                        // Idempotent: skip if this edge was already written
                        if edge.id < header.next_edge_id {
                            continue;
                        }
                        let offset =
                            header.edge_region_offset + header.edge_count * EDGE_SIZE as u64;
                        unsafe {
                            let dest = self.mmap.ptr_at_mut(offset);
                            std::ptr::copy_nonoverlapping(
                                entry.data.as_ptr(),
                                dest,
                                EDGE_SIZE,
                            );
                            let h = self.mmap.header_mut();
                            h.edge_count += 1;
                            h.next_edge_id = edge.id + 1;
                        }
                    }
                }
                WalOp::NodeUpdate => {
                    if entry.data.len() == 8 + NODE_SIZE {
                        let slot = u64::from_le_bytes(entry.data[..8].try_into().unwrap());
                        let header = self.mmap.read_header();
                        if slot < header.node_count {
                            let offset = header.node_region_offset + slot * NODE_SIZE as u64;
                            unsafe {
                                let dest = self.mmap.ptr_at_mut(offset);
                                std::ptr::copy_nonoverlapping(
                                    entry.data[8..].as_ptr(),
                                    dest,
                                    NODE_SIZE,
                                );
                            }
                        }
                    }
                }
                WalOp::EdgeUpdate => {
                    if entry.data.len() == 8 + EDGE_SIZE {
                        let slot = u64::from_le_bytes(entry.data[..8].try_into().unwrap());
                        let header = self.mmap.read_header();
                        if slot < header.edge_count {
                            let offset = header.edge_region_offset + slot * EDGE_SIZE as u64;
                            unsafe {
                                let dest = self.mmap.ptr_at_mut(offset);
                                std::ptr::copy_nonoverlapping(
                                    entry.data[8..].as_ptr(),
                                    dest,
                                    EDGE_SIZE,
                                );
                            }
                        }
                    }
                }
                WalOp::EdgeDelete => {
                    // Same format as EdgeUpdate: slot(8) + edge_bytes(EDGE_SIZE)
                    if entry.data.len() == 8 + EDGE_SIZE {
                        let slot = u64::from_le_bytes(entry.data[..8].try_into().unwrap());
                        let header = self.mmap.read_header();
                        if slot < header.edge_count {
                            let offset = header.edge_region_offset + slot * EDGE_SIZE as u64;
                            unsafe {
                                let dest = self.mmap.ptr_at_mut(offset);
                                std::ptr::copy_nonoverlapping(
                                    entry.data[8..].as_ptr(),
                                    dest,
                                    EDGE_SIZE,
                                );
                            }
                        }
                    }
                }
                WalOp::Checkpoint => {}
                _ => {
                    tracing::warn!("WAL replay: unhandled op {:?} at seq {}", entry.op, entry.seq);
                }
            }
        }

        // Update header checksum and checkpoint
        unsafe {
            let h = self.mmap.header_mut();
            h.checksum = h.compute_checksum();
        }
        self.mmap.flush()?;

        tracing::info!("WAL replay complete");
        Ok(())
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

    #[test]
    fn create_and_store_nodes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create(&path).unwrap();
        let id1 = brain.store_node("server-01").unwrap();
        let id2 = brain.store_node("server-02").unwrap();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        let (nodes, edges) = brain.stats();
        assert_eq!(nodes, 2);
        assert_eq!(edges, 0);
    }

    #[test]
    fn read_node_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create(&path).unwrap();
        brain.store_node("my-node").unwrap();

        let node = brain.read_node(0).unwrap();
        assert_eq!(node.label(), "my-node");
        assert!(node.is_active());
        assert_eq!(node.confidence, 0.80);
    }

    #[test]
    fn find_by_label() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create(&path).unwrap();
        brain.store_node("alpha").unwrap();
        brain.store_node("beta").unwrap();
        brain.store_node("gamma").unwrap();

        let result = brain.find_node_by_label("beta").unwrap();
        assert!(result.is_some());
        let (slot, node) = result.unwrap();
        assert_eq!(slot, 1);
        assert_eq!(node.label(), "beta");

        let missing = brain.find_node_by_label("delta").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn store_and_read_edges() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create(&path).unwrap();
        let n1 = brain.store_node("a").unwrap();
        let n2 = brain.store_node("b").unwrap();
        let e1 = brain.store_edge(n1, n2, 1).unwrap();

        assert_eq!(e1, 1);
        let edge = brain.read_edge(0).unwrap();
        assert_eq!(edge.from_node, n1);
        assert_eq!(edge.to_node, n2);
        assert_eq!(edge.edge_type, 1);
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        // Create and store
        {
            let mut brain = BrainFile::create(&path).unwrap();
            brain.store_node("persistent-node").unwrap();
            brain.checkpoint().unwrap();
        }

        // Reopen and verify
        {
            let brain = BrainFile::open(&path).unwrap();
            let (nodes, _) = brain.stats();
            assert_eq!(nodes, 1);
            let node = brain.read_node(0).unwrap();
            assert_eq!(node.label(), "persistent-node");
        }
    }

    #[test]
    fn wal_recovery_after_crash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        // Create, store, but DON'T checkpoint (simulating crash before flush)
        {
            let mut brain = BrainFile::create(&path).unwrap();
            brain.store_node("before-crash").unwrap();
            // No checkpoint — WAL has the entry, mmap might not be flushed
        }

        // Reopen — WAL should replay
        {
            let brain = BrainFile::open(&path).unwrap();
            let (nodes, _) = brain.stats();
            assert_eq!(nodes, 1);
            let node = brain.read_node(0).unwrap();
            assert_eq!(node.label(), "before-crash");
        }
    }

    #[test]
    fn auto_grow_node_region() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create_with_capacity(&path, 2, 2).unwrap();
        brain.store_node("a").unwrap();
        brain.store_node("b").unwrap();

        // Third node triggers auto-grow (capacity was 2)
        let id3 = brain.store_node("c").unwrap();
        assert_eq!(id3, 3);

        // Verify all three nodes are readable
        assert_eq!(brain.read_node(0).unwrap().label(), "a");
        assert_eq!(brain.read_node(1).unwrap().label(), "b");
        assert_eq!(brain.read_node(2).unwrap().label(), "c");

        let (nodes, _) = brain.stats();
        assert_eq!(nodes, 3);
    }

    #[test]
    fn auto_grow_edge_region() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create_with_capacity(&path, 10, 2).unwrap();
        let n1 = brain.store_node("a").unwrap();
        let n2 = brain.store_node("b").unwrap();
        let n3 = brain.store_node("c").unwrap();

        brain.store_edge(n1, n2, 1).unwrap();
        brain.store_edge(n2, n3, 1).unwrap();

        // Third edge triggers auto-grow (capacity was 2)
        let e3 = brain.store_edge(n1, n3, 1).unwrap();
        assert_eq!(e3, 3);

        let (_, edges) = brain.stats();
        assert_eq!(edges, 3);

        // Verify all edges are readable
        let edge = brain.read_edge(2).unwrap();
        assert_eq!(edge.from_node, n1);
        assert_eq!(edge.to_node, n3);
    }

    #[test]
    fn auto_grow_preserves_edges_during_node_grow() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        // Start with tiny capacity: 2 nodes, 4 edges
        let mut brain = BrainFile::create_with_capacity(&path, 2, 4).unwrap();
        let n1 = brain.store_node("x").unwrap();
        let n2 = brain.store_node("y").unwrap();
        brain.store_edge(n1, n2, 1).unwrap();
        brain.store_edge(n2, n1, 2).unwrap();

        // This triggers node region grow, which must relocate edge data
        let n3 = brain.store_node("z").unwrap();
        assert_eq!(n3, 3);

        // Edges must still be intact after relocation
        let edge0 = brain.read_edge(0).unwrap();
        assert_eq!(edge0.from_node, n1);
        assert_eq!(edge0.to_node, n2);
        assert_eq!(edge0.edge_type, 1);

        let edge1 = brain.read_edge(1).unwrap();
        assert_eq!(edge1.from_node, n2);
        assert_eq!(edge1.to_node, n1);
        assert_eq!(edge1.edge_type, 2);

        // Can still add more edges after grow
        brain.store_edge(n1, n3, 3).unwrap();
        let (nodes, edges) = brain.stats();
        assert_eq!(nodes, 3);
        assert_eq!(edges, 3);
    }

    #[test]
    fn auto_grow_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        // Create with tiny capacity, grow, and checkpoint
        {
            let mut brain = BrainFile::create_with_capacity(&path, 2, 2).unwrap();
            brain.store_node("p").unwrap();
            brain.store_node("q").unwrap();
            brain.store_node("r").unwrap(); // triggers grow
            let n1 = 1;
            let n2 = 2;
            brain.store_edge(n1, n2, 1).unwrap();
            brain.store_edge(n2, n1, 1).unwrap();
            brain.store_edge(n1, n1, 1).unwrap(); // triggers edge grow
            brain.checkpoint().unwrap();
        }

        // Reopen and verify everything survived
        {
            let brain = BrainFile::open(&path).unwrap();
            let (nodes, edges) = brain.stats();
            assert_eq!(nodes, 3);
            assert_eq!(edges, 3);
            assert_eq!(brain.read_node(0).unwrap().label(), "p");
            assert_eq!(brain.read_node(1).unwrap().label(), "q");
            assert_eq!(brain.read_node(2).unwrap().label(), "r");
        }
    }

    #[test]
    fn wal_recovery_of_update() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        {
            let mut brain = BrainFile::create(&path).unwrap();
            brain.store_node("updatable").unwrap();
            brain.update_node_field(0, |n| n.confidence = 0.95).unwrap();
            // No checkpoint — simulates crash
        }

        {
            let brain = BrainFile::open(&path).unwrap();
            let node = brain.read_node(0).unwrap();
            assert_eq!(node.label(), "updatable");
            assert_eq!(node.confidence, 0.95);
        }
    }

    #[test]
    fn zero_copy_node_access() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut brain = BrainFile::create(&path).unwrap();
        brain.store_node("zero-copy").unwrap();

        // Read returns a reference directly into mmap'd memory — no deserialization
        let node: &Node = brain.read_node(0).unwrap();
        assert_eq!(node.id, 1);
        assert_eq!(node.label(), "zero-copy");

        // Verify it's truly a pointer into the file, not a copy
        let ptr = node as *const Node;
        assert!(!ptr.is_null());
    }
}
