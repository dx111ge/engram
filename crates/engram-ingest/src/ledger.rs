/// Search ledger: tracks queries executed, temporal cursors, content hash dedup.
///
/// Persisted as `.brain.ledger` sidecar file. Each entry records a query
/// that was run, its result hash, and temporal cursor for incremental fetching.
///
/// Format (tab-separated, one entry per line):
/// ```text
/// source\tquery\tcontent_hash\ttimestamp\tcursor\titem_count
/// ```

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// A single ledger entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LedgerEntry {
    /// Source name.
    pub source: String,
    /// Query string that was executed.
    pub query: String,
    /// Hash of the result content (for dedup).
    pub content_hash: u64,
    /// Timestamp when this query was executed (unix seconds).
    pub timestamp: i64,
    /// Temporal cursor for the next incremental fetch.
    pub cursor: Option<i64>,
    /// Number of items returned.
    pub item_count: u32,
}

/// Search ledger with persistence.
pub struct SearchLedger {
    /// Path to the `.brain.ledger` file.
    path: PathBuf,
    /// In-memory entries, keyed by (source, query).
    entries: Vec<LedgerEntry>,
    /// Content hash set for dedup: (source, hash) -> entry index.
    hash_index: HashMap<(String, u64), usize>,
    /// Cursor index: (source, query) -> latest cursor.
    cursor_index: HashMap<(String, String), i64>,
}

