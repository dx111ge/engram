/// Relation chain: orchestrates multiple relation extraction backends.
///
/// Uses MergeAll strategy (not cascade): layers are complementary.
/// - Gazetteer confirms known relations
/// - GLiREL discovers new relations
/// - KGE predicts structural relations
///
/// Deduplicates same (head, tail, rel_type) triples keeping highest confidence.
/// Applies corroboration boost when multiple backends agree.

use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};

/// Orchestrates multiple relation extraction backends.
pub struct RelationChain {
    backends: Vec<Box<dyn RelationExtractor>>,
    /// Minimum confidence to keep a candidate relation.
    min_confidence: f32,
    /// Boost factor when multiple backends agree on same triple.
    corroboration_boost: f32,
}

impl RelationChain {
    pub fn new(min_confidence: f32) -> Self {
        Self {
            backends: Vec::new(),
            min_confidence,
            corroboration_boost: 0.10,
        }
    }

    /// Add a backend to the chain.
    pub fn add_backend(&mut self, backend: Box<dyn RelationExtractor>) {
        self.backends.push(backend);
    }

    /// Number of registered backends.
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }
}

impl RelationExtractor for RelationChain {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        if self.backends.is_empty() || input.entities.len() < 2 {
            return Vec::new();
        }

        // Collect all candidates from all backends
        let mut all: Vec<CandidateRelation> = Vec::new();
        for backend in &self.backends {
            let candidates = backend.extract_relations(input);
            tracing::debug!(
                backend = backend.name(),
                candidates = candidates.len(),
                "relation backend produced candidates"
            );
            all.extend(candidates);
        }

        // Deduplicate: group by (head_idx, tail_idx, rel_type), keep highest confidence
        // Track how many backends agreed for corroboration boost
        let mut groups: std::collections::HashMap<(usize, usize, String), (CandidateRelation, usize)> =
            std::collections::HashMap::new();

        for candidate in all {
            let key = (candidate.head_idx, candidate.tail_idx, candidate.rel_type.clone());
            let entry = groups.entry(key).or_insert_with(|| (candidate.clone(), 0));
            entry.1 += 1; // count agreements
            if candidate.confidence > entry.0.confidence {
                entry.0 = candidate;
            }
        }

        // Apply corroboration boost and filter
        let mut results: Vec<CandidateRelation> = groups
            .into_values()
            .filter_map(|(mut candidate, agreement_count)| {
                if agreement_count > 1 {
                    candidate.confidence =
                        (candidate.confidence + self.corroboration_boost).min(1.0);
                    tracing::debug!(
                        rel_type = %candidate.rel_type,
                        head = candidate.head_idx,
                        tail = candidate.tail_idx,
                        agreements = agreement_count,
                        boosted_confidence = candidate.confidence,
                        "corroboration boost applied"
                    );
                }
                if candidate.confidence >= self.min_confidence {
                    Some(candidate)
                } else {
                    None
                }
            })
            .collect();

        // Sort by confidence descending for deterministic output
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        results
    }

    fn name(&self) -> &str {
        "relation-chain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtractedEntity, ExtractionMethod};

    fn make_input() -> RelationExtractionInput {
        RelationExtractionInput {
            text: "Tim Cook is CEO of Apple Inc.".into(),
            entities: vec![
                ExtractedEntity {
                    text: "Tim Cook".into(),
                    entity_type: "PERSON".into(),
                    span: (0, 8),
                    confidence: 0.9,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(1),
                },
                ExtractedEntity {
                    text: "Apple Inc.".into(),
                    entity_type: "ORG".into(),
                    span: (19, 29),
                    confidence: 0.85,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(2),
                },
            ],
            language: "en".into(),
            area_of_interest: None,
        }
    }

    struct MockRelExtractor {
        name: &'static str,
        relations: Vec<(usize, usize, &'static str, f32)>,
    }

    impl RelationExtractor for MockRelExtractor {
        fn extract_relations(&self, _input: &RelationExtractionInput) -> Vec<CandidateRelation> {
            self.relations
                .iter()
                .map(|(h, t, rt, conf)| CandidateRelation {
                    head_idx: *h,
                    tail_idx: *t,
                    rel_type: rt.to_string(),
                    confidence: *conf,
                    method: ExtractionMethod::Gazetteer,
                })
                .collect()
        }
        fn name(&self) -> &str {
            self.name
        }
    }

    #[test]
    fn empty_chain_returns_empty() {
        let chain = RelationChain::new(0.1);
        let input = make_input();
        assert!(chain.extract_relations(&input).is_empty());
    }

    #[test]
    fn single_entity_returns_empty() {
        let mut chain = RelationChain::new(0.1);
        chain.add_backend(Box::new(MockRelExtractor {
            name: "test",
            relations: vec![(0, 1, "ceo_of", 0.8)],
        }));
        let input = RelationExtractionInput {
            text: "Tim Cook".into(),
            entities: vec![ExtractedEntity {
                text: "Tim Cook".into(),
                entity_type: "PERSON".into(),
                span: (0, 8),
                confidence: 0.9,
                method: ExtractionMethod::Gazetteer,
                language: "en".into(),
                resolved_to: None,
            }],
            language: "en".into(),
            area_of_interest: None,
        };
        assert!(chain.extract_relations(&input).is_empty());
    }

    #[test]
    fn dedup_keeps_highest_confidence() {
        let mut chain = RelationChain::new(0.1);
        chain.add_backend(Box::new(MockRelExtractor {
            name: "backend1",
            relations: vec![(0, 1, "ceo_of", 0.7)],
        }));
        chain.add_backend(Box::new(MockRelExtractor {
            name: "backend2",
            relations: vec![(0, 1, "ceo_of", 0.9)],
        }));
        let input = make_input();
        let results = chain.extract_relations(&input);
        assert_eq!(results.len(), 1);
        // Corroboration boost: 0.9 + 0.10 = 1.0 (capped)
        assert!(results[0].confidence >= 0.9);
    }

    #[test]
    fn min_confidence_filters() {
        let mut chain = RelationChain::new(0.5);
        chain.add_backend(Box::new(MockRelExtractor {
            name: "test",
            relations: vec![
                (0, 1, "ceo_of", 0.8),
                (0, 1, "knows", 0.2), // below threshold
            ],
        }));
        let input = make_input();
        let results = chain.extract_relations(&input);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_type, "ceo_of");
    }

    #[test]
    fn different_rel_types_kept_separately() {
        let mut chain = RelationChain::new(0.1);
        chain.add_backend(Box::new(MockRelExtractor {
            name: "test",
            relations: vec![
                (0, 1, "ceo_of", 0.8),
                (0, 1, "founded", 0.7),
            ],
        }));
        let input = make_input();
        let results = chain.extract_relations(&input);
        assert_eq!(results.len(), 2);
    }
}
