/// Hybrid search — combines BM25 keyword search and vector similarity search.
///
/// Uses Reciprocal Rank Fusion (RRF) to merge results from both sources
/// into a single ranked list. RRF is robust and doesn't need score normalization.

use std::collections::HashMap;

/// A result from any search source
#[derive(Debug, Clone)]
pub struct HybridHit {
    pub slot: u64,
    pub score: f64,
    /// Which sources contributed to this result
    pub sources: HitSources,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HitSources {
    pub keyword: bool,
    pub vector: bool,
}

/// Reciprocal Rank Fusion constant (standard value from the literature)
const RRF_K: f64 = 60.0;

/// Merge results from keyword search and vector search using RRF.
///
/// Each result list should be pre-sorted by relevance (best first).
/// Returns merged results sorted by combined RRF score (highest first).
pub fn reciprocal_rank_fusion(
    keyword_results: &[(u64, f64)],   // (slot, bm25_score) — sorted best first
    vector_results: &[(u64, f32)],    // (slot, distance) — sorted closest first
    limit: usize,
) -> Vec<HybridHit> {
    let mut scores: HashMap<u64, (f64, HitSources)> = HashMap::new();

    // RRF for keyword results
    for (rank, &(slot, _bm25_score)) in keyword_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(slot).or_insert((0.0, HitSources::default()));
        entry.0 += rrf_score;
        entry.1.keyword = true;
    }

    // RRF for vector results
    for (rank, &(slot, _distance)) in vector_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(slot).or_insert((0.0, HitSources::default()));
        entry.0 += rrf_score;
        entry.1.vector = true;
    }

    let mut results: Vec<HybridHit> = scores
        .into_iter()
        .map(|(slot, (score, sources))| HybridHit { slot, score, sources })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_keyword_only() {
        let keywords = vec![(0, 5.0), (1, 3.0), (2, 1.0)];
        let vectors: Vec<(u64, f32)> = vec![];

        let results = reciprocal_rank_fusion(&keywords, &vectors, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].slot, 0); // highest rank
        assert!(results[0].sources.keyword);
        assert!(!results[0].sources.vector);
    }

    #[test]
    fn rrf_vector_only() {
        let keywords: Vec<(u64, f64)> = vec![];
        let vectors = vec![(5, 0.1), (3, 0.3), (7, 0.5)];

        let results = reciprocal_rank_fusion(&keywords, &vectors, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].slot, 5); // closest vector
        assert!(!results[0].sources.keyword);
        assert!(results[0].sources.vector);
    }

    #[test]
    fn rrf_combined_boosts_overlap() {
        // Slot 1 appears in both keyword and vector results
        let keywords = vec![(1, 5.0), (2, 3.0)];
        let vectors = vec![(1, 0.1), (3, 0.3)];

        let results = reciprocal_rank_fusion(&keywords, &vectors, 10);
        // Slot 1 should be top (appears in both)
        assert_eq!(results[0].slot, 1);
        assert!(results[0].sources.keyword);
        assert!(results[0].sources.vector);
        // Score should be higher than single-source results
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn rrf_limit() {
        let keywords: Vec<(u64, f64)> = (0..20).map(|i| (i, 1.0)).collect();
        let vectors: Vec<(u64, f32)> = vec![];

        let results = reciprocal_rank_fusion(&keywords, &vectors, 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn rrf_empty_inputs() {
        let results = reciprocal_rank_fusion(&[], &[], 10);
        assert!(results.is_empty());
    }
}
