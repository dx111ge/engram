/// NER chain: orchestrates multiple NER backends with cascade/merge strategies.
///
/// The chain runs backends in priority order and combines results
/// according to the configured strategy.

use crate::traits::Extractor;
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// How to combine results from multiple NER backends.
#[derive(Debug, Clone, Copy, Default)]
pub enum ChainStrategy {
    /// Stop at first backend that produces results.
    #[default]
    Cascade,
    /// Run all backends, merge and deduplicate results.
    MergeAll,
    /// Run next backend only if previous found fewer than N entities.
    CascadeThreshold(usize),
}

/// Orchestrates multiple NER backends into a single extraction pipeline.
pub struct NerChain {
    /// Ordered backends — higher priority first.
    backends: Vec<Box<dyn Extractor>>,
    /// How to combine results.
    strategy: ChainStrategy,
}

impl NerChain {
    pub fn new(strategy: ChainStrategy) -> Self {
        Self {
            backends: Vec::new(),
            strategy,
        }
    }

    /// Add a backend to the chain (appended at end = lowest priority).
    pub fn add_backend(&mut self, backend: Box<dyn Extractor>) {
        self.backends.push(backend);
    }

    /// Number of registered backends.
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }
}

impl Extractor for NerChain {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        match self.strategy {
            ChainStrategy::Cascade => self.extract_cascade(text, lang),
            ChainStrategy::MergeAll => self.extract_merge(text, lang),
            ChainStrategy::CascadeThreshold(min) => self.extract_cascade_threshold(text, lang, min),
        }
    }

    fn name(&self) -> &str {
        "ner-chain"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel // chain delegates to actual backends
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // delegates language filtering to individual backends
    }
}

impl NerChain {
    /// Cascade: first backend that produces results wins.
    fn extract_cascade(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        for backend in &self.backends {
            let supported = backend.supported_languages();
            if !supported.is_empty() && !supported.contains(&lang.code) {
                continue;
            }
            let entities = backend.extract(text, lang);
            if !entities.is_empty() {
                tracing::debug!(
                    backend = backend.name(),
                    entities = entities.len(),
                    "cascade: backend produced results"
                );
                return entities;
            }
        }
        Vec::new()
    }

    /// Merge: run all backends, deduplicate overlapping spans.
    fn extract_merge(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        let mut all: Vec<ExtractedEntity> = Vec::new();

        for backend in &self.backends {
            let supported = backend.supported_languages();
            if !supported.is_empty() && !supported.contains(&lang.code) {
                continue;
            }
            let entities = backend.extract(text, lang);
            all.extend(entities);
        }

        dedup_entities(all)
    }

    /// Cascade with threshold: run next backend if previous found < min entities.
    fn extract_cascade_threshold(
        &self,
        text: &str,
        lang: &DetectedLanguage,
        min: usize,
    ) -> Vec<ExtractedEntity> {
        let mut accumulated: Vec<ExtractedEntity> = Vec::new();

        for backend in &self.backends {
            let supported = backend.supported_languages();
            if !supported.is_empty() && !supported.contains(&lang.code) {
                continue;
            }
            let entities = backend.extract(text, lang);
            accumulated.extend(entities);

            if accumulated.len() >= min {
                break;
            }
        }

        dedup_entities(accumulated)
    }
}

