/// Co-occurrence tracking — passive frequency counters.
///
/// Tracks how often two events/facts appear together within a time window.
/// This is purely statistical — engram does NOT infer causation from co-occurrence.
/// The counters are surfaced as evidence when queried.
///
/// Example: "migration was followed by missing-index 3 out of 3 times within 24h"

use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// A pair of labels that co-occurred.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CooccurrencePair {
    /// The antecedent (what happened first / trigger).
    pub antecedent: String,
    /// The consequent (what followed).
    pub consequent: String,
}

/// Statistics for a co-occurrence pair.
#[derive(Debug, Clone)]
pub struct CooccurrenceStats {
    /// Number of times this pair co-occurred.
    pub count: u32,
    /// Number of times the antecedent occurred total (for conditional probability).
    pub antecedent_total: u32,
    /// Timestamps of each co-occurrence (unix nanos), most recent last.
    pub timestamps: Vec<i64>,
}

impl CooccurrenceStats {
    fn new() -> Self {
        CooccurrenceStats {
            count: 0,
            antecedent_total: 0,
            timestamps: Vec::new(),
        }
    }

    /// Conditional probability: P(consequent | antecedent).
    pub fn probability(&self) -> f32 {
        if self.antecedent_total == 0 {
            0.0
        } else {
            self.count as f32 / self.antecedent_total as f32
        }
    }
}

/// Co-occurrence tracker — in-memory with sidecar persistence.
pub struct CooccurrenceTracker {
    path: PathBuf,
    pairs: HashMap<CooccurrencePair, CooccurrenceStats>,
}

impl CooccurrenceTracker {
    pub fn new(brain_path: &Path) -> Self {
        CooccurrenceTracker {
            path: brain_path.with_extension("brain.cooccur"),
            pairs: HashMap::new(),
        }
    }

    /// Record a co-occurrence between two labels.
    pub fn record(&mut self, antecedent: &str, consequent: &str, now: i64) {
        let pair = CooccurrencePair {
            antecedent: antecedent.to_string(),
            consequent: consequent.to_string(),
        };
        let stats = self.pairs.entry(pair).or_insert_with(CooccurrenceStats::new);
        stats.count += 1;
        stats.timestamps.push(now);
    }

    /// Increment the antecedent's total occurrence count.
    /// Call this each time the antecedent occurs, whether or not a consequent follows.
    pub fn record_antecedent(&mut self, antecedent: &str) {
        // Update all pairs that have this antecedent
        for (pair, stats) in &mut self.pairs {
            if pair.antecedent == antecedent {
                stats.antecedent_total += 1;
            }
        }
    }

    /// Get stats for a specific pair.
    pub fn get(&self, antecedent: &str, consequent: &str) -> Option<&CooccurrenceStats> {
        let pair = CooccurrencePair {
            antecedent: antecedent.to_string(),
            consequent: consequent.to_string(),
        };
        self.pairs.get(&pair)
    }

    /// Get all co-occurrences for a given antecedent.
    pub fn for_antecedent(&self, antecedent: &str) -> Vec<(&str, &CooccurrenceStats)> {
        self.pairs
            .iter()
            .filter(|(pair, _)| pair.antecedent == antecedent)
            .map(|(pair, stats)| (pair.consequent.as_str(), stats))
            .collect()
    }

    /// Get all tracked pairs.
    pub fn all_pairs(&self) -> &HashMap<CooccurrencePair, CooccurrenceStats> {
        &self.pairs
    }

    /// Persist to sidecar file.
    /// Binary format: pair_count(u32) + [
    ///   antecedent_len(u16) + antecedent +
    ///   consequent_len(u16) + consequent +
    ///   count(u32) + antecedent_total(u32) +
    ///   ts_count(u32) + [timestamp(i64)]*
    /// ]*
    pub fn flush(&self) -> std::io::Result<()> {
        let file = std::fs::File::create(&self.path)?;
        let mut w = BufWriter::new(file);

        w.write_all(&(self.pairs.len() as u32).to_le_bytes())?;

        for (pair, stats) in &self.pairs {
            let ant = pair.antecedent.as_bytes();
            let con = pair.consequent.as_bytes();
            w.write_all(&(ant.len() as u16).to_le_bytes())?;
            w.write_all(ant)?;
            w.write_all(&(con.len() as u16).to_le_bytes())?;
            w.write_all(con)?;
            w.write_all(&stats.count.to_le_bytes())?;
            w.write_all(&stats.antecedent_total.to_le_bytes())?;
            w.write_all(&(stats.timestamps.len() as u32).to_le_bytes())?;
            for &ts in &stats.timestamps {
                w.write_all(&ts.to_le_bytes())?;
            }
        }

        w.flush()
    }

