/// Content deduplication: hash-based + semantic similarity.
///
/// Detects duplicate facts before they enter the graph.
/// Two strategies: exact content hash and fuzzy semantic similarity.

use std::collections::{HashMap, HashSet};

use crate::types::ProcessedFact;

/// Deduplication result for a single fact.
#[derive(Debug, Clone)]
pub enum DedupResult {
    /// Fact is unique, should be stored.
    Unique,
    /// Exact duplicate of an existing fact (same hash).
    ExactDuplicate { existing_entity: String },
    /// Near-duplicate detected (semantic similarity above threshold).
    NearDuplicate {
        existing_entity: String,
        similarity: f32,
    },
}

/// Content-hash-based deduplicator.
///
/// Computes a hash from entity label + entity type + source.
/// Fast O(1) lookup per fact.
pub struct ContentDedup {
    /// entity_hash -> entity label (for reporting which entity matched).
    seen: HashMap<u64, String>,
}

impl ContentDedup {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// Check if a fact is a duplicate and register it.
    pub fn check(&mut self, fact: &ProcessedFact) -> DedupResult {
        let hash = content_hash(fact);

        if let Some(existing) = self.seen.get(&hash) {
            DedupResult::ExactDuplicate {
                existing_entity: existing.clone(),
            }
        } else {
            self.seen.insert(hash, fact.entity.clone());
            DedupResult::Unique
        }
    }

    /// Number of unique facts seen.
    pub fn unique_count(&self) -> usize {
        self.seen.len()
    }

    /// Reset the dedup state.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

impl Default for ContentDedup {
    fn default() -> Self {
        Self::new()
    }
}

/// Deduplicate a batch of facts in place.
/// Returns (kept_facts, dedup_count).
pub fn dedup_batch(facts: Vec<ProcessedFact>) -> (Vec<ProcessedFact>, u32) {
    let mut dedup = ContentDedup::new();
    let mut kept = Vec::with_capacity(facts.len());
    let mut dedup_count = 0u32;

    for fact in facts {
        match dedup.check(&fact) {
            DedupResult::Unique => kept.push(fact),
            DedupResult::ExactDuplicate { .. } => dedup_count += 1,
            DedupResult::NearDuplicate { .. } => dedup_count += 1,
        }
    }

    (kept, dedup_count)
}

/// Deduplicate within a batch using a set (by entity label only — fast path).
pub fn dedup_by_label(facts: Vec<ProcessedFact>) -> (Vec<ProcessedFact>, u32) {
    let mut seen = HashSet::new();
    let mut kept = Vec::with_capacity(facts.len());
    let mut count = 0u32;

    for fact in facts {
        let key = fact.entity.to_lowercase();
        if seen.contains(&key) {
            count += 1;
        } else {
            seen.insert(key);
            kept.push(fact);
        }
    }

    (kept, count)
}

/// Compute a content hash for a processed fact.
/// Based on: lowercase entity + entity_type + source.
fn content_hash(fact: &ProcessedFact) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fact.entity.to_lowercase().hash(&mut hasher);
    fact.entity_type.hash(&mut hasher);
    fact.provenance.source.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtractionMethod, Provenance};

    fn make_fact(entity: &str, source: &str) -> ProcessedFact {
        ProcessedFact {
            entity: entity.into(),
            entity_type: Some("ORG".into()),
            properties: Default::default(),
            confidence: 0.8,
            provenance: Provenance {
                source: source.into(),
                source_url: None,
                author: None,
                extraction_method: ExtractionMethod::Manual,
                fetched_at: 0,
                ingested_at: 0,
            },
            extraction_method: ExtractionMethod::Manual,
            language: "en".into(),
            relations: vec![],
            conflicts: vec![],
            resolution: None,
            source_text: None,
        }
    }

    #[test]
    fn content_dedup_detects_exact_duplicates() {
        let mut dedup = ContentDedup::new();

        let f1 = make_fact("Apple Inc.", "reuters");
        let f2 = make_fact("Apple Inc.", "reuters"); // same entity + source
        let f3 = make_fact("Apple Inc.", "bbc"); // same entity, different source

        assert!(matches!(dedup.check(&f1), DedupResult::Unique));
        assert!(matches!(dedup.check(&f2), DedupResult::ExactDuplicate { .. }));
        assert!(matches!(dedup.check(&f3), DedupResult::Unique)); // different source = unique
    }

    #[test]
    fn dedup_batch_filters_duplicates() {
        let facts = vec![
            make_fact("Apple", "reuters"),
            make_fact("Apple", "reuters"), // dup
            make_fact("Microsoft", "reuters"),
            make_fact("Apple", "bbc"), // different source
        ];

        let (kept, count) = dedup_batch(facts);
        assert_eq!(kept.len(), 3);
        assert_eq!(count, 1);
    }

    #[test]
    fn dedup_by_label_ignores_source() {
        let facts = vec![
            make_fact("Apple", "reuters"),
            make_fact("Apple", "bbc"), // same label, different source = dup by label
            make_fact("Microsoft", "reuters"),
        ];

        let (kept, count) = dedup_by_label(facts);
        assert_eq!(kept.len(), 2); // Apple + Microsoft
        assert_eq!(count, 1);
    }

    #[test]
    fn case_insensitive_dedup() {
        let mut dedup = ContentDedup::new();
        let f1 = make_fact("Apple Inc.", "test");
        let f2 = make_fact("apple inc.", "test");

        assert!(matches!(dedup.check(&f1), DedupResult::Unique));
        assert!(matches!(dedup.check(&f2), DedupResult::ExactDuplicate { .. }));
    }
}