impl SearchLedger {
    /// Create an empty ledger with no path (placeholder, must call open later).
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            entries: Vec::new(),
            hash_index: HashMap::new(),
            cursor_index: HashMap::new(),
        }
    }

    /// Create a new ledger, loading from disk if the file exists.
    pub fn open(brain_path: &Path) -> Self {
        let path = brain_path.with_extension("ledger");
        let mut ledger = Self {
            path,
            entries: Vec::new(),
            hash_index: HashMap::new(),
            cursor_index: HashMap::new(),
        };
        ledger.load();
        ledger
    }

    /// Record a query execution.
    ///
    /// Returns `true` if this is new content, `false` if the content hash
    /// already exists (duplicate).
    pub fn record(
        &mut self,
        source: &str,
        query: &str,
        content_hash: u64,
        cursor: Option<i64>,
        item_count: u32,
    ) -> bool {
        let key = (source.to_string(), content_hash);

        // Check for duplicate content
        if self.hash_index.contains_key(&key) {
            return false;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let entry = LedgerEntry {
            source: source.to_string(),
            query: query.to_string(),
            content_hash,
            timestamp: now,
            cursor,
            item_count,
        };

        let idx = self.entries.len();
        self.hash_index.insert(key, idx);

        if let Some(c) = cursor {
            self.cursor_index.insert(
                (source.to_string(), query.to_string()),
                c,
            );
        }

        self.entries.push(entry);
        true
    }

    /// Get the latest temporal cursor for a (source, query) pair.
    pub fn get_cursor(&self, source: &str, query: &str) -> Option<i64> {
        self.cursor_index
            .get(&(source.to_string(), query.to_string()))
            .copied()
    }

    /// Check if content with the given hash has already been ingested.
    pub fn has_content(&self, source: &str, content_hash: u64) -> bool {
        self.hash_index
            .contains_key(&(source.to_string(), content_hash))
    }

    /// Get all entries for a source.
    pub fn entries_for_source(&self, source: &str) -> Vec<&LedgerEntry> {
        self.entries
            .iter()
            .filter(|e| e.source == source)
            .collect()
    }

    /// Get all entries.
    pub fn all_entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ledger is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Compute a content hash for dedup.
    pub fn content_hash(data: &[u8]) -> u64 {
        // FNV-1a hash for speed
        let mut hash: u64 = 0xcbf29ce484222325;
        for &byte in data {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Save ledger to disk.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let mut file = std::fs::File::create(&self.path)?;
        for entry in &self.entries {
            writeln!(
                file,
                "{}\t{}\t{}\t{}\t{}\t{}",
                entry.source,
                entry.query,
                entry.content_hash,
                entry.timestamp,
                entry.cursor.map(|c| c.to_string()).unwrap_or_default(),
                entry.item_count,
            )?;
        }
        Ok(())
    }

    /// Load ledger from disk.
    fn load(&mut self) {
        let file = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return, // no ledger file yet
        };

        let reader = std::io::BufReader::new(file);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 6 {
                continue;
            }

            let content_hash: u64 = match parts[2].parse() {
                Ok(h) => h,
                Err(_) => continue,
            };
            let timestamp: i64 = parts[3].parse().unwrap_or(0);
            let cursor: Option<i64> = if parts[4].is_empty() {
                None
            } else {
                parts[4].parse().ok()
            };
            let item_count: u32 = parts[5].parse().unwrap_or(0);

            let entry = LedgerEntry {
                source: parts[0].to_string(),
                query: parts[1].to_string(),
                content_hash,
                timestamp,
                cursor,
                item_count,
            };

            let idx = self.entries.len();
            self.hash_index.insert(
                (entry.source.clone(), entry.content_hash),
                idx,
            );
            if let Some(c) = entry.cursor {
                let existing = self.cursor_index
                    .entry((entry.source.clone(), entry.query.clone()))
                    .or_insert(c);
                if c > *existing {
                    *existing = c;
                }
            }

            self.entries.push(entry);
        }
    }

    /// Prune entries older than the given timestamp.
    pub fn prune_before(&mut self, before: i64) {
        self.entries.retain(|e| e.timestamp >= before);
        self.rebuild_indices();
    }

    fn rebuild_indices(&mut self) {
        self.hash_index.clear();
        self.cursor_index.clear();
        for (idx, entry) in self.entries.iter().enumerate() {
            self.hash_index.insert(
                (entry.source.clone(), entry.content_hash),
                idx,
            );
            if let Some(c) = entry.cursor {
                let existing = self.cursor_index
                    .entry((entry.source.clone(), entry.query.clone()))
                    .or_insert(c);
                if c > *existing {
                    *existing = c;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_deterministic() {
        let h1 = SearchLedger::content_hash(b"hello world");
        let h2 = SearchLedger::content_hash(b"hello world");
        let h3 = SearchLedger::content_hash(b"different");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn record_and_dedup() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");
        let mut ledger = SearchLedger::open(&brain);

        let hash = SearchLedger::content_hash(b"some content");

        // First record should succeed
        assert!(ledger.record("reuters", "Apple Inc", hash, Some(1000), 5));
        assert_eq!(ledger.len(), 1);

        // Duplicate hash should be rejected
        assert!(!ledger.record("reuters", "Apple Inc", hash, Some(1001), 3));
        assert_eq!(ledger.len(), 1);

        // Different hash should succeed
        let hash2 = SearchLedger::content_hash(b"different content");
        assert!(ledger.record("reuters", "Apple Inc", hash2, Some(1002), 2));
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn cursor_tracking() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");
        let mut ledger = SearchLedger::open(&brain);

        assert!(ledger.get_cursor("src", "q1").is_none());

        ledger.record("src", "q1", 1, Some(100), 5);
        assert_eq!(ledger.get_cursor("src", "q1"), Some(100));

        ledger.record("src", "q1", 2, Some(200), 3);
        assert_eq!(ledger.get_cursor("src", "q1"), Some(200));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");

        {
            let mut ledger = SearchLedger::open(&brain);
            ledger.record("src1", "query1", 111, Some(1000), 5);
            ledger.record("src2", "query2", 222, None, 3);
            ledger.save().unwrap();
        }

        // Reload
        let ledger = SearchLedger::open(&brain);
        assert_eq!(ledger.len(), 2);
        assert!(ledger.has_content("src1", 111));
        assert!(ledger.has_content("src2", 222));
        assert!(!ledger.has_content("src1", 999));
        assert_eq!(ledger.get_cursor("src1", "query1"), Some(1000));
        assert_eq!(ledger.get_cursor("src2", "query2"), None);
    }

    #[test]
    fn entries_for_source() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");
        let mut ledger = SearchLedger::open(&brain);

        ledger.record("reuters", "q1", 1, None, 5);
        ledger.record("reuters", "q2", 2, None, 3);
        ledger.record("bbc", "q1", 3, None, 2);

        assert_eq!(ledger.entries_for_source("reuters").len(), 2);
        assert_eq!(ledger.entries_for_source("bbc").len(), 1);
        assert_eq!(ledger.entries_for_source("unknown").len(), 0);
    }

    #[test]
    fn prune_removes_old_entries() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");
        let mut ledger = SearchLedger::open(&brain);

        ledger.record("src", "q1", 1, None, 1);
        ledger.record("src", "q2", 2, None, 1);

        // All entries have recent timestamps, so pruning with a future cutoff removes all
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64 + 1000;
        ledger.prune_before(future);
        assert!(ledger.is_empty());
    }
}
