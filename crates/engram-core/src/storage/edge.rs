/// Edge structure -- 80 bytes, fixed size, zero-copy via mmap.
///
/// Temporal bounds (`valid_from`, `valid_to`) are stored directly in the struct
/// for zero-cost filtering during traversal. Arbitrary edge metadata (version,
/// qualifiers, source details) lives in the edge property store (`.brain.edge_props`).

pub const EDGE_SIZE: usize = 72;

/// Edge flag: soft-deleted.
pub const FLAG_EDGE_DELETED: u32 = 1 << 0;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// Unique edge ID
    pub id: u64,
    /// Relationship type registry index
    pub edge_type: u32,
    /// Flags: directed, bidirectional, etc.
    pub flags: u32,
    /// Source node ID
    pub from_node: u64,
    /// Target node ID
    pub to_node: u64,
    /// Confidence score 0.0 - 1.0
    pub confidence: f32,
    /// Padding for alignment
    pub _pad1: u32,
    /// Creation timestamp (unix seconds)
    pub created_at: i64,
    /// Provenance source ID
    pub source_id: u64,
    /// Temporal validity start (unix seconds, 0 = unset/unbounded)
    pub valid_from: i64,
    /// Temporal validity end (unix seconds, 0 = unset/unbounded = still current)
    pub valid_to: i64,
}

const _: () = assert!(std::mem::size_of::<Edge>() == EDGE_SIZE);

impl Edge {
    pub fn new(id: u64, from: u64, to: u64, edge_type: u32, now: i64) -> Self {
        Edge {
            id,
            edge_type,
            flags: 0,
            from_node: from,
            to_node: to,
            confidence: 0.80,
            _pad1: 0,
            created_at: now,
            source_id: 0,
            valid_from: 0,
            valid_to: 0,
        }
    }

    /// Returns true if this edge is not deleted.
    pub fn is_active(&self) -> bool {
        self.flags & FLAG_EDGE_DELETED == 0
    }

    /// Returns true if this edge has been soft-deleted.
    pub fn is_deleted(&self) -> bool {
        self.flags & FLAG_EDGE_DELETED != 0
    }

    /// Mark this edge as soft-deleted.
    pub fn soft_delete(&mut self) {
        self.flags |= FLAG_EDGE_DELETED;
        self.confidence = 0.0;
    }

    /// Returns true if this edge has temporal bounds set.
    pub fn has_temporal(&self) -> bool {
        self.valid_from != 0 || self.valid_to != 0
    }

    /// Returns true if this edge is currently valid (no end date, or end date in future).
    pub fn is_current(&self, now: i64) -> bool {
        self.valid_to == 0 || self.valid_to > now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_size_is_72() {
        assert_eq!(std::mem::size_of::<Edge>(), 72);
    }

    #[test]
    fn zero_copy_roundtrip() {
        let edge = Edge::new(1, 10, 20, 5, 999);
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(&edge as *const Edge as *const u8, EDGE_SIZE) };
        let recovered: &Edge = unsafe { &*(bytes.as_ptr() as *const Edge) };
        assert_eq!(recovered.id, 1);
        assert_eq!(recovered.from_node, 10);
        assert_eq!(recovered.to_node, 20);
        assert_eq!(recovered.edge_type, 5);
        assert_eq!(recovered.valid_from, 0);
        assert_eq!(recovered.valid_to, 0);
    }

    #[test]
    fn temporal_helpers() {
        let mut edge = Edge::new(1, 10, 20, 5, 1000);
        assert!(!edge.has_temporal());
        assert!(edge.is_current(999999));

        edge.valid_from = 1000;
        edge.valid_to = 2000;
        assert!(edge.has_temporal());
        assert!(edge.is_current(1500));
        assert!(!edge.is_current(2500));
    }
}