    /// Load from sidecar file.
    pub fn load(brain_path: &Path) -> std::io::Result<Self> {
        let path = brain_path.with_extension("brain.cooccur");
        if !path.exists() {
            return Ok(CooccurrenceTracker {
                path,
                pairs: HashMap::new(),
            });
        }

        let file = std::fs::File::open(&path)?;
        let mut r = BufReader::new(file);
        let mut pairs = HashMap::new();

        let mut buf4 = [0u8; 4];
        r.read_exact(&mut buf4)?;
        let pair_count = u32::from_le_bytes(buf4) as usize;

        for _ in 0..pair_count {
            let mut buf2 = [0u8; 2];
            r.read_exact(&mut buf2)?;
            let ant_len = u16::from_le_bytes(buf2) as usize;
            let mut ant_buf = vec![0u8; ant_len];
            r.read_exact(&mut ant_buf)?;

            r.read_exact(&mut buf2)?;
            let con_len = u16::from_le_bytes(buf2) as usize;
            let mut con_buf = vec![0u8; con_len];
            r.read_exact(&mut con_buf)?;

            r.read_exact(&mut buf4)?;
            let count = u32::from_le_bytes(buf4);
            r.read_exact(&mut buf4)?;
            let antecedent_total = u32::from_le_bytes(buf4);
            r.read_exact(&mut buf4)?;
            let ts_count = u32::from_le_bytes(buf4) as usize;

            let mut timestamps = Vec::with_capacity(ts_count);
            let mut buf8 = [0u8; 8];
            for _ in 0..ts_count {
                r.read_exact(&mut buf8)?;
                timestamps.push(i64::from_le_bytes(buf8));
            }

            let pair = CooccurrencePair {
                antecedent: String::from_utf8_lossy(&ant_buf).into_owned(),
                consequent: String::from_utf8_lossy(&con_buf).into_owned(),
            };
            pairs.insert(pair, CooccurrenceStats {
                count,
                antecedent_total,
                timestamps,
            });
        }

        Ok(CooccurrenceTracker { path, pairs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn record_and_query() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut tracker = CooccurrenceTracker::new(&path);

        tracker.record("migration", "missing-index", 1000);
        tracker.record("migration", "missing-index", 2000);
        tracker.record("migration", "missing-index", 3000);

        let stats = tracker.get("migration", "missing-index").unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.timestamps.len(), 3);
    }

    #[test]
    fn conditional_probability() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut tracker = CooccurrenceTracker::new(&path);

        // migration happened 5 times, missing-index followed 3 times
        tracker.record("migration", "missing-index", 1000);
        tracker.record("migration", "missing-index", 2000);
        tracker.record("migration", "missing-index", 3000);

        // Manually set antecedent_total
        let pair = CooccurrencePair {
            antecedent: "migration".into(),
            consequent: "missing-index".into(),
        };
        tracker.pairs.get_mut(&pair).unwrap().antecedent_total = 5;

        let stats = tracker.get("migration", "missing-index").unwrap();
        assert!((stats.probability() - 0.60).abs() < f32::EPSILON);
    }

    #[test]
    fn for_antecedent_returns_all() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut tracker = CooccurrenceTracker::new(&path);

        tracker.record("deploy", "latency-spike", 1000);
        tracker.record("deploy", "error-rate-up", 2000);
        tracker.record("reboot", "downtime", 3000);

        let deploy_pairs = tracker.for_antecedent("deploy");
        assert_eq!(deploy_pairs.len(), 2);

        let reboot_pairs = tracker.for_antecedent("reboot");
        assert_eq!(reboot_pairs.len(), 1);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        {
            let mut tracker = CooccurrenceTracker::new(&path);
            tracker.record("migration", "missing-index", 1000);
            tracker.record("migration", "missing-index", 2000);
            tracker.record("deploy", "error-rate", 3000);
            tracker.flush().unwrap();
        }

        {
            let tracker = CooccurrenceTracker::load(&path).unwrap();
            let stats = tracker.get("migration", "missing-index").unwrap();
            assert_eq!(stats.count, 2);
            assert_eq!(stats.timestamps, vec![1000, 2000]);

            let stats2 = tracker.get("deploy", "error-rate").unwrap();
            assert_eq!(stats2.count, 1);
        }
    }

    #[test]
    fn empty_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.brain");
        let tracker = CooccurrenceTracker::load(&path).unwrap();
        assert!(tracker.all_pairs().is_empty());
    }
}
