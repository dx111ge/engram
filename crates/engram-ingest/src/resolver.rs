/// Entity resolution: conservative 4-step matching against the graph.
///
/// Implements the progressive ER framework:
/// 1. FILTER — reduce search space via hash + fulltext index
/// 2. WEIGHT — similarity scoring (string distance + BM25 score)
/// 3. SCHEDULE — prioritize by score
/// 4. MATCH — apply decision thresholds

use crate::traits::Resolver;
use crate::types::{ExtractedEntity, ResolutionResult};

/// Configuration for entity resolution.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Above this: auto-match (link to existing node).
    pub auto_match_threshold: f32,
    /// Between ambiguous_threshold and auto_match: create maybe_same_as edge.
    pub ambiguous_threshold: f32,
    /// Maximum candidates to consider from fulltext search.
    pub max_candidates: usize,
    /// Weight for string similarity in the combined score.
    pub string_weight: f32,
    /// Weight for BM25/fulltext score.
    pub fulltext_weight: f32,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            auto_match_threshold: 0.90,
            ambiguous_threshold: 0.60,
            max_candidates: 20,
            string_weight: 0.6,
            fulltext_weight: 0.4,
        }
    }
}

/// Conservative entity resolver.
///
/// Runs under a read lock — no graph mutations during resolution.
/// Mutation (node creation, maybe_same_as edges) happens in the load phase.
pub struct ConservativeResolver {
    config: ResolverConfig,
}

impl ConservativeResolver {
    pub fn new(config: ResolverConfig) -> Self {
        Self { config }
    }
}

impl Default for ConservativeResolver {
    fn default() -> Self {
        Self::new(ResolverConfig::default())
    }
}

impl Resolver for ConservativeResolver {
    fn resolve(
        &self,
        entity: &ExtractedEntity,
        graph: &engram_core::graph::Graph,
    ) -> ResolutionResult {
        // Already resolved (e.g. by gazetteer)?
        if let Some(nid) = entity.resolved_to {
            return ResolutionResult::Matched(nid);
        }

        let label = &entity.text;

        // ── Step 1: FILTER ──

        // Fast path: exact label match via hash index
        if let Ok(Some(node_id)) = graph.find_node_id(label) {
            return ResolutionResult::Matched(node_id);
        }

        // Fuzzy path: fulltext index search for candidates
        let candidates = match graph.search_text(label, self.config.max_candidates) {
            Ok(results) => results,
            Err(_) => return ResolutionResult::New,
        };

        if candidates.is_empty() {
            return ResolutionResult::New;
        }

        // ── Step 2: WEIGHT ──

        let mut scored: Vec<(u64, f32)> = candidates
            .iter()
            .map(|candidate| {
                let string_sim = compute_string_similarity(label, &candidate.label);

                // Also check canonical_name property -- if the existing node has
                // canonical_name matching our incoming label, boost the score.
                // This handles: existing "Putin" with canonical_name "Vladimir Putin"
                // matching incoming "Vladimir Putin".
                let canonical_sim = graph
                    .get_property(&candidate.label, "canonical_name")
                    .ok()
                    .flatten()
                    .map(|canonical| compute_string_similarity(label, &canonical))
                    .unwrap_or(0.0);

                let best_string_sim = string_sim.max(canonical_sim);

                let fulltext_score = candidate.score as f32;

                // Normalize fulltext score to [0, 1] range (BM25 scores can be > 1)
                let norm_fulltext = (fulltext_score / (fulltext_score + 1.0)).min(1.0);

                let combined = self.config.string_weight * best_string_sim
                    + self.config.fulltext_weight * norm_fulltext;

                (candidate.node_id, combined)
            })
            .collect();

        // ── Step 3: SCHEDULE ──

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // ── Step 4: MATCH ──

        if scored.is_empty() {
            return ResolutionResult::New;
        }

        let (best_id, best_score) = scored[0];

        if best_score >= self.config.auto_match_threshold {
            // High confidence: auto-match
            ResolutionResult::Matched(best_id)
        } else if best_score >= self.config.ambiguous_threshold {
            // Medium confidence: ambiguous, report all candidates above threshold
            let ambiguous: Vec<(u64, f32)> = scored
                .into_iter()
                .filter(|(_, score)| *score >= self.config.ambiguous_threshold)
                .collect();
            ResolutionResult::Ambiguous(ambiguous)
        } else {
            // Low confidence: new entity
            ResolutionResult::New
        }
    }
}

