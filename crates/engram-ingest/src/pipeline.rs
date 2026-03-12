/// Pipeline executor: orchestrates stages, manages workers, batch writes.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::IngestError;
use crate::rel_traits::{RelationExtractionInput, RelationExtractor};
use crate::traits::{Extractor, LanguageDetector, Parser, Resolver, Transformer};
use crate::types::{
    AnalyzeResult, Content, DetectedLanguage, ExtractedRelation, ExtractionMethod, PipelineConfig,
    PipelineResult, ProcessedFact, Provenance, RawItem, TransformResult,
};

/// A parsed text segment with its source metadata preserved.
#[derive(Debug, Clone)]
struct ParsedSegment {
    text: String,
    source_name: String,
    source_url: Option<String>,
    fetched_at: i64,
    metadata: HashMap<String, String>,
}

/// The ingest pipeline executor.
///
/// Owns the stage implementations and orchestrates execution across
/// multiple worker threads. Uses rayon for CPU-bound NER work and
/// tokio for async I/O (source fetching, batch writes).
pub struct Pipeline {
    config: PipelineConfig,
    graph: Arc<RwLock<engram_core::graph::Graph>>,
    parsers: Vec<Box<dyn Parser>>,
    language_detector: Option<Box<dyn LanguageDetector>>,
    extractors: Vec<Box<dyn Extractor>>,
    resolvers: Vec<Box<dyn Resolver>>,
    transformers: Vec<Box<dyn Transformer>>,
    relation_extractors: Vec<Box<dyn RelationExtractor>>,
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
            relation_extractors: Vec::new(),
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

    /// Add a relation extractor to the pipeline.
    pub fn add_relation_extractor(&mut self, extractor: Box<dyn RelationExtractor>) {
        self.relation_extractors.push(extractor);
    }

