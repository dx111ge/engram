/// Assessment sidecar store — manages `.brain.assessments` JSON file.
///
/// Append-only score history, CRUD operations, and checkpoint support.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::AssessmentRecord;

/// Persistent assessment store backed by a JSON sidecar file.
pub struct AssessmentStore {
    records: HashMap<String, AssessmentRecord>,
    path: PathBuf,
    dirty: AtomicBool,
}

impl AssessmentStore {
    /// Create a new empty store at the given path.
    pub fn new(path: PathBuf) -> Self {
        Self {
            records: HashMap::new(),
            path,
            dirty: AtomicBool::new(false),
        }
    }

    /// Load from a JSON sidecar file. Returns empty store if file doesn't exist.
    pub fn load(path: PathBuf) -> Self {
        let records = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    serde_json::from_str::<Vec<AssessmentRecord>>(&contents)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|r| (r.label.clone(), r))
                        .collect()
                }
                Err(e) => {
                    tracing::warn!("failed to load assessments from {}: {}", path.display(), e);
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        Self {
            records,
            path,
            dirty: AtomicBool::new(false),
        }
    }

    /// Get an assessment record by label.
    pub fn get(&self, label: &str) -> Option<&AssessmentRecord> {
        self.records.get(label)
    }

    /// Get a mutable assessment record by label.
    pub fn get_mut(&mut self, label: &str) -> Option<&mut AssessmentRecord> {
        let r = self.records.get_mut(label)?;
        self.dirty.store(true, Ordering::Release);
        Some(r)
    }

    /// Insert or replace an assessment record.
    pub fn insert(&mut self, record: AssessmentRecord) {
        self.records.insert(record.label.clone(), record);
        self.dirty.store(true, Ordering::Release);
    }

    /// Remove an assessment record. Returns the removed record if it existed.
    pub fn remove(&mut self, label: &str) -> Option<AssessmentRecord> {
        let removed = self.records.remove(label);
        if removed.is_some() {
            self.dirty.store(true, Ordering::Release);
        }
        removed
    }

    /// List all assessment labels.
    pub fn labels(&self) -> Vec<&str> {
        self.records.keys().map(|s| s.as_str()).collect()
    }

    /// List all assessment records.
    pub fn all(&self) -> Vec<&AssessmentRecord> {
        self.records.values().collect()
    }

    /// Number of assessments.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Save to disk (unconditionally).
    pub fn save(&self) -> std::io::Result<()> {
        let records: Vec<&AssessmentRecord> = self.records.values().collect();
        let json = serde_json::to_string_pretty(&records)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.path, json)?;
        self.dirty.store(false, Ordering::Release);
        Ok(())
    }

    /// Save only if dirty. Returns true if flushed.
    pub fn checkpoint_if_dirty(&self) -> bool {
        if self.dirty.swap(false, Ordering::AcqRel) {
            if let Err(e) = self.save() {
                tracing::warn!("assessment checkpoint failed: {}", e);
                self.dirty.store(true, Ordering::Release);
                return false;
            }
            return true;
        }
        false
    }

    /// Mark as dirty (e.g., after external modification).
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ScorePoint, ScoreTrigger};

    fn make_record(label: &str) -> AssessmentRecord {
        AssessmentRecord {
            label: label.to_string(),
            node_id: 1,
            history: vec![ScorePoint {
                timestamp: 1000,
                probability: 0.50,
                shift: 0.0,
                trigger: ScoreTrigger::Created,
                reason: "Initial assessment".to_string(),
                path: None,
            }],
            evidence_for: vec![],
            evidence_against: vec![],
        }
    }

    #[test]
    fn crud_operations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.assessments");
        let mut store = AssessmentStore::new(path.clone());

        // Insert
        store.insert(make_record("Assessment:test-1"));
        assert_eq!(store.len(), 1);
        assert!(store.get("Assessment:test-1").is_some());

        // Update via get_mut
        store.get_mut("Assessment:test-1").unwrap().node_id = 42;
        assert_eq!(store.get("Assessment:test-1").unwrap().node_id, 42);

        // Remove
        let removed = store.remove("Assessment:test-1");
        assert!(removed.is_some());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.assessments");

        // Save
        let mut store = AssessmentStore::new(path.clone());
        store.insert(make_record("Assessment:save-test"));
        store.save().unwrap();

        // Load
        let loaded = AssessmentStore::load(path);
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get("Assessment:save-test").is_some());
    }

    #[test]
    fn checkpoint_if_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.assessments");
        let mut store = AssessmentStore::new(path);

        // Not dirty -> no save
        assert!(!store.checkpoint_if_dirty());

        // Insert makes dirty
        store.insert(make_record("Assessment:dirty-test"));
        assert!(store.checkpoint_if_dirty());
        assert!(!store.checkpoint_if_dirty()); // cleared after save
    }
}
