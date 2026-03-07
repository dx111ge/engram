/// Property store — variable-length key-value pairs attached to nodes.
///
/// Uses a sidecar `.brain.props` file with a simple binary format.
/// In-memory HashMap for fast access, serialized on checkpoint, loaded on open.

use crate::storage::error::{Result, StorageError};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

pub struct PropertyStore {
    path: PathBuf,
    /// node_slot -> (key -> value)
    props: HashMap<u64, HashMap<String, String>>,
}

impl PropertyStore {
    /// Create a new empty property store.
    pub fn new(brain_path: &Path) -> Self {
        PropertyStore {
            path: brain_path.with_extension("brain.props"),
            props: HashMap::new(),
        }
    }

    /// Load properties from the sidecar file.
    pub fn load(brain_path: &Path) -> Result<Self> {
        let path = brain_path.with_extension("brain.props");
        let mut store = PropertyStore {
            path,
            props: HashMap::new(),
        };

        if store.path.exists() {
            store.read_from_file()?;
        }

        Ok(store)
    }

    /// Set a property on a node slot.
    pub fn set(&mut self, slot: u64, key: &str, value: &str) {
        self.props
            .entry(slot)
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// Get a property value.
    pub fn get(&self, slot: u64, key: &str) -> Option<&str> {
        self.props
            .get(&slot)
            .and_then(|m| m.get(key))
            .map(|s| s.as_str())
    }

    /// Get all properties for a node slot.
    pub fn get_all(&self, slot: u64) -> Option<&HashMap<String, String>> {
        self.props.get(&slot)
    }

    /// Remove a single property.
    pub fn remove(&mut self, slot: u64, key: &str) -> bool {
        if let Some(m) = self.props.get_mut(&slot) {
            let removed = m.remove(key).is_some();
            if m.is_empty() {
                self.props.remove(&slot);
            }
            removed
        } else {
            false
        }
    }

    /// Remove all properties for a node slot.
    pub fn remove_all(&mut self, slot: u64) {
        self.props.remove(&slot);
    }

    /// Persist to sidecar file.
    pub fn flush(&self) -> Result<()> {
        let file = File::create(&self.path)?;
        let mut w = BufWriter::new(file);

        // Header: entry_count (u32)
        let count = self.props.len() as u32;
        w.write_all(&count.to_le_bytes())?;

        for (&slot, pairs) in &self.props {
            // slot: u64
            w.write_all(&slot.to_le_bytes())?;
            // pair_count: u32
            let pair_count = pairs.len() as u32;
            w.write_all(&pair_count.to_le_bytes())?;

            for (key, value) in pairs {
                // key_len: u16, key: bytes
                let key_bytes = key.as_bytes();
                w.write_all(&(key_bytes.len() as u16).to_le_bytes())?;
                w.write_all(key_bytes)?;
                // value_len: u32, value: bytes
                let val_bytes = value.as_bytes();
                w.write_all(&(val_bytes.len() as u32).to_le_bytes())?;
                w.write_all(val_bytes)?;
            }
        }

        w.flush()?;
        Ok(())
    }

    fn read_from_file(&mut self) -> Result<()> {
        let file = File::open(&self.path)?;
        let mut r = BufReader::new(file);

        let mut buf4 = [0u8; 4];
        let mut buf8 = [0u8; 8];
        let mut buf2 = [0u8; 2];

        // entry_count
        if r.read_exact(&mut buf4).is_err() {
            return Ok(()); // empty file
        }
        let entry_count = u32::from_le_bytes(buf4) as usize;

        for _ in 0..entry_count {
            r.read_exact(&mut buf8)?;
            let slot = u64::from_le_bytes(buf8);

            r.read_exact(&mut buf4)?;
            let pair_count = u32::from_le_bytes(buf4) as usize;

            let mut pairs = HashMap::with_capacity(pair_count);
            for _ in 0..pair_count {
                // key
                r.read_exact(&mut buf2)?;
                let key_len = u16::from_le_bytes(buf2) as usize;
                let mut key_buf = vec![0u8; key_len];
                r.read_exact(&mut key_buf)?;
                let key = String::from_utf8(key_buf)
                    .map_err(|_| StorageError::InvalidFile { reason: "invalid UTF-8 in property data".into() })?;

                // value
                r.read_exact(&mut buf4)?;
                let val_len = u32::from_le_bytes(buf4) as usize;
                let mut val_buf = vec![0u8; val_len];
                r.read_exact(&mut val_buf)?;
                let value = String::from_utf8(val_buf)
                    .map_err(|_| StorageError::InvalidFile { reason: "invalid UTF-8 in property data".into() })?;

                pairs.insert(key, value);
            }

            self.props.insert(slot, pairs);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn set_and_get() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");
        let mut store = PropertyStore::new(&brain_path);

        store.set(0, "role", "database");
        store.set(0, "version", "16");
        store.set(1, "os", "linux");

        assert_eq!(store.get(0, "role"), Some("database"));
        assert_eq!(store.get(0, "version"), Some("16"));
        assert_eq!(store.get(1, "os"), Some("linux"));
        assert_eq!(store.get(0, "missing"), None);
        assert_eq!(store.get(99, "role"), None);
    }

    #[test]
    fn get_all_properties() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");
        let mut store = PropertyStore::new(&brain_path);

        store.set(0, "a", "1");
        store.set(0, "b", "2");

        let all = store.get_all(0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["a"], "1");
        assert_eq!(all["b"], "2");

        assert!(store.get_all(99).is_none());
    }

    #[test]
    fn remove_property() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");
        let mut store = PropertyStore::new(&brain_path);

        store.set(0, "key", "value");
        assert!(store.remove(0, "key"));
        assert_eq!(store.get(0, "key"), None);
        assert!(!store.remove(0, "key"));
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");

        {
            let mut store = PropertyStore::new(&brain_path);
            store.set(0, "role", "database");
            store.set(0, "version", "16");
            store.set(5, "hostname", "server-01");
            store.flush().unwrap();
        }

        {
            let store = PropertyStore::load(&brain_path).unwrap();
            assert_eq!(store.get(0, "role"), Some("database"));
            assert_eq!(store.get(0, "version"), Some("16"));
            assert_eq!(store.get(5, "hostname"), Some("server-01"));
        }
    }

    #[test]
    fn unicode_properties() {
        let dir = TempDir::new().unwrap();
        let brain_path = dir.path().join("test.brain");

        let mut store = PropertyStore::new(&brain_path);
        store.set(0, "description", "Datenbank-Server");
        store.flush().unwrap();

        let store = PropertyStore::load(&brain_path).unwrap();
        assert_eq!(store.get(0, "description"), Some("Datenbank-Server"));
    }
}
