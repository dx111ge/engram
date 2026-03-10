/// Pipeline executor: orchestrates stages, manages workers, batch writes.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::IngestError;
use crate::traits::{Extractor, LanguageDetector, Parser, Resolver, Transformer};
use crate::types::{PipelineConfig, PipelineResult, RawItem};

/// The ingest pipeline executor.
///
/// Owns the stage implementations and orchestrates execution across
/// multiple worker threads. Uses rayon for CPU-bound NER work and
/// tokio for async I/O (source fetching, batch writes).
pub struct Pipeline {
    config: PipelineConfig,
    #[allow(dead_code)] // used in Phase 8.2+ (resolve, dedup, load stages)
    graph: Arc<RwLock<engram_core::graph::Graph>>,
    parsers: Vec<Box<dyn Parser>>,
    language_detector: Option<Box<dyn LanguageDetector>>,
    extractors: Vec<Box<dyn Extractor>>,
    resolvers: Vec<Box<dyn Resolver>>,
    transformers: Vec<Box<dyn Transformer>>,
}

impl Pipeline {
    /// Create a new pipeline with the given graph and config.
    pub fn new(
        graph: Arc<RwLock<engram_core::graph::Graph>>,
        config: PipelineConfig,
    ) -> Self {
        Self {
            config,
            graph,
            parsers: Vec::new(),
            language_detector: None,
            extractors: Vec::new(),
            resolvers: Vec::new(),
            transformers: Vec::new(),
        }
    }

    /// Add a parser to the pipeline.
    pub fn add_parser(&mut self, parser: Box<dyn Parser>) {
        self.parsers.push(parser);
    }

    /// Set the language detector.
    pub fn set_language_detector(&mut self, detector: Box<dyn LanguageDetector>) {
        self.language_detector = Some(detector);
    }

    /// Add an extractor (NER backend) to the pipeline.
    /// Extractors are run in order (cascade by default).
    pub fn add_extractor(&mut self, extractor: Box<dyn Extractor>) {
        self.extractors.push(extractor);
    }

    /// Add a resolver for entity resolution.
    pub fn add_resolver(&mut self, resolver: Box<dyn Resolver>) {
        self.resolvers.push(resolver);
    }

    /// Add a transformer to the post-processing chain.
    pub fn add_transformer(&mut self, transformer: Box<dyn Transformer>) {
        self.transformers.push(transformer);
    }

    /// Get the pipeline configuration.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Execute the pipeline on a batch of raw items.
    ///
    /// This is the main entry point. It:
    /// 1. Parses raw items into text segments
    /// 2. Detects language per segment
    /// 3. Runs NER extraction (parallelized across workers)
    /// 4. Resolves entities against the graph (read lock)
    /// 5. Deduplicates
    /// 6. Checks for conflicts
    /// 7. Calculates confidence
    /// 8. Applies transformers
    /// 9. Batch-writes to graph (write lock, chunked)
    ///
    /// Returns a summary of the pipeline run.
    pub async fn execute(&self, items: Vec<RawItem>) -> Result<PipelineResult, IngestError> {
        let start = std::time::Instant::now();
        let mut result = PipelineResult::default();

        if items.is_empty() {
            return Ok(result);
        }

        tracing::info!(
            pipeline = %self.config.name,
            items = items.len(),
            "starting pipeline execution"
        );

        // Phase 1: Parse raw items into text segments
        let _texts = if self.config.stages.parse {
            self.parse_items(&items, &mut result)?
        } else {
            // No parsing — extract text directly from content
            items
                .iter()
                .filter_map(|item| match &item.content {
                    crate::types::Content::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        // Phase 2-9: TODO — implemented in subsequent build tasks (8.2+)
        // Each phase will be added as its build task is completed.

        result.duration_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            pipeline = %self.config.name,
            facts_stored = result.facts_stored,
            relations = result.relations_created,
            errors = result.errors.len(),
            duration_ms = result.duration_ms,
            "pipeline execution complete"
        );

        Ok(result)
    }

    /// Parse raw items into text segments using registered parsers.
    fn parse_items(
        &self,
        items: &[RawItem],
        result: &mut PipelineResult,
    ) -> Result<Vec<String>, IngestError> {
        let mut texts = Vec::new();

        for item in items {
            let mut parsed = false;
            for parser in &self.parsers {
                match parser.parse(&item.content) {
                    Ok(segments) => {
                        texts.extend(segments);
                        parsed = true;
                        break;
                    }
                    Err(_) => continue, // try next parser
                }
            }

            if !parsed {
                // Fallback: extract text directly
                match &item.content {
                    crate::types::Content::Text(t) => texts.push(t.clone()),
                    crate::types::Content::Structured(map) => {
                        // Concatenate all values as text
                        let text = map.values().cloned().collect::<Vec<_>>().join(" ");
                        if !text.is_empty() {
                            texts.push(text);
                        }
                    }
                    crate::types::Content::Bytes { mime, .. } => {
                        result
                            .errors
                            .push(format!("no parser for MIME type: {}", mime));
                    }
                }
            }
        }

        Ok(texts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Content, PipelineConfig, RawItem};
    use tempfile::TempDir;

    fn test_graph() -> Arc<RwLock<engram_core::graph::Graph>> {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = engram_core::graph::Graph::create(&path).unwrap();
        Arc::new(RwLock::new(graph))
    }

    #[tokio::test]
    async fn empty_pipeline_returns_default_result() {
        let graph = test_graph();
        let pipeline = Pipeline::new(graph, PipelineConfig::default());
        let result = pipeline.execute(vec![]).await.unwrap();
        assert_eq!(result.facts_stored, 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[tokio::test]
    async fn pipeline_extracts_text_from_raw_items() {
        let graph = test_graph();
        let config = PipelineConfig {
            stages: crate::types::StageConfig {
                parse: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let pipeline = Pipeline::new(graph, config);

        let items = vec![RawItem {
            content: Content::Text("Hello world".into()),
            source_url: None,
            source_name: "test".into(),
            fetched_at: 0,
            metadata: Default::default(),
        }];

        let result = pipeline.execute(items).await.unwrap();
        // No extractors registered, so no facts stored — but no errors either
        assert_eq!(result.facts_stored, 0);
        assert!(result.errors.is_empty());
    }
}
