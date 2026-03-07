/// .brain file header -- first 4096 bytes (one page)
///
/// SAFETY: This struct is read/written directly from/to mmap'd memory.
/// It must be repr(C) with a fixed, stable layout. All fields are explicitly
/// ordered and padded to avoid implicit compiler padding.

pub const MAGIC: [u8; 8] = *b"ENGRAM\0\0";
pub const VERSION: u32 = 1;
pub const HEADER_SIZE: u64 = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// Magic bytes: "ENGRAM\0\0"
    pub magic: [u8; 8],           // 8  (offset 0)
    /// File format version
    pub version: u32,             // 4  (offset 8)
    /// Explicit padding after u32 to align next u64
    pub _pad0: u32,               // 4  (offset 12)
    /// Total number of active nodes
    pub node_count: u64,          // 8  (offset 16)
    /// Total number of active edges
    pub edge_count: u64,          // 8  (offset 24)
    /// Next available node ID
    pub next_node_id: u64,        // 8  (offset 32)
    /// Next available edge ID
    pub next_edge_id: u64,        // 8  (offset 40)
    /// Offset in file where node region starts
    pub node_region_offset: u64,  // 8  (offset 48)
    /// Maximum number of nodes the node region can hold
    pub node_region_capacity: u64, // 8 (offset 56)
    /// Offset in file where edge region starts
    pub edge_region_offset: u64,  // 8  (offset 64)
    /// Maximum number of edges the edge region can hold
    pub edge_region_capacity: u64, // 8 (offset 72)
    /// Offset in file where WAL region starts
    pub wal_region_offset: u64,   // 8  (offset 80)
    /// Size of WAL region in bytes
    pub wal_region_size: u64,     // 8  (offset 88)
    /// Last committed WAL sequence number
    pub wal_last_seq: u64,        // 8  (offset 96)
    /// CRC32 checksum of this header (excluding this field)
    pub checksum: u32,            // 4  (offset 104)
    /// Explicit padding
    pub _pad1: u32,               // 4  (offset 108)
    /// Reserved for future use   // 3984 (offset 112)
    pub _reserved: [u8; 3984],
}

// Total: 112 + 3984 = 4096
const _: () = assert!(std::mem::size_of::<Header>() == HEADER_SIZE as usize);

impl Header {
    pub fn new(node_capacity: u64, edge_capacity: u64) -> Self {
        let node_region_offset = HEADER_SIZE;
        let node_region_bytes = node_capacity * crate::storage::node::NODE_SIZE as u64;
        let edge_region_offset = node_region_offset + node_region_bytes;
        let edge_region_bytes = edge_capacity * crate::storage::edge::EDGE_SIZE as u64;
        let wal_region_offset = edge_region_offset + edge_region_bytes;

        let mut header = Header {
            magic: MAGIC,
            version: VERSION,
            _pad0: 0,
            node_count: 0,
            edge_count: 0,
            next_node_id: 1,
            next_edge_id: 1,
            node_region_offset,
            node_region_capacity: node_capacity,
            edge_region_offset,
            edge_region_capacity: edge_capacity,
            wal_region_offset,
            wal_region_size: 0,
            wal_last_seq: 0,
            checksum: 0,
            _pad1: 0,
            _reserved: [0u8; 3984],
        };
        header.checksum = header.compute_checksum();
        header
    }

    pub fn compute_checksum(&self) -> u32 {
        // SAFETY: Header is repr(C) with known size
        let bytes = unsafe {
            std::slice::from_raw_parts(self as *const Header as *const u8, HEADER_SIZE as usize)
        };
        let checksum_offset = std::mem::offset_of!(Header, checksum);
        let mut crc: u32 = 0xFFFFFFFF;
        for (i, &byte) in bytes.iter().enumerate() {
            if i >= checksum_offset && i < checksum_offset + 4 {
                continue;
            }
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xFFFFFFFF
    }

    pub fn validate(&self) -> crate::storage::error::Result<()> {
        if self.magic != MAGIC {
            return Err(crate::storage::error::StorageError::BadMagic {
                expected: MAGIC,
                got: self.magic,
            });
        }
        if self.version != VERSION {
            return Err(crate::storage::error::StorageError::UnsupportedVersion {
                version: self.version,
            });
        }
        let expected = self.compute_checksum();
        if self.checksum != expected {
            return Err(crate::storage::error::StorageError::InvalidFile {
                reason: format!(
                    "header checksum mismatch: stored={:#x}, computed={:#x}",
                    self.checksum, expected
                ),
            });
        }
        Ok(())
    }

    pub fn total_file_size(&self) -> u64 {
        self.wal_region_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_4096() {
        assert_eq!(std::mem::size_of::<Header>(), 4096);
    }

    #[test]
    fn new_header_validates() {
        let header = Header::new(1024, 4096);
        assert!(header.validate().is_ok());
    }

    #[test]
    fn corrupted_magic_fails_validation() {
        let mut header = Header::new(1024, 4096);
        header.magic[0] = b'X';
        assert!(header.validate().is_err());
    }

    #[test]
    fn corrupted_checksum_fails_validation() {
        let mut header = Header::new(1024, 4096);
        header.checksum ^= 0xFF;
        assert!(header.validate().is_err());
    }

    #[test]
    fn region_offsets_are_contiguous() {
        let header = Header::new(1024, 4096);
        assert_eq!(header.node_region_offset, HEADER_SIZE);
        let expected_edge_offset =
            HEADER_SIZE + 1024 * crate::storage::node::NODE_SIZE as u64;
        assert_eq!(header.edge_region_offset, expected_edge_offset);
    }
}
