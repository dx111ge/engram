/// Node structure — 256 bytes, fixed size, zero-copy via mmap.
///
/// SAFETY: repr(C) with explicit alignment ensures stable memory layout
/// for direct pointer casting from mmap'd regions.

pub const NODE_SIZE: usize = 256;

/// Node flags
pub const FLAG_ACTIVE: u32 = 1 << 0;
pub const FLAG_DELETED: u32 = 1 << 1;
pub const FLAG_LOCKED: u32 = 1 << 2;

/// Memory tiers
pub const TIER_CORE: u8 = 0; // Always in LLM context
pub const TIER_ACTIVE: u8 = 1; // Recently relevant
pub const TIER_ARCHIVAL: u8 = 2; // Search-only

/// Sensitivity levels
pub const SENSITIVITY_PUBLIC: u8 = 0;
pub const SENSITIVITY_INTERNAL: u8 = 1;
pub const SENSITIVITY_CONFIDENTIAL: u8 = 2;
pub const SENSITIVITY_RESTRICTED: u8 = 3;

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Node {
    /// Unique node ID (0 = null/unused)
    pub id: u64,
    /// Type registry index
    pub node_type: u32,
    /// Status flags (active, deleted, locked)
    pub flags: u32,
    /// When this node was ingested (unix nanos)
    pub created_at: i64,
    /// Last modification timestamp
    pub updated_at: i64,
    /// When the real-world event occurred (bi-temporal)
    pub event_time: i64,
    /// Confidence score 0.0 - 1.0
    pub confidence: f32,
    /// Access count for LRU and reinforcement
    pub access_count: u32,
    /// Last access timestamp for decay calculation
    pub last_accessed: i64,
    /// Memory tier: core(0), active(1), archival(2)
    pub memory_tier: u8,
    /// Sensitivity: public(0), internal(1), confidential(2), restricted(3)
    pub sensitivity: u8,
    /// Padding for alignment
    pub _pad1: [u8; 6],
    /// Provenance — source ID
    pub source_id: u64,
    /// Pointer to outgoing edge list in edge region
    pub edge_out_ptr: u64,
    /// Number of outgoing edges
    pub edge_out_count: u32,
    /// Padding
    pub _pad2: u32,
    /// Pointer to incoming edge list in edge region
    pub edge_in_ptr: u64,
    /// Number of incoming edges
    pub edge_in_count: u32,
    /// Padding
    pub _pad3: u32,
    /// Pointer to property block in property region
    pub prop_ptr: u64,
    /// Property data size in bytes
    pub prop_size: u32,
    /// Padding
    pub _pad4: u32,
    /// Pointer to embedding vector
    pub embed_ptr: u64,
    /// Embedding dimensions (e.g. 384, 768)
    pub embed_dim: u16,
    /// Padding
    pub _pad5: [u8; 6],
    /// Hash of primary label for fast lookup
    pub label_hash: u64,
    /// Inline label (short labels avoid property region lookup)
    pub label_inline: [u8; 48],
    /// Reserved for future use
    pub _reserved: [u8; 16],
}

const _: () = assert!(std::mem::size_of::<Node>() == NODE_SIZE);

impl Node {
    pub fn new(id: u64, label: &str, now: i64) -> Self {
        let mut node = Node {
            id,
            node_type: 0,
            flags: FLAG_ACTIVE,
            created_at: now,
            updated_at: now,
            event_time: now,
            confidence: 0.80,
            access_count: 0,
            last_accessed: now,
            memory_tier: TIER_ACTIVE,
            sensitivity: SENSITIVITY_INTERNAL,
            _pad1: [0; 6],
            source_id: 0,
            edge_out_ptr: 0,
            edge_out_count: 0,
            _pad2: 0,
            edge_in_ptr: 0,
            edge_in_count: 0,
            _pad3: 0,
            prop_ptr: 0,
            prop_size: 0,
            _pad4: 0,
            embed_ptr: 0,
            embed_dim: 0,
            _pad5: [0; 6],
            label_hash: hash_label(label),
            label_inline: [0u8; 48],
            _reserved: [0u8; 16],
        };
        let label_bytes = label.as_bytes();
        let copy_len = label_bytes.len().min(47); // leave room for null terminator
        node.label_inline[..copy_len].copy_from_slice(&label_bytes[..copy_len]);
        node
    }