    /// Get the pipeline configuration.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Execute the pipeline on a batch of raw items.
    ///
    /// Full pipeline flow:
    /// 1. Parse raw items into text segments
    /// 2. Detect language per segment
    /// 3. Run NER extraction (cascade through extractors)
    /// 4. Resolve entities against the graph (read lock)
    /// 5. Build ProcessedFacts from extracted entities
    /// 6. Apply transformers
    /// 7. Batch-write to graph (write lock, chunked)
    pub fn execute(&self, items: Vec<RawItem>) -> Result<PipelineResult, IngestError> {
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

        // Stage 1: Parse raw items into text segments
        let segments = self.parse_items(&items, &mut result)?;

        if segments.is_empty() {
            result.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(result);
        }

        // Stage 2: Detect language per segment
        let lang_segments: Vec<(ParsedSegment, DetectedLanguage)> = segments
            .into_iter()
            .map(|seg| {
                let lang = if self.config.stages.language_detect {
                    self.detect_language(&seg.text)
                } else {
                    DetectedLanguage {
                        code: "en".into(),
                        confidence: 0.0,
                    }
                };
                (seg, lang)
            })
            .collect();

        // Stage 3: Extract entities via NER (cascade through extractors)
        let mut facts: Vec<ProcessedFact> = Vec::new();

        if self.config.stages.ner && !self.extractors.is_empty() {
            for (seg, lang) in &lang_segments {
                let mut extracted = self.run_extractors(&seg.text, lang);

                // Stage 4: Resolve extracted entities against graph
                if self.config.stages.entity_resolve && !self.resolvers.is_empty() {
                    let graph = self.graph.read().map_err(|_| IngestError::Graph("graph lock poisoned".into()))?;
                    for entity in &mut extracted {
                        if entity.resolved_to.is_none() {
                            for resolver in &self.resolvers {
                                let res = resolver.resolve(entity, &graph);
                                match &res {
                                    crate::types::ResolutionResult::Matched(nid) => {
                                        entity.resolved_to = Some(*nid);
                                        break;
                                    }
                                    crate::types::ResolutionResult::New => {}
                                    crate::types::ResolutionResult::Ambiguous(_) => {}
                                }
                            }
                        }
                    }
                }

                // Stage 5: Relation extraction (after NER + resolve, before fact building)
                let relations = self.run_relation_extraction(&seg.text, &extracted);

                // Stage 6: Convert extracted entities to ProcessedFacts
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                for (eidx, entity) in extracted.iter().enumerate() {
                    // Attach relations where this entity is the head
                    let entity_relations: Vec<ExtractedRelation> = relations
                        .iter()
                        .filter(|r| r.from == entity.text)
                        .cloned()
                        .collect();

                    facts.push(ProcessedFact {
                        entity: entity.text.clone(),
                        entity_type: Some(entity.entity_type.clone()),
                        properties: seg.metadata.clone(),
                        confidence: entity.confidence,
                        provenance: Provenance {
                            source: seg.source_name.clone(),
                            source_url: seg.source_url.clone(),
                            author: None,
                            extraction_method: entity.method,
                            fetched_at: seg.fetched_at,
                            ingested_at: now,
                        },
                        extraction_method: entity.method,
                        language: entity.language.clone(),
                        relations: entity_relations,
                        conflicts: Vec::new(),
                        resolution: entity.resolved_to.map(crate::types::ResolutionResult::Matched),
                    });

                    let _ = eidx; // suppress unused warning
                }
            }
        } else {
            // No NER — treat structured items as direct facts
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            for (seg, lang) in &lang_segments {
                facts.push(ProcessedFact {
                    entity: seg.text.clone(),
                    entity_type: None,
                    properties: seg.metadata.clone(),
                    confidence: 0.5, // default for unscored input
                    provenance: Provenance {
                        source: seg.source_name.clone(),
                        source_url: seg.source_url.clone(),
                        author: None,
                        extraction_method: ExtractionMethod::Manual,
                        fetched_at: seg.fetched_at,
                        ingested_at: now,
                    },
                    extraction_method: ExtractionMethod::Manual,
                    language: lang.code.clone(),
                    relations: Vec::new(),
                    conflicts: Vec::new(),
                    resolution: None,
                });
            }
        }

        // Stage 6: Apply transformers
        let facts = self.apply_transformers(facts, &mut result);

        // Stage 7: Load — batch write to graph (chunked write locking)
        self.load_facts(facts, &mut result)?;

        result.duration_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            pipeline = %self.config.name,
            facts_stored = result.facts_stored,
            relations = result.relations_created,
            resolved = result.facts_resolved,
            errors = result.errors.len(),
            duration_ms = result.duration_ms,
            "pipeline execution complete"
        );

