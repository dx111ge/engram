/// Edge type registry — maps type names to u32 IDs.
///
/// Persisted to a `.brain.types` sidecar file.
/// One type name per line, line number (0-based) = type_id.

use crate::storage::error::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct TypeRegistry {
    path: PathBuf,
    names: Vec<String>,
    lookup: HashMap<String, u32>,
}

impl TypeRegistry {
    pub fn new(brain_path: &Path) -> Self {
        TypeRegistry {
            path: brain_path.with_extension("brain.types"),
            names: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    pub fn load(brain_path: &Path) -> Result<Self> {
        let path = brain_path.with_extension("brain.types");
        let mut reg = TypeRegistry {
            path,
            names: Vec::new(),
            lookup: HashMap::new(),
        };

        if reg.path.exists() {
            let file = File::open(&reg.path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let name = line?;
                let id = reg.names.len() as u32;
                reg.lookup.insert(name.clone(), id);
                reg.names.push(name);
            }
        }

        Ok(reg)
    }

    /// Look up a type ID by name, without creating it.
    pub fn get(&self, name: &str) -> Option<u32> {
        self.lookup.get(name).copied()
    }

    /// Get or create a type ID for a name.
    pub fn get_or_create(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.lookup.get(name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        self.lookup.insert(name.to_string(), id);
        id
    }

    /// Get the name for a type ID.
    pub fn name(&self, type_id: u32) -> Option<&str> {
        self.names.get(type_id as usize).map(|s| s.as_str())
    }

    /// Get the name or a fallback string.
    pub fn name_or_default(&self, type_id: u32) -> String {
        self.names
            .get(type_id as usize)
            .cloned()
            .unwrap_or_else(|| format!("type_{type_id}"))
    }

    /// Persist to sidecar file.
    pub fn flush(&self) -> Result<()> {
        let file = File::create(&self.path)?;
        let mut w = BufWriter::new(file);
        for name in &self.names {
            writeln!(w, "{name}")?;
        }
        w.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn get_or_create_assigns_sequential_ids() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut reg = TypeRegistry::new(&path);

        assert_eq!(reg.get_or_create("runs"), 0);
        assert_eq!(reg.get_or_create("depends_on"), 1);
        assert_eq!(reg.get_or_create("runs"), 0); // idempotent
    }

    #[test]
    fn name_lookup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut reg = TypeRegistry::new(&path);

        reg.get_or_create("connects_to");
        assert_eq!(reg.name(0), Some("connects_to"));
        assert_eq!(reg.name(99), None);
        assert_eq!(reg.name_or_default(99), "type_99");
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        {
            let mut reg = TypeRegistry::new(&path);
            reg.get_or_create("runs");
            reg.get_or_create("depends_on");
            reg.get_or_create("monitors");
            reg.flush().unwrap();
        }

        {
            let reg = TypeRegistry::load(&path).unwrap();
            assert_eq!(reg.name(0), Some("runs"));
            assert_eq!(reg.name(1), Some("depends_on"));
            assert_eq!(reg.name(2), Some("monitors"));
        }
    }
}