/// Deduplicate entities with overlapping spans.
/// When spans overlap, keep the one with higher confidence.
/// If the dropped entity had a `resolved_to` node ID (e.g. from gazetteer),
/// merge it into the winner so entity resolution info is not lost.
fn dedup_entities(mut entities: Vec<ExtractedEntity>) -> Vec<ExtractedEntity> {
    if entities.len() <= 1 {
        return entities;
    }

    // Sort by span start, then by confidence descending
    entities.sort_by(|a, b| {
        a.span
            .0
            .cmp(&b.span.0)
            .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut result: Vec<ExtractedEntity> = Vec::new();
    for entity in entities {
        let overlap_idx = result.iter().position(|existing| {
            // Overlap check: spans intersect
            existing.span.0 < entity.span.1 && entity.span.0 < existing.span.1
        });
        match overlap_idx {
            Some(idx) => {
                // Winner already in result; merge resolved_to from the dropped entity
                if result[idx].resolved_to.is_none() && entity.resolved_to.is_some() {
                    result[idx].resolved_to = entity.resolved_to;
                }
            }
            None => {
                result.push(entity);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_lang() -> DetectedLanguage {
        DetectedLanguage {
            code: "en".into(),
            confidence: 1.0,
        }
    }

    /// Mock extractor that always returns fixed entities.
    struct MockExtractor {
        name: &'static str,
        entities: Vec<(&'static str, &'static str, (usize, usize), f32)>,
    }

    impl Extractor for MockExtractor {
        fn extract(&self, _text: &str, _lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
            self.entities
                .iter()
                .map(|(text, etype, span, conf)| ExtractedEntity {
                    text: text.to_string(),
                    entity_type: etype.to_string(),
                    span: *span,
                    confidence: *conf,
                    method: ExtractionMethod::StatisticalModel,
                    language: "en".into(),
                    resolved_to: None,
                })
                .collect()
        }

        fn name(&self) -> &str {
            self.name
        }

        fn method(&self) -> ExtractionMethod {
            ExtractionMethod::StatisticalModel
        }

        fn supported_languages(&self) -> Vec<String> {
            vec![]
        }
    }

    /// Mock extractor that returns nothing.
    struct EmptyExtractor;
    impl Extractor for EmptyExtractor {
        fn extract(&self, _text: &str, _lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
            vec![]
        }
        fn name(&self) -> &str { "empty" }
        fn method(&self) -> ExtractionMethod { ExtractionMethod::StatisticalModel }
        fn supported_languages(&self) -> Vec<String> { vec![] }
    }

    #[test]
    fn cascade_returns_first_non_empty() {
        let mut chain = NerChain::new(ChainStrategy::Cascade);
        chain.add_backend(Box::new(EmptyExtractor));
        chain.add_backend(Box::new(MockExtractor {
            name: "backend2",
            entities: vec![("Apple", "ORG", (0, 5), 0.9)],
        }));

        let entities = chain.extract("Apple stock", &en_lang());
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "Apple");
    }

    #[test]
    fn cascade_skips_after_first_hit() {
        let mut chain = NerChain::new(ChainStrategy::Cascade);
        chain.add_backend(Box::new(MockExtractor {
            name: "backend1",
            entities: vec![("Apple", "ORG", (0, 5), 0.9)],
        }));
        chain.add_backend(Box::new(MockExtractor {
            name: "backend2",
            entities: vec![("Apple", "FOOD", (0, 5), 0.5)], // should not be reached
        }));

        let entities = chain.extract("Apple stock", &en_lang());
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "ORG"); // from backend1, not backend2
    }

    #[test]
    fn merge_all_combines_and_deduplicates() {
        let mut chain = NerChain::new(ChainStrategy::MergeAll);
        chain.add_backend(Box::new(MockExtractor {
            name: "backend1",
            entities: vec![("Apple", "ORG", (0, 5), 0.9)],
        }));
        chain.add_backend(Box::new(MockExtractor {
            name: "backend2",
            entities: vec![
                ("Apple", "ORG", (0, 5), 0.7),   // overlap with backend1, lower conf
                ("Tim Cook", "PERSON", (10, 18), 0.85),
            ],
        }));

        let entities = chain.extract("Apple and Tim Cook", &en_lang());
        assert_eq!(entities.len(), 2);
        // Apple should be the 0.9 version (higher confidence)
        let apple = entities.iter().find(|e| e.text == "Apple").unwrap();
        assert_eq!(apple.confidence, 0.9);
    }

    #[test]
    fn cascade_threshold_continues_until_min() {
        let mut chain = NerChain::new(ChainStrategy::CascadeThreshold(2));
        chain.add_backend(Box::new(MockExtractor {
            name: "backend1",
            entities: vec![("Apple", "ORG", (0, 5), 0.9)], // only 1 entity
        }));
        chain.add_backend(Box::new(MockExtractor {
            name: "backend2",
            entities: vec![("Tim Cook", "PERSON", (10, 18), 0.85)], // now 2 total
        }));

        let entities = chain.extract("Apple and Tim Cook", &en_lang());
        assert_eq!(entities.len(), 2); // both backends contributed
    }

    #[test]
    fn empty_chain_returns_empty() {
        let chain = NerChain::new(ChainStrategy::Cascade);
        let entities = chain.extract("some text", &en_lang());
        assert!(entities.is_empty());
    }
}