    pub fn is_active(&self) -> bool {
        self.flags & FLAG_ACTIVE != 0 && self.flags & FLAG_DELETED == 0
    }

    pub fn is_deleted(&self) -> bool {
        self.flags & FLAG_DELETED != 0
    }

    pub fn label(&self) -> &str {
        let end = self
            .label_inline
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.label_inline.len());
        std::str::from_utf8(&self.label_inline[..end]).unwrap_or("")
    }

    pub fn soft_delete(&mut self, now: i64) {
        self.flags |= FLAG_DELETED;
        self.flags &= !FLAG_ACTIVE;
        self.confidence = 0.0;
        self.updated_at = now;
    }
}

/// FNV-1a hash for label lookup (case-insensitive).
///
/// Labels are conceptually case-insensitive: "PostgreSQL", "postgresql", and
/// "Postgresql" all refer to the same entity. The hash folds ASCII uppercase
/// to lowercase so they land in the same bucket.
pub fn hash_label(label: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in label.as_bytes() {
        // Fold ASCII uppercase to lowercase for case-insensitive hashing
        let b = byte.to_ascii_lowercase();
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Case-insensitive label comparison.
#[inline]
pub fn labels_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_size_is_256() {
        assert_eq!(std::mem::size_of::<Node>(), 256);
    }

    #[test]
    fn new_node_is_active() {
        let node = Node::new(1, "test-node", 1000);
        assert!(node.is_active());
        assert!(!node.is_deleted());
        assert_eq!(node.id, 1);
        assert_eq!(node.label(), "test-node");
    }

    #[test]
    fn soft_delete_marks_inactive() {
        let mut node = Node::new(1, "test", 1000);
        node.soft_delete(2000);
        assert!(!node.is_active());
        assert!(node.is_deleted());
        assert_eq!(node.confidence, 0.0);
        assert_eq!(node.updated_at, 2000);
    }

    #[test]
    fn label_truncates_at_47() {
        let long_label = "a".repeat(100);
        let node = Node::new(1, &long_label, 1000);
        assert_eq!(node.label().len(), 47);
    }

    #[test]
    fn label_hash_is_deterministic() {
        assert_eq!(hash_label("server-01"), hash_label("server-01"));
        assert_ne!(hash_label("server-01"), hash_label("server-02"));
    }

    #[test]
    fn label_hash_is_case_insensitive() {
        assert_eq!(hash_label("PostgreSQL"), hash_label("postgresql"));
        assert_eq!(hash_label("PostgreSQL"), hash_label("Postgresql"));
        assert_eq!(hash_label("Redis"), hash_label("redis"));
        assert_eq!(hash_label("REDIS"), hash_label("redis"));
    }

    #[test]
    fn labels_eq_is_case_insensitive() {
        assert!(labels_eq("PostgreSQL", "postgresql"));
        assert!(labels_eq("PostgreSQL", "Postgresql"));
        assert!(labels_eq("Redis", "REDIS"));
        assert!(!labels_eq("Redis", "Memcached"));
    }

    #[test]
    fn zero_copy_roundtrip() {
        let node = Node::new(42, "zero-copy-test", 1234567890);
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(&node as *const Node as *const u8, NODE_SIZE)
        };
        let recovered: &Node = unsafe { &*(bytes.as_ptr() as *const Node) };
        assert_eq!(recovered.id, 42);
        assert_eq!(recovered.label(), "zero-copy-test");
        assert_eq!(recovered.created_at, 1234567890);
    }
}