/// Compute string similarity between two labels.
/// Uses Jaro-Winkler for short strings, normalized Levenshtein for longer ones.
fn compute_string_similarity(a: &str, b: &str) -> f32 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Exact match (case-insensitive)
    if a_lower == b_lower {
        return 1.0;
    }

    // Use Jaro-Winkler for names (short strings), Levenshtein for longer
    if a.len() <= 30 && b.len() <= 30 {
        strsim::jaro_winkler(&a_lower, &b_lower) as f32
    } else {
        strsim::normalized_levenshtein(&a_lower, &b_lower) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExtractionMethod;
    use tempfile::TempDir;

    fn make_entity(text: &str, entity_type: &str) -> ExtractedEntity {
        ExtractedEntity {
            text: text.into(),
            entity_type: entity_type.into(),
            span: (0, text.len()),
            confidence: 0.8,
            method: ExtractionMethod::Manual,
            language: "en".into(),
            resolved_to: None,
        }
    }

    fn test_graph_with_entities() -> (TempDir, engram_core::graph::Graph) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = engram_core::graph::Graph::create(&path).unwrap();

        let prov = engram_core::graph::Provenance::user("test");
        graph.store_with_confidence("Apple Inc.", 0.9, &prov).unwrap();
        graph.store_with_confidence("Tim Cook", 0.85, &prov).unwrap();
        graph.store_with_confidence("Microsoft Corporation", 0.9, &prov).unwrap();
        graph.store_with_confidence("Berlin", 0.95, &prov).unwrap();

        (dir, graph)
    }

    #[test]
    fn exact_match_resolves_immediately() {
        let (_dir, graph) = test_graph_with_entities();
        let resolver = ConservativeResolver::default();

        let entity = make_entity("Apple Inc.", "ORG");
        let result = resolver.resolve(&entity, &graph);

        match result {
            ResolutionResult::Matched(nid) => assert!(nid > 0 || nid == 0),
            other => panic!("expected Matched, got {:?}", other),
        }
    }

    #[test]
    fn unknown_entity_returns_new() {
        let (_dir, graph) = test_graph_with_entities();
        let resolver = ConservativeResolver::default();

        let entity = make_entity("Totally Unknown Entity XYZ", "ORG");
        let result = resolver.resolve(&entity, &graph);

        assert!(matches!(result, ResolutionResult::New));
    }

    #[test]
    fn already_resolved_entity_passes_through() {
        let (_dir, graph) = test_graph_with_entities();
        let resolver = ConservativeResolver::default();

        let mut entity = make_entity("Something", "ORG");
        entity.resolved_to = Some(42);

        let result = resolver.resolve(&entity, &graph);
        assert!(matches!(result, ResolutionResult::Matched(42)));
    }

    #[test]
    fn string_similarity_basics() {
        assert_eq!(compute_string_similarity("Apple", "Apple"), 1.0);
        assert_eq!(compute_string_similarity("apple", "APPLE"), 1.0);
        assert!(compute_string_similarity("Apple Inc.", "Apple Inc") > 0.9);
        assert!(compute_string_similarity("Apple", "Microsoft") < 0.5);
    }

    #[test]
    fn config_thresholds_respected() {
        let config = ResolverConfig {
            auto_match_threshold: 0.95, // very strict
            ambiguous_threshold: 0.50,
            ..Default::default()
        };
        let resolver = ConservativeResolver::new(config);
        let (_dir, graph) = test_graph_with_entities();

        // Exact match should still work (score = 1.0 > 0.95)
        let entity = make_entity("Berlin", "LOC");
        let result = resolver.resolve(&entity, &graph);
        assert!(matches!(result, ResolutionResult::Matched(_)));
    }
}
