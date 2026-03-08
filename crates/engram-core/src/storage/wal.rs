/// Write-Ahead Log for crash recovery.
///
/// WAL entries are appended to a separate file (.brain.wal).
/// On startup, uncommitted entries are replayed. After checkpoint,
/// the WAL file is truncated.

use crate::storage::error::{Result, StorageError};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// WAL operation types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WalOp {
    NodeCreate = 0x01,
    NodeUpdate = 0x02,
    NodeDelete = 0x03,
    EdgeCreate = 0x04,
    EdgeUpdate = 0x05,
    EdgeDelete = 0x06,
    Checkpoint = 0x0A,
}

impl TryFrom<u8> for WalOp {
    type Error = StorageError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x01 => Ok(WalOp::NodeCreate),
            0x02 => Ok(WalOp::NodeUpdate),
            0x03 => Ok(WalOp::NodeDelete),
            0x04 => Ok(WalOp::EdgeCreate),
            0x05 => Ok(WalOp::EdgeUpdate),
            0x06 => Ok(WalOp::EdgeDelete),
            0x0A => Ok(WalOp::Checkpoint),
            _ => Err(StorageError::WalCorrupt {
                seq: 0,
                reason: format!("unknown op type: {:#x}", value),
            }),
        }
    }
}

/// A single WAL entry on disk:
/// | seq: u64 | op: u8 | data_len: u32 | data: [u8; N] | checksum: u32 |
#[derive(Debug, Clone)]
pub struct WalEntry {
    pub seq: u64,
    pub op: WalOp,
    pub data: Vec<u8>,
}

const _WAL_ENTRY_HEADER_SIZE: usize = 8 + 1 + 4; // seq + op + data_len
const _WAL_ENTRY_FOOTER_SIZE: usize = 4; // checksum

pub struct Wal {
    path: PathBuf,
    writer: BufWriter<File>,
    next_seq: u64,
}

impl Wal {
    /// Open or create the WAL file.
    pub fn open(brain_path: &Path, last_seq: u64) -> Result<Self> {
        let path = brain_path.with_extension("brain.wal");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        Ok(Wal {
            path,
            writer: BufWriter::new(file),
            next_seq: last_seq + 1,
        })
    }

    /// Append an entry to the WAL. Returns the sequence number.
    pub fn append(&mut self, op: WalOp, data: &[u8]) -> Result<u64> {
        let seq = self.next_seq;
        self.next_seq += 1;

        // Write: seq(8) + op(1) + data_len(4) + data(N) + checksum(4)
        self.writer.write_all(&seq.to_le_bytes())?;
        self.writer.write_all(&[op as u8])?;
        self.writer.write_all(&(data.len() as u32).to_le_bytes())?;
        self.writer.write_all(data)?;

        let checksum = compute_wal_checksum(seq, op as u8, data);
        self.writer.write_all(&checksum.to_le_bytes())?;
        self.writer.flush()?;

        Ok(seq)
    }

    /// Write a checkpoint marker and sync.
    pub fn checkpoint(&mut self) -> Result<u64> {
        self.append(WalOp::Checkpoint, &[])
    }

    /// Read all entries after `after_seq` for replay.
    pub fn read_entries(brain_path: &Path, after_seq: u64) -> Result<Vec<WalEntry>> {
        let path = brain_path.with_extension("brain.wal");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();

        loop {
            // Try reading entry header
            let mut seq_buf = [0u8; 8];
            match reader.read_exact(&mut seq_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let seq = u64::from_le_bytes(seq_buf);

            let mut op_buf = [0u8; 1];
            reader.read_exact(&mut op_buf)?;
            let op = WalOp::try_from(op_buf[0])?;

            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf)?;
            let data_len = u32::from_le_bytes(len_buf) as usize;

            let mut data = vec![0u8; data_len];
            reader.read_exact(&mut data)?;

            let mut checksum_buf = [0u8; 4];
            reader.read_exact(&mut checksum_buf)?;
            let stored_checksum = u32::from_le_bytes(checksum_buf);

            let computed = compute_wal_checksum(seq, op_buf[0], &data);
            if stored_checksum != computed {
                return Err(StorageError::WalChecksumMismatch { seq });
            }

            if seq > after_seq {
                entries.push(WalEntry { seq, op, data });
            }
        }

        Ok(entries)
    }

    /// Truncate the WAL file (called after successful checkpoint + flush).
    pub fn truncate(&mut self) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        self.writer = BufWriter::new(file);
        Ok(())
    }
}

fn compute_wal_checksum(seq: u64, op: u8, data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in seq.to_le_bytes().iter().chain(&[op]).chain(data.iter()) {
        crc ^= b as u32;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_entries() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");

        // Write entries
        {
            let mut wal = Wal::open(&brain_path, 0).unwrap();
            let seq1 = wal.append(WalOp::NodeCreate, b"node1-data").unwrap();
            let seq2 = wal.append(WalOp::EdgeCreate, b"edge1-data").unwrap();
            let seq3 = wal.checkpoint().unwrap();
            assert_eq!(seq1, 1);
            assert_eq!(seq2, 2);
            assert_eq!(seq3, 3);
        }

        // Read all entries
        let entries = Wal::read_entries(&brain_path, 0).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].op, WalOp::NodeCreate);
        assert_eq!(entries[0].data, b"node1-data");
        assert_eq!(entries[1].op, WalOp::EdgeCreate);
        assert_eq!(entries[2].op, WalOp::Checkpoint);
    }

    #[test]
    fn read_after_seq_filters() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");

        {
            let mut wal = Wal::open(&brain_path, 0).unwrap();
            wal.append(WalOp::NodeCreate, b"old").unwrap();
            wal.append(WalOp::NodeCreate, b"new").unwrap();
        }

        // Only entries after seq 1
        let entries = Wal::read_entries(&brain_path, 1).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].data, b"new");
    }

    #[test]
    fn truncate_clears_wal() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");

        {
            let mut wal = Wal::open(&brain_path, 0).unwrap();
            wal.append(WalOp::NodeCreate, b"data").unwrap();
            wal.truncate().unwrap();
        }

        let entries = Wal::read_entries(&brain_path, 0).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn checksum_catches_corruption() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");
        let wal_path = brain_path.with_extension("brain.wal");

        {
            let mut wal = Wal::open(&brain_path, 0).unwrap();
            wal.append(WalOp::NodeCreate, b"data").unwrap();
        }

        // Corrupt a byte in the WAL file
        {
            let mut data = std::fs::read(&wal_path).unwrap();
            if let Some(byte) = data.get_mut(10) {
                *byte ^= 0xFF;
            }
            std::fs::write(&wal_path, data).unwrap();
        }

        let result = Wal::read_entries(&brain_path, 0);
        assert!(result.is_err());
    }
}
