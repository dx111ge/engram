/// Learned patterns: extracts entity patterns from graph co-occurrence data.
///
/// Scans the graph's co-occurrence counters to discover frequent entity
/// pairs. These patterns are then used to boost NER confidence when
/// extracted entities co-occur with known patterns.

use std::collections::HashMap;

use crate::types::{ExtractedEntity, ExtractionMethod};

/// A learned co-occurrence pattern.
#[derive(Debug, Clone)]
pub struct LearnedPattern {
    /// First entity in the pattern.
    pub entity_a: String,
    /// Second entity in the pattern.
    pub entity_b: String,
    /// Co-occurrence count.
    pub count: u32,
    /// Probability P(B|A).
    pub probability: f32,
}

/// Learned pattern extractor using graph co-occurrence statistics.
pub struct PatternExtractor {
    /// Minimum co-occurrence count to qualify as a pattern.
    min_count: u32,
    /// Minimum probability to qualify as a pattern.
    min_probability: f32,
    /// Confidence boost for entities found in learned patterns.
    boost: f32,
}

impl PatternExtractor {
    pub fn new(min_count: u32, min_probability: f32, boost: f32) -> Self {
        Self {
            min_count,
            min_probability,
            boost,
        }
    }

    /// Extract learned patterns from the graph's co-occurrence data.
    ///
    /// Scans co-occurrences for the given seed entities and filters by
    /// minimum count and probability thresholds.
    pub fn extract_patterns(
        &self,
        graph: &engram_core::graph::Graph,
        seed_entities: &[String],
    ) -> Vec<LearnedPattern> {
        let mut patterns = Vec::new();

        for entity_a in seed_entities {
            let cooccurrences = graph.cooccurrences_for(entity_a);
            for (entity_b, count) in cooccurrences {
                if count < self.min_count {
                    continue;
                }
                // Get probability from the graph
                if let Some((_, probability)) = graph.get_cooccurrence(entity_a, &entity_b) {
                    if probability >= self.min_probability {
                        patterns.push(LearnedPattern {
                            entity_a: entity_a.clone(),
                            entity_b,
                            count,
                            probability,
                        });
                    }
                }
            }
        }

        patterns
    }

    /// Boost confidence for entities that match learned patterns.
    ///
    /// If an extracted entity appears in a learned pattern alongside another
    /// entity also found in the same text, boost its confidence.
    pub fn boost_from_patterns(
        &self,
        entities: &mut [ExtractedEntity],
        patterns: &[LearnedPattern],
    ) {
        if patterns.is_empty() || entities.len() < 2 {
            return;
        }

        // Build a lookup: entity -> pattern partners
        let mut pattern_map: HashMap<&str, Vec<(&str, f32)>> = HashMap::new();
        for pattern in patterns {
            pattern_map
                .entry(&pattern.entity_a)
                .or_default()
                .push((&pattern.entity_b, pattern.probability));
            pattern_map
                .entry(&pattern.entity_b)
                .or_default()
                .push((&pattern.entity_a, pattern.probability));
        }

        // Collect entity texts for cross-checking
        let entity_texts: Vec<String> = entities.iter().map(|e| e.text.clone()).collect();

        for entity in entities.iter_mut() {
            if let Some(partners) = pattern_map.get(entity.text.as_str()) {
                for (partner, probability) in partners {
                    if entity_texts.iter().any(|t| t == *partner) {
                        // Co-occurring entity found! Boost confidence.
                        let boost = self.boost * probability;
                        entity.confidence = (entity.confidence + boost).min(1.0);
                        entity.method = ExtractionMethod::LearnedPattern;
                        break; // one boost per entity
                    }
                }
            }
        }
    }
}

impl Default for PatternExtractor {
    fn default() -> Self {
        Self::new(3, 0.3, 0.1)
    }
}

/// NER correction feedback: adjusts NER rules based on user corrections.
///
/// When a user corrects an entity via POST /learn/correct, the correction
/// is recorded. The feedback loop:
/// 1. Tracks false positives (entities that were wrong)
/// 2. Tracks false negatives (entities that were missed)
/// 3. Adjusts rule confidence scores based on correction history
#[derive(Debug, Clone, Default)]
pub struct NerFeedback {
    /// False positive corrections: entity -> count.
    pub false_positives: HashMap<String, u32>,
    /// Missed entity reports: entity -> count.
    pub false_negatives: HashMap<String, u32>,
    /// Rule accuracy adjustments: rule_name -> correction_factor.
    pub rule_adjustments: HashMap<String, f32>,
}

impl NerFeedback {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a false positive: entity was extracted but shouldn't have been.
    pub fn record_false_positive(&mut self, entity: &str) {
        *self.false_positives.entry(entity.to_string()).or_insert(0) += 1;
    }

