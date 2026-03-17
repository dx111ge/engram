/// Confidence calculation: learned trust * extraction confidence.
///
/// Implements the v1.1.0 confidence model:
/// - effective_trust = author_trust || source_trust || global_baseline
/// - initial_confidence = effective_trust * extraction_confidence
/// - Human-confirmed: 0.90 (direct assertion)
/// - LLM-generated: capped at 0.10 (safety rail)

use crate::types::{ExtractionMethod, ProcessedFact};

/// Confidence calculation configuration.
#[derive(Debug, Clone)]
pub struct ConfidenceConfig {
    /// Default trust for unknown sources/authors.
    pub global_baseline: f32,
    /// Confidence for human-confirmed facts (POST /store with source: user:*).
    pub human_confirmed: f32,
    /// Maximum confidence for LLM-generated facts.
    pub llm_cap: f32,
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        Self {
            global_baseline: 0.15,
            human_confirmed: 0.90,
            llm_cap: 0.10,
        }
    }
}

/// Calculates initial confidence for ingested facts.
pub struct ConfidenceCalculator {
    config: ConfidenceConfig,
}

impl ConfidenceCalculator {
    pub fn new(config: ConfidenceConfig) -> Self {
        Self { config }
    }

    /// Calculate confidence for a single fact.
    ///
    /// Looks up author trust, then source trust, then falls back to global baseline.
    /// Multiplies effective_trust by extraction_confidence.
    pub fn calculate(
        &self,
        fact: &ProcessedFact,
        graph: &engram_core::graph::Graph,
    ) -> f32 {
        // Special case: human-confirmed
        if fact.provenance.source.starts_with("user:") {
            return self.config.human_confirmed;
        }

        // Special case: LLM fallback
        if fact.extraction_method == ExtractionMethod::LlmFallback {
            return self.config.llm_cap;
        }

        // Resolve effective trust: author > source > baseline
        let effective_trust = self.resolve_trust(fact, graph);

        // Extraction confidence (from the NER/extraction method)
        let extraction_conf = fact.confidence.clamp(0.0, 1.0);

        // Final confidence = trust * extraction confidence
        let result = effective_trust * extraction_conf;
        result.clamp(0.0, 1.0)
    }

    /// Calculate confidence for a batch of facts in place.
    pub fn calculate_batch(
        &self,
        facts: &mut [ProcessedFact],
        graph: &engram_core::graph::Graph,
    ) {
        for fact in facts.iter_mut() {
            fact.confidence = self.calculate(fact, graph);
        }
    }

    /// Resolve effective trust: most specific wins.
    /// 1. Author trust (if author known + node exists in graph)
    /// 2. Source trust (if source node exists in graph)
    /// 3. Global baseline
    fn resolve_trust(
        &self,
        fact: &ProcessedFact,
        graph: &engram_core::graph::Graph,
    ) -> f32 {
        // Try author trust first
        if let Some(ref author) = fact.provenance.author {
            let author_label = format!("Author:{}", author);
            if let Ok(Some(node)) = graph.get_node(&author_label) {
                return node.confidence;
            }
        }

        // Try source trust
        let source_label = format!("Source:{}", fact.provenance.source);
        if let Ok(Some(node)) = graph.get_node(&source_label) {
            return node.confidence;
        }

        // Fallback to global baseline
        self.config.global_baseline
    }
}

impl Default for ConfidenceCalculator {
    fn default() -> Self {
        Self::new(ConfidenceConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Provenance, ExtractionMethod};
    use tempfile::TempDir;

    fn make_fact(entity: &str, source: &str, confidence: f32) -> ProcessedFact {
        ProcessedFact {
            entity: entity.into(),
            entity_type: Some("ORG".into()),
            properties: Default::default(),
            confidence,
            provenance: Provenance {
                source: source.into(),
                source_url: None,
                author: None,
                extraction_method: ExtractionMethod::StatisticalModel,
                fetched_at: 0,
                ingested_at: 0,
            },
            extraction_method: ExtractionMethod::StatisticalModel,
            language: "en".into(),
            relations: vec![],
            conflicts: vec![],
            resolution: None,
            source_text: None,
        }
    }

    fn test_graph() -> (TempDir, engram_core::graph::Graph) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = engram_core::graph::Graph::create(&path).unwrap();
        (dir, graph)
    }

    #[test]
    fn baseline_confidence_for_unknown_source() {
        let (_dir, graph) = test_graph();
        let calc = ConfidenceCalculator::default();

        let fact = make_fact("Apple", "unknown-source.com", 0.8);
        let conf = calc.calculate(&fact, &graph);

        // 0.15 (baseline) * 0.8 (extraction) = 0.12
        assert!((conf - 0.12).abs() < 0.001);
    }

    #[test]
    fn human_confirmed_gets_high_confidence() {
        let (_dir, graph) = test_graph();
        let calc = ConfidenceCalculator::default();

        let mut fact = make_fact("Apple", "user:alice", 0.8);
        fact.provenance.source = "user:alice".into();
        let conf = calc.calculate(&fact, &graph);

        assert_eq!(conf, 0.90);
    }

    #[test]
    fn llm_fallback_capped() {
        let (_dir, graph) = test_graph();
        let calc = ConfidenceCalculator::default();

        let mut fact = make_fact("Apple", "some-source", 0.95);
        fact.extraction_method = ExtractionMethod::LlmFallback;
        let conf = calc.calculate(&fact, &graph);

        assert_eq!(conf, 0.10);
    }

    #[test]
    fn source_trust_from_graph() {
        let (_dir, mut graph) = test_graph();
        let prov = engram_core::graph::Provenance::user("test");

        // Create a source node with learned trust 0.70
        graph.store_with_confidence("Source:reuters.com", 0.70, &prov).unwrap();

        let calc = ConfidenceCalculator::default();
        let fact = make_fact("Apple", "reuters.com", 0.9);
        let conf = calc.calculate(&fact, &graph);

        // 0.70 (source trust) * 0.9 (extraction) = 0.63
        assert!((conf - 0.63).abs() < 0.001);
    }

    #[test]
    fn author_trust_overrides_source_trust() {
        let (_dir, mut graph) = test_graph();
        let prov = engram_core::graph::Provenance::user("test");

        // Source trust = 0.30
        graph.store_with_confidence("Source:x.com", 0.30, &prov).unwrap();
        // Author trust = 0.80 (more specific)
        graph.store_with_confidence("Author:twitter:@good_analyst", 0.80, &prov).unwrap();

        let calc = ConfidenceCalculator::default();
        let mut fact = make_fact("Apple", "x.com", 0.9);
        fact.provenance.author = Some("twitter:@good_analyst".into());
        let conf = calc.calculate(&fact, &graph);

        // 0.80 (author trust, overrides source) * 0.9 = 0.72
        assert!((conf - 0.72).abs() < 0.001);
    }

    #[test]
    fn batch_calculation() {
        let (_dir, graph) = test_graph();
        let calc = ConfidenceCalculator::default();

        let mut facts = vec![
            make_fact("Apple", "unknown.com", 0.8),
            make_fact("Microsoft", "unknown.com", 0.6),
        ];

        calc.calculate_batch(&mut facts, &graph);

        assert!((facts[0].confidence - 0.12).abs() < 0.001); // 0.15 * 0.8
        assert!((facts[1].confidence - 0.09).abs() < 0.001); // 0.15 * 0.6
    }
}
