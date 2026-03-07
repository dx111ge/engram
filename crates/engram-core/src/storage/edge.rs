/// Edge structure — 64 bytes, fixed size, zero-copy via mmap.

pub const EDGE_SIZE: usize = 64;

#[repr(C, align(64))]
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
    /// Padding
    pub _pad1: u32,
    /// Creation timestamp (unix nanos)
    pub created_at: i64,
    /// Provenance source ID
    pub source_id: u64,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_size_is_64() {
        assert_eq!(std::mem::size_of::<Edge>(), 64);
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
    }
}