    /// Record a false negative: entity should have been extracted but wasn't.
    pub fn record_false_negative(&mut self, entity: &str) {
        *self.false_negatives.entry(entity.to_string()).or_insert(0) += 1;
    }

    /// Check if an entity has been frequently corrected (potential false positive).
    pub fn is_likely_false_positive(&self, entity: &str, threshold: u32) -> bool {
        self.false_positives
            .get(entity)
            .is_some_and(|&count| count >= threshold)
    }

    /// Get confidence penalty for a frequently-corrected entity.
    /// Returns 0.0 for no penalty, up to 0.5 for heavily corrected entities.
    pub fn confidence_penalty(&self, entity: &str) -> f32 {
        match self.false_positives.get(entity) {
            None => 0.0,
            Some(&count) => {
                // Logarithmic penalty: 0.1 per doubling
                let penalty = 0.1 * (count as f32).log2().max(0.0);
                penalty.min(0.5)
            }
        }
    }

    /// Apply feedback penalties to extracted entities.
    pub fn apply_penalties(&self, entities: &mut [ExtractedEntity], threshold: u32) {
        for entity in entities.iter_mut() {
            if self.is_likely_false_positive(&entity.text, threshold) {
                let penalty = self.confidence_penalty(&entity.text);
                entity.confidence = (entity.confidence - penalty).max(0.0);
            }
        }
    }

    /// Total corrections recorded.
    pub fn total_corrections(&self) -> u32 {
        let fp: u32 = self.false_positives.values().sum();
        let fn_: u32 = self.false_negatives.values().sum();
        fp + fn_
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(text: &str, confidence: f32) -> ExtractedEntity {
        ExtractedEntity {
            text: text.into(),
            entity_type: "ORG".into(),
            span: (0, text.len()),
            confidence,
            method: ExtractionMethod::StatisticalModel,
            language: "en".into(),
            resolved_to: None,
        }
    }

    #[test]
    fn pattern_boost_applied_when_cooccurring() {
        let extractor = PatternExtractor::new(1, 0.1, 0.15);

        let patterns = vec![LearnedPattern {
            entity_a: "Apple".into(),
            entity_b: "Tim Cook".into(),
            count: 10,
            probability: 0.8,
        }];

        let mut entities = vec![
            make_entity("Apple", 0.6),
            make_entity("Tim Cook", 0.5),
        ];

        extractor.boost_from_patterns(&mut entities, &patterns);

        // Apple should be boosted: 0.6 + (0.15 * 0.8) = 0.72
        assert!((entities[0].confidence - 0.72).abs() < 0.001);
        assert_eq!(entities[0].method, ExtractionMethod::LearnedPattern);
    }

    #[test]
    fn no_boost_without_cooccurring_partner() {
        let extractor = PatternExtractor::new(1, 0.1, 0.15);

        let patterns = vec![LearnedPattern {
            entity_a: "Apple".into(),
            entity_b: "Tim Cook".into(),
            count: 10,
            probability: 0.8,
        }];

        let mut entities = vec![make_entity("Apple", 0.6)];
        extractor.boost_from_patterns(&mut entities, &patterns);

        // No boost: Tim Cook not in extracted entities
        assert!((entities[0].confidence - 0.6).abs() < 0.001);
    }

    #[test]
    fn feedback_false_positive_tracking() {
        let mut feedback = NerFeedback::new();
        feedback.record_false_positive("Apple");
        feedback.record_false_positive("Apple");
        feedback.record_false_positive("Apple");

        assert!(feedback.is_likely_false_positive("Apple", 3));
        assert!(!feedback.is_likely_false_positive("Apple", 4));
        assert!(!feedback.is_likely_false_positive("Google", 1));
    }

    #[test]
    fn feedback_penalty_increases_with_corrections() {
        let mut feedback = NerFeedback::new();
        let p0 = feedback.confidence_penalty("X");
        assert_eq!(p0, 0.0);

        for _ in 0..8 {
            feedback.record_false_positive("X");
        }
        let p8 = feedback.confidence_penalty("X");
        assert!(p8 > 0.0);
        assert!(p8 <= 0.5);
    }

    #[test]
    fn apply_penalties_reduces_confidence() {
        let mut feedback = NerFeedback::new();
        for _ in 0..4 {
            feedback.record_false_positive("BadEntity");
        }

        let mut entities = vec![
            make_entity("GoodEntity", 0.8),
            make_entity("BadEntity", 0.8),
        ];

        feedback.apply_penalties(&mut entities, 3);
        assert_eq!(entities[0].confidence, 0.8); // untouched
        assert!(entities[1].confidence < 0.8);   // penalized
    }

    #[test]
    fn total_corrections() {
        let mut feedback = NerFeedback::new();
        feedback.record_false_positive("A");
        feedback.record_false_positive("A");
        feedback.record_false_negative("B");
        assert_eq!(feedback.total_corrections(), 3);
    }
}
