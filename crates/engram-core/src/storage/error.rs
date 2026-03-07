use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid .brain file: {reason}")]
    InvalidFile { reason: String },

    #[error("corrupted header: expected magic {expected:?}, got {got:?}")]
    BadMagic { expected: [u8; 8], got: [u8; 8] },

    #[error("unsupported version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("WAL corrupted at sequence {seq}: {reason}")]
    WalCorrupt { seq: u64, reason: String },

    #[error("WAL checksum mismatch at sequence {seq}")]
    WalChecksumMismatch { seq: u64 },

    #[error("node {id} not found")]
    NodeNotFound { id: u64 },

    #[error("node region full (capacity: {capacity})")]
    NodeRegionFull { capacity: u64 },

    #[error("edge region full (capacity: {capacity})")]
    EdgeRegionFull { capacity: u64 },

    #[error("file is locked by another process")]
    FileLocked,

    #[error("mmap failed: {reason}")]
    MmapFailed { reason: String },
}

pub type Result<T> = std::result::Result<T, StorageError>;