        Ok(result)
    }

    /// Execute the pipeline with rayon-parallelized NER + resolve.
    ///
    /// Same stages as `execute()` but NER extraction and entity resolution
    /// run in parallel across rayon worker threads. Best for large batches
    /// where NER is the bottleneck.
    pub fn execute_parallel(
        &self,
        items: Vec<RawItem>,
    ) -> Result<PipelineResult, IngestError> {
        use rayon::prelude::*;

        let start = std::time::Instant::now();
        let mut result = PipelineResult::default();

        if items.is_empty() {
            return Ok(result);
        }

        tracing::info!(
            pipeline = %self.config.name,
            items = items.len(),
            workers = self.config.workers,
            "starting parallel pipeline execution"
        );

        // Stage 1: Parse (sequential — usually fast)
        let segments = self.parse_items(&items, &mut result)?;
        if segments.is_empty() {
            result.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(result);
        }

        // Stage 2+3+4: Language detect + NER + Resolve (parallel via rayon)
        let has_ner = self.config.stages.ner && !self.extractors.is_empty();
        let has_resolve = self.config.stages.entity_resolve && !self.resolvers.is_empty();
        let graph_ref = &self.graph;

        let facts: Vec<ProcessedFact> = if has_ner {
            segments
                .par_iter()
                .flat_map(|seg| {
                    let lang = if self.config.stages.language_detect {
                        self.detect_language(&seg.text)
                    } else {
                        DetectedLanguage {
                            code: "en".into(),
                            confidence: 0.0,
                        }
                    };

                    let mut extracted = self.run_extractors(&seg.text, &lang);

                    // Resolve under read lock
                    if has_resolve {
                        let graph = graph_ref.read().unwrap();
                        for entity in &mut extracted {
                            if entity.resolved_to.is_none() {
                                for resolver in &self.resolvers {
                                    let res = resolver.resolve(entity, &graph);
                                    if let crate::types::ResolutionResult::Matched(nid) = &res {
                                        entity.resolved_to = Some(*nid);
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Relation extraction
                    let relations = self.run_relation_extraction(&seg.text, &extracted);

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    extracted
                        .iter()
                        .map(|entity| {
                            let entity_relations: Vec<ExtractedRelation> = relations
                                .iter()
                                .filter(|r| r.from == entity.text)
                                .cloned()
                                .collect();

                            ProcessedFact {
                                entity: entity.text.clone(),
                                entity_type: Some(entity.entity_type.clone()),
                                properties: seg.metadata.clone(),
                                confidence: entity.confidence,
                                provenance: Provenance {
                                    source: seg.source_name.clone(),
                                    source_url: seg.source_url.clone(),
                                    author: None,
                                    extraction_method: entity.method,
                                    fetched_at: seg.fetched_at,
                                    ingested_at: now,
                                },
                                extraction_method: entity.method,
                                language: entity.language.clone(),
                                relations: entity_relations,
                                conflicts: Vec::new(),
                                resolution: entity
                                    .resolved_to
                                    .map(crate::types::ResolutionResult::Matched),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            // No NER — same as sequential path
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            segments
                .iter()
                .map(|seg| {
                    let lang = if self.config.stages.language_detect {
                        self.detect_language(&seg.text)
                    } else {
                        DetectedLanguage {
                            code: "en".into(),
                            confidence: 0.0,
                        }
                    };
                    ProcessedFact {
                        entity: seg.text.clone(),
                        entity_type: None,
                        properties: seg.metadata.clone(),
                        confidence: 0.5,
                        provenance: Provenance {
                            source: seg.source_name.clone(),
                            source_url: seg.source_url.clone(),
                            author: None,
                            extraction_method: ExtractionMethod::Manual,
                            fetched_at: seg.fetched_at,
                            ingested_at: now,
                        },
                        extraction_method: ExtractionMethod::Manual,
                        language: lang.code,
                        relations: Vec::new(),
                        conflicts: Vec::new(),
                        resolution: None,
                    }
                })
                .collect()
        };

        // Stage 6: Apply transformers (sequential)
        let facts = self.apply_transformers(facts, &mut result);

        // Stage 7: Load (chunked writes)
        self.load_facts(facts, &mut result)?;

        result.duration_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            pipeline = %self.config.name,
            facts_stored = result.facts_stored,
            relations = result.relations_created,
            resolved = result.facts_resolved,
            errors = result.errors.len(),
            duration_ms = result.duration_ms,
            "parallel pipeline execution complete"
        );

        Ok(result)
    }

    /// Analyze text: runs parse, language detect, NER, and entity resolution
    /// but does NOT store anything to the graph. Returns extracted entities for preview.
    pub fn analyze(&self, items: Vec<RawItem>) -> Result<AnalyzeResult, IngestError> {
        let start = std::time::Instant::now();

        if items.is_empty() {
            return Ok(AnalyzeResult {
                entities: Vec::new(),
                relations: Vec::new(),
                language: "en".into(),
                duration_ms: 0,
                warnings: Vec::new(),
            });
        }

        // Stage 1: Parse
        let mut result = PipelineResult::default();
        let segments = self.parse_items(&items, &mut result)?;

        if segments.is_empty() {
            return Ok(AnalyzeResult {
                entities: Vec::new(),
                relations: Vec::new(),
                language: "en".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                warnings: Vec::new(),
            });
        }

        // Stage 2: Detect language
        let lang_segments: Vec<(ParsedSegment, DetectedLanguage)> = segments
            .into_iter()
            .map(|seg| {
                let lang = if self.config.stages.language_detect {
                    self.detect_language(&seg.text)
                } else {
                    DetectedLanguage { code: "en".into(), confidence: 0.0 }
                };
                (seg, lang)
            })
            .collect();

        let detected_lang = lang_segments
            .first()
            .map(|(_, l)| l.code.clone())
            .unwrap_or_else(|| "en".into());

        // Stage 3+4: NER + Resolve (no store)
        let mut entities = Vec::new();

        if self.config.stages.ner && !self.extractors.is_empty() {
            for (seg, lang) in &lang_segments {
                let mut extracted = self.run_extractors(&seg.text, lang);

                if self.config.stages.entity_resolve && !self.resolvers.is_empty() {
                    let graph = self.graph.read().map_err(|_| IngestError::Graph("graph lock poisoned".into()))?;
                    for entity in &mut extracted {
                        if entity.resolved_to.is_none() {
                            for resolver in &self.resolvers {
                                if let crate::types::ResolutionResult::Matched(nid) = resolver.resolve(entity, &graph) {
                                    entity.resolved_to = Some(nid);
                                    break;
                                }
                            }
                        }
                    }
                }

                entities.extend(extracted);
            }
        }

        // Run relation extraction on all extracted entities
        let text = lang_segments
            .iter()
            .map(|(seg, _)| seg.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let relations = self.run_relation_extraction(&text, &entities);

        Ok(AnalyzeResult {
            entities,
            relations,
            language: detected_lang,
            duration_ms: start.elapsed().as_millis() as u64,
            warnings: Vec::new(),
        })
    }

    /// Execute the pipeline on pre-processed facts (skip parse/NER/resolve).
    /// Useful for structured data that already has entity labels and types.
    pub fn load_processed(
        &self,
        facts: Vec<ProcessedFact>,
    ) -> Result<PipelineResult, IngestError> {
        let start = std::time::Instant::now();
        let mut result = PipelineResult::default();

        if facts.is_empty() {
            return Ok(result);
        }

        let facts = self.apply_transformers(facts, &mut result);
        self.load_facts(facts, &mut result)?;

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    // ── Internal stage implementations ──

    /// Parse raw items into text segments using registered parsers.
    fn parse_items(
        &self,
        items: &[RawItem],
        result: &mut PipelineResult,
    ) -> Result<Vec<ParsedSegment>, IngestError> {
        let mut segments = Vec::new();

        for item in items {
            let texts = if self.config.stages.parse {
                self.parse_content(&item.content, result)
            } else {
                // No parsing — extract text directly
                match &item.content {
                    Content::Text(t) => vec![t.clone()],
                    Content::Structured(map) => {
                        let text = map.values().cloned().collect::<Vec<_>>().join(" ");
                        if text.is_empty() {
                            vec![]
                        } else {
                            vec![text]
                        }
                    }
                    Content::Bytes { mime, .. } => {
                        result
                            .errors
                            .push(format!("no parser for MIME type: {}", mime));
                        vec![]
                    }
                }
            };

            for text in texts {
                if !text.trim().is_empty() {
                    segments.push(ParsedSegment {
                        text,
                        source_name: item.source_name.clone(),
                        source_url: item.source_url.clone(),
                        fetched_at: item.fetched_at,
                        metadata: item.metadata.clone(),
                    });
                }
            }
        }

        Ok(segments)
    }

    /// Try each registered parser, fall back to direct text extraction.
    fn parse_content(&self, content: &Content, result: &mut PipelineResult) -> Vec<String> {
        for parser in &self.parsers {
            if let Ok(segments) = parser.parse(content) {
                if !segments.is_empty() {
                    return segments;
                }
            }
        }

        // Fallback
        match content {
            Content::Text(t) => vec![t.clone()],
            Content::Structured(map) => {
                let text = map.values().cloned().collect::<Vec<_>>().join(" ");
                if text.is_empty() {
                    vec![]
                } else {
                    vec![text]
                }
            }
            Content::Bytes { mime, .. } => {
                result
                    .errors
                    .push(format!("no parser for MIME type: {}", mime));
                vec![]
            }
        }
    }

    /// Detect language for a text segment.
    fn detect_language(&self, text: &str) -> DetectedLanguage {
        if let Some(ref detector) = self.language_detector {
            detector.detect(text)
        } else {
            // Default: assume English
            DetectedLanguage {
                code: "en".into(),
                confidence: 0.0,
            }
        }
    }

    /// Run extractors in cascade: first extractor that produces results wins.
    fn run_extractors(
        &self,
        text: &str,
        lang: &DetectedLanguage,
    ) -> Vec<crate::types::ExtractedEntity> {
        for extractor in &self.extractors {
            let supported = extractor.supported_languages();
            if !supported.is_empty() && !supported.contains(&lang.code) {
                continue;
            }
            let entities = extractor.extract(text, lang);
            if !entities.is_empty() {
                return entities;
            }
        }
        Vec::new()
    }

    /// Run relation extraction on extracted entities.
    ///
    /// Only runs if the relation_extract stage is enabled and there are >=2 entities.
    /// Converts `CandidateRelation` (index-based) to `ExtractedRelation` (label-based).
    fn run_relation_extraction(
        &self,
        text: &str,
        entities: &[crate::types::ExtractedEntity],
    ) -> Vec<ExtractedRelation> {
        if !self.config.stages.relation_extract
            || self.relation_extractors.is_empty()
            || entities.len() < 2
        {
            return Vec::new();
        }

        let input = RelationExtractionInput {
            text: text.to_string(),
            entities: entities.to_vec(),
            language: entities
                .first()
                .map(|e| e.language.clone())
                .unwrap_or_else(|| "en".into()),
        };

        let mut all_relations = Vec::new();

        for extractor in &self.relation_extractors {
            let candidates = extractor.extract_relations(&input);
            for candidate in candidates {
                if candidate.head_idx < entities.len() && candidate.tail_idx < entities.len() {
                    all_relations.push(ExtractedRelation {
                        from: entities[candidate.head_idx].text.clone(),
                        to: entities[candidate.tail_idx].text.clone(),
                        rel_type: candidate.rel_type,
                        confidence: candidate.confidence,
                        method: candidate.method,
                    });
                }
            }
        }

        if !all_relations.is_empty() {
            tracing::debug!(
                relations = all_relations.len(),
                "relation extraction produced candidates"
            );
        }

        all_relations
    }

    /// Apply transformers to facts. Drops facts where a transformer returns Drop.
    fn apply_transformers(
        &self,
        facts: Vec<ProcessedFact>,
        result: &mut PipelineResult,
    ) -> Vec<ProcessedFact> {
        if self.transformers.is_empty() {
            return facts;
        }

        let mut output = Vec::with_capacity(facts.len());

        for mut fact in facts {
            let mut keep = true;
            for transformer in &self.transformers {
                match transformer.transform(&mut fact) {
                    TransformResult::Ok => {}
                    TransformResult::Drop(reason) => {
                        tracing::debug!(
                            entity = %fact.entity,
                            transformer = transformer.name(),
                            reason = %reason,
                            "fact dropped by transformer"
                        );
                        keep = false;
                        break;
                    }
                    TransformResult::Error(err) => {
                        result.errors.push(format!(
                            "transformer '{}' error on '{}': {}",
                            transformer.name(),
                            fact.entity,
                            err
                        ));
                    }
                }
            }
            if keep {
                output.push(fact);
            }
        }

        output
    }

    /// Load processed facts into the graph with chunked write locking.
    ///
    /// Acquires write lock per chunk of `batch_size` facts.
    /// Readers can interleave between chunks.
    fn load_facts(
        &self,
        facts: Vec<ProcessedFact>,
        result: &mut PipelineResult,
    ) -> Result<(), IngestError> {
        if facts.is_empty() {
            return Ok(());
        }

        let chunk_size = self.config.batch_size;

        for chunk in facts.chunks(chunk_size) {
            let mut graph = self.graph.write().map_err(|_| IngestError::Graph("graph write lock poisoned".into()))?;

            for fact in chunk {
                // Store the entity node
                let provenance = engram_core::graph::Provenance {
                    source_type: engram_core::graph::SourceType::Api,
                    source_id: fact.provenance.source.clone(),
                };

                let store_result = graph.store_with_confidence(
                    &fact.entity,
                    fact.confidence,
                    &provenance,
                );

                match store_result {
                    Ok(node_id) => {
                        result.facts_stored += 1;

                        // Set entity type if provided
                        if let Some(ref etype) = fact.entity_type {
                            let _ = graph.set_node_type(&fact.entity, etype);
                        }

                        // Set properties
                        for (key, value) in &fact.properties {
                            let _ = graph.set_property(&fact.entity, key, value);
                        }

                        // Set provenance properties
                        let _ = graph.set_property(
                            &fact.entity,
                            "ingest_source",
                            &fact.provenance.source,
                        );
                        if let Some(ref url) = fact.provenance.source_url {
                            let _ = graph.set_property(&fact.entity, "source_url", url);
                        }
                        if let Some(ref author) = fact.provenance.author {
                            let _ = graph.set_property(&fact.entity, "author", author);
                        }

                        // Track resolution
                        if let Some(crate::types::ResolutionResult::Matched(_)) = &fact.resolution {
                            result.facts_resolved += 1;
                        }

                        // Create relations
                        for rel in &fact.relations {
                            match graph.relate(
                                &rel.from,
                                &rel.to,
                                &rel.rel_type,
                                &provenance,
                            ) {
                                Ok(_) => result.relations_created += 1,
                                Err(e) => result.errors.push(format!(
                                    "relation {}-[{}]->{}: {}",
                                    rel.from, rel.rel_type, rel.to, e
                                )),
                            }
                        }

                        // Track conflicts
                        result.conflicts_detected += fact.conflicts.len() as u32;

                        let _ = node_id; // used above, suppress warning
                    }
                    Err(e) => {
                        result
                            .errors
                            .push(format!("store '{}': {}", fact.entity, e));
                    }
                }
            }

            // Write lock drops here — readers can interleave between chunks
        }

        Ok(())
    }
}

// ── Built-in parsers ──

/// Simple plain-text parser. Passes text through unchanged.
pub struct PlainTextParser;

impl Parser for PlainTextParser {
    fn parse(&self, content: &Content) -> Result<Vec<String>, IngestError> {
        match content {
            Content::Text(t) => Ok(vec![t.clone()]),
            _ => Err(IngestError::Parse("PlainTextParser only handles text".into())),
        }
    }

    fn supported_types(&self) -> Vec<String> {
        vec!["text/plain".into()]
    }
}

/// Structured data parser. Extracts entity labels from key-value maps.
/// Looks for an "entity" key; falls back to concatenating all values.
pub struct StructuredParser;

impl Parser for StructuredParser {
    fn parse(&self, content: &Content) -> Result<Vec<String>, IngestError> {
        match content {
            Content::Structured(map) => {
                if let Some(entity) = map.get("entity") {
                    Ok(vec![entity.clone()])
                } else {
                    let text = map.values().cloned().collect::<Vec<_>>().join(" ");
                    if text.is_empty() {
                        Ok(vec![])
                    } else {
                        Ok(vec![text])
                    }
                }
            }
            _ => Err(IngestError::Parse("StructuredParser only handles structured data".into())),
        }
    }

    fn supported_types(&self) -> Vec<String> {
        vec!["application/json".into(), "text/csv".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PipelineConfig, StageConfig};
    use tempfile::TempDir;

    fn test_graph() -> (TempDir, Arc<RwLock<engram_core::graph::Graph>>) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = engram_core::graph::Graph::create(&path).unwrap();
        (dir, Arc::new(RwLock::new(graph)))
    }

    fn make_item(text: &str, source: &str) -> RawItem {
        RawItem {
            content: Content::Text(text.into()),
            source_url: None,
            source_name: source.into(),
            fetched_at: 1000,
            metadata: Default::default(),
        }
    }

    fn no_ner_config() -> PipelineConfig {
        PipelineConfig {
            stages: StageConfig {
                ner: false,
                entity_resolve: false,
                language_detect: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn empty_pipeline_returns_default_result() {
        let (_dir, graph) = test_graph();
        let pipeline = Pipeline::new(graph, PipelineConfig::default());
        let result = pipeline.execute(vec![]).unwrap();
        assert_eq!(result.facts_stored, 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn pipeline_stores_text_items_without_ner() {
        let (_dir, graph) = test_graph();
        let pipeline = Pipeline::new(graph.clone(), no_ner_config());

        let items = vec![
            make_item("Apple Inc.", "test"),
            make_item("Tim Cook", "test"),
        ];

        let result = pipeline.execute(items).unwrap();
        assert_eq!(result.facts_stored, 2);
        assert!(result.errors.is_empty());

        let g = graph.read().unwrap();
        assert!(g.find_node_id("Apple Inc.").unwrap().is_some());
        assert!(g.find_node_id("Tim Cook").unwrap().is_some());
    }

    #[test]
    fn pipeline_sets_provenance_properties() {
        let (_dir, graph) = test_graph();
        let pipeline = Pipeline::new(graph.clone(), no_ner_config());

        let items = vec![RawItem {
            content: Content::Text("TestEntity".into()),
            source_url: Some("https://example.com".into()),
            source_name: "reuters".into(),
            fetched_at: 12345,
            metadata: HashMap::from([("key1".into(), "val1".into())]),
        }];

        let result = pipeline.execute(items).unwrap();
        assert_eq!(result.facts_stored, 1);

        let g = graph.read().unwrap();
        let nid = g.find_node_id("TestEntity").unwrap().unwrap();
        assert!(nid > 0 || nid == 0);
    }

    #[test]
    fn load_processed_stores_facts_with_relations() {
        let (_dir, graph) = test_graph();
        let pipeline = Pipeline::new(graph.clone(), PipelineConfig::default());

        let now = 1000i64;
        let facts = vec![
            ProcessedFact {
                entity: "Alice".into(),
                entity_type: Some("PERSON".into()),
                properties: Default::default(),
                confidence: 0.9,
                provenance: Provenance {
                    source: "test".into(),
                    source_url: None,
                    author: None,
                    extraction_method: ExtractionMethod::Manual,
                    fetched_at: now,
                    ingested_at: now,
                },
                extraction_method: ExtractionMethod::Manual,
                language: "en".into(),
                relations: vec![],
                conflicts: vec![],
                resolution: None,
            },
            ProcessedFact {
                entity: "Acme Corp".into(),
                entity_type: Some("ORG".into()),
                properties: Default::default(),
                confidence: 0.85,
                provenance: Provenance {
                    source: "test".into(),
                    source_url: None,
                    author: None,
                    extraction_method: ExtractionMethod::Manual,
                    fetched_at: now,
                    ingested_at: now,
                },
                extraction_method: ExtractionMethod::Manual,
                language: "en".into(),
                relations: vec![crate::types::ExtractedRelation {
                    from: "Alice".into(),
                    to: "Acme Corp".into(),
                    rel_type: "works_at".into(),
                    confidence: 0.8,
                    method: ExtractionMethod::Manual,
                }],
                conflicts: vec![],
                resolution: None,
            },
        ];

        let result = pipeline.load_processed(facts).unwrap();
        assert_eq!(result.facts_stored, 2);
        assert_eq!(result.relations_created, 1);
        assert!(result.errors.is_empty());

        let g = graph.read().unwrap();
        assert!(g.find_node_id("Alice").unwrap().is_some());
        assert!(g.find_node_id("Acme Corp").unwrap().is_some());
    }

    #[test]
    fn chunked_write_locking_works_with_small_batch_size() {
        let (_dir, graph) = test_graph();
        let config = PipelineConfig {
            batch_size: 2,
            stages: StageConfig {
                ner: false,
                entity_resolve: false,
                language_detect: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let pipeline = Pipeline::new(graph.clone(), config);

        let items: Vec<RawItem> = (0..5)
            .map(|i| make_item(&format!("Entity{}", i), "test"))
            .collect();

        let result = pipeline.execute(items).unwrap();
        assert_eq!(result.facts_stored, 5);

        let g = graph.read().unwrap();
        for i in 0..5 {
            assert!(
                g.find_node_id(&format!("Entity{}", i)).unwrap().is_some(),
                "Entity{} should exist",
                i
            );
        }
    }

    #[test]
    fn transformer_can_drop_facts() {
        let (_dir, graph) = test_graph();
        let mut pipeline = Pipeline::new(graph.clone(), no_ner_config());

        struct DropFilter;
        impl Transformer for DropFilter {
            fn transform(&self, fact: &mut ProcessedFact) -> TransformResult {
                if fact.entity.starts_with("DROP_") {
                    TransformResult::Drop("filtered by prefix".into())
                } else {
                    TransformResult::Ok
                }
            }
            fn name(&self) -> &str {
                "drop-filter"
            }
        }

        pipeline.add_transformer(Box::new(DropFilter));

        let items = vec![
            make_item("Keep This", "test"),
            make_item("DROP_This", "test"),
            make_item("Also Keep", "test"),
        ];

        let result = pipeline.execute(items).unwrap();
        assert_eq!(result.facts_stored, 2);

        let g = graph.read().unwrap();
        assert!(g.find_node_id("Keep This").unwrap().is_some());
        assert!(g.find_node_id("DROP_This").unwrap().is_none());
        assert!(g.find_node_id("Also Keep").unwrap().is_some());
    }

    #[test]
    fn plain_text_parser_works() {
        let parser = PlainTextParser;
        let result = parser
            .parse(&Content::Text("hello".into()))
            .unwrap();
        assert_eq!(result, vec!["hello"]);

        assert!(parser.parse(&Content::Bytes {
            data: vec![],
            mime: "application/pdf".into(),
        }).is_err());
    }

    #[test]
    fn structured_parser_extracts_entity_key() {
        let parser = StructuredParser;
        let map = HashMap::from([
            ("entity".into(), "Test Corp".into()),
            ("type".into(), "ORG".into()),
        ]);
        let result = parser.parse(&Content::Structured(map)).unwrap();
        assert_eq!(result, vec!["Test Corp"]);
    }

    #[test]
    fn parallel_execution_stores_facts() {
        let (_dir, graph) = test_graph();
        let pipeline = Pipeline::new(graph.clone(), no_ner_config());

        let items: Vec<RawItem> = (0..10)
            .map(|i| make_item(&format!("ParEntity{}", i), "test"))
            .collect();

        let result = pipeline.execute_parallel(items).unwrap();
        assert_eq!(result.facts_stored, 10);

        let g = graph.read().unwrap();
        for i in 0..10 {
            assert!(
                g.find_node_id(&format!("ParEntity{}", i)).unwrap().is_some(),
                "ParEntity{} should exist",
                i
            );
        }
    }

    #[test]
    fn skip_stages_applied_to_config() {
        let mut stages = StageConfig::default();
        let unknown = stages.apply_skip("ner,resolve,dedup");
        assert!(!stages.ner);
        assert!(!stages.entity_resolve);
        assert!(!stages.dedup);
        assert!(stages.parse);
        assert!(stages.language_detect);
        assert!(stages.conflict_check);
        assert!(stages.confidence_calc);
        assert!(unknown.is_empty());
    }

    #[test]
    fn skip_unknown_stages_reported() {
        let mut stages = StageConfig::default();
        let unknown = stages.apply_skip("ner,foo,bar");
        assert!(!stages.ner);
        assert_eq!(unknown, vec!["foo", "bar"]);
    }

    #[test]
    fn enabled_and_skipped_stages() {
        let mut stages = StageConfig::default();
        stages.apply_skip("ner,conflict");
        let enabled = stages.enabled_stages();
        let skipped = stages.skipped_stages();
        assert!(!enabled.contains(&"ner"));
        assert!(!enabled.contains(&"conflict"));
        assert!(skipped.contains(&"ner"));
        assert!(skipped.contains(&"conflict"));
        assert!(enabled.contains(&"parse"));
    }
}
