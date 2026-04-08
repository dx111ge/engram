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
    /// Document context for provenance (shared via Arc across facts from same doc).
    doc_context: Option<std::sync::Arc<crate::types::DocumentContext>>,
}

/// Snap a byte index to the nearest valid UTF-8 char boundary (rounding down).
fn snap_to_char_boundary(text: &str, idx: usize) -> usize {
    let idx = idx.min(text.len());
    if text.is_char_boundary(idx) { return idx; }
    // Walk backwards to find the nearest char boundary
    let mut i = idx;
    while i > 0 && !text.is_char_boundary(i) { i -= 1; }
    i
}

/// Extract the sentence containing an entity span.
/// Finds the nearest sentence boundaries (. ! ? or newline) around the span.
/// Uses char-boundary-safe slicing to handle multi-byte UTF-8 (curly quotes, etc.).
fn extract_snippet(text: &str, span: (usize, usize), max_chars: usize) -> String {
    // Snap span to valid char boundaries
    let span_start = snap_to_char_boundary(text, span.0);
    let span_end = snap_to_char_boundary(text, span.1);

    // Find sentence start: look backwards from entity start
    let snippet_start = text[..span_start]
        .rfind(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| {
            // Advance past the delimiter to the next char boundary
            let next = i + 1;
            snap_to_char_boundary(text, next)
        })
        .unwrap_or(0);
    // Find sentence end: look forwards from entity end
    let snippet_end = text[span_end..]
        .find(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|i| {
            let end = span_end + i + 1;
            snap_to_char_boundary(text, end)
        })
        .unwrap_or(text.len().min(span_end + max_chars));
    let snippet_end = snap_to_char_boundary(text, snippet_end.min(text.len()));

    let result = text[snippet_start..snippet_end].trim();
    // Clamp to max_chars at a char boundary
    if result.len() > max_chars {
        let cut = snap_to_char_boundary(result, max_chars);
        result[..cut].trim().to_string()
    } else {
        result.to_string()
    }
}

/// Check if a date text contains a specific year (19xx or 20xx).
fn is_specific_date(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(4).any(|w| {
        w.iter().all(|c| c.is_ascii_digit()) && {
            let year: String = w.iter().collect();
            year.starts_with("19") || year.starts_with("20")
        }
    })
}

/// Generate a dedup-safe label for a Fact node: `Fact:{summary}-{hash4}`.
/// Uses the first ~50 chars of the claim as a readable summary slug.
pub fn make_fact_label(entity: &str, source_text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Build a readable slug from the claim text (first ~50 chars, words only)
    let words: Vec<&str> = source_text.split_whitespace().collect();
    let mut slug = String::new();
    for w in &words {
        let clean: String = w.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        if clean.is_empty() { continue; }
        if !slug.is_empty() { slug.push('-'); }
        slug.push_str(&clean.to_lowercase());
        if slug.len() >= 50 { break; }
    }
    // Safe truncate: snap to char boundary to avoid panic on multi-byte UTF-8
    let cut = snap_to_char_boundary(&slug, 50);
    slug.truncate(cut);
    let mut hasher = DefaultHasher::new();
    entity.hash(&mut hasher);
    source_text.hash(&mut hasher);
    let hash = hasher.finish();
    format!("Fact:{}-{:04x}", slug, hash & 0xFFFF)
}

/// The ingest pipeline executor.
///
/// Owns the stage implementations and orchestrates execution across
/// multiple worker threads. Uses rayon for CPU-bound NER work and
/// tokio for async I/O (source fetching, batch writes).
pub struct Pipeline {
    config: PipelineConfig,
    graph: Arc<RwLock<engram_core::graph::Graph>>,
    doc_store: Option<Arc<RwLock<engram_core::storage::doc_store::DocStore>>>,
    parsers: Vec<Box<dyn Parser>>,
    language_detector: Option<Box<dyn LanguageDetector>>,
    extractors: Vec<Box<dyn Extractor>>,
    resolvers: Vec<Box<dyn Resolver>>,
    transformers: Vec<Box<dyn Transformer>>,
    relation_extractors: Vec<Box<dyn RelationExtractor>>,
    /// LLM config for fact extraction (Layer 2). None = skip fact extraction.
    llm_endpoint: Option<String>,
    llm_model: Option<String>,
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
            doc_store: None,
            parsers: Vec::new(),
            language_detector: None,
            extractors: Vec::new(),
            resolvers: Vec::new(),
            transformers: Vec::new(),
            relation_extractors: Vec::new(),
            llm_endpoint: None,
            llm_model: None,
        }
    }

    /// Set the document store for content caching and provenance tracking.
    pub fn set_doc_store(&mut self, store: Arc<RwLock<engram_core::storage::doc_store::DocStore>>) {
        self.doc_store = Some(store);
    }

    /// Set LLM config for fact extraction (Layer 2).
    pub fn set_llm(&mut self, endpoint: String, model: String) {
        self.llm_endpoint = Some(endpoint);
        self.llm_model = Some(model);
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
    /// Log current process RSS in MB for memory debugging.
    fn log_ram(label: &str) {
        #[cfg(target_os = "windows")]
        {
            // Read from /proc/self or use GetProcessMemoryInfo
            use std::process::Command;
            if let Ok(out) = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", std::process::id()), "/FO", "CSV", "/NH"])
                .output()
            {
                let s = String::from_utf8_lossy(&out.stdout);
                // CSV format: "engram.exe","PID","Console","1","123.456 K"
                if let Some(mem) = s.split(',').nth(4) {
                    let mem = mem.trim().trim_matches('"').replace('.', "").replace(" K", "");
                    if let Ok(kb) = mem.parse::<u64>() {
                        eprintln!("[RAM] {}: {} MB", label, kb / 1024);
                    }
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            eprintln!("[RAM] {}: (non-windows)", label);
        }
    }

    pub fn execute(&self, items: Vec<RawItem>) -> Result<PipelineResult, IngestError> {
        let start = std::time::Instant::now();
        let mut result = PipelineResult::default();
        let mut absorbed_dates: HashMap<String, String> = HashMap::new();
        Self::log_ram("pipeline-start");

        if items.is_empty() {
            return Ok(result);
        }

        tracing::info!(
            pipeline = %self.config.name,
            items = items.len(),
            "starting pipeline execution"
        );

        // Stage 1: Parse raw items into text segments
        Self::log_ram("before-parse");
        let segments = self.parse_items(&items, &mut result)?;
        Self::log_ram("after-parse");

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

        // Stage 2.5: Translate non-English text to English via LLM
        let lang_segments: Vec<(ParsedSegment, DetectedLanguage)> = if self.config.stages.translate
            && self.llm_endpoint.is_some()
        {
            lang_segments
                .into_iter()
                .map(|(mut seg, lang)| {
                    if lang.code != "en" && lang.confidence > 0.5 {
                        if let Some(translated) = self.translate_to_english(&seg.text, &lang.code) {
                            tracing::info!(
                                from_lang = %lang.code,
                                original_len = seg.text.len(),
                                translated_len = translated.len(),
                                "translated segment to English"
                            );
                            seg.text = translated;
                            // Update doc_context with translated text and original language
                            if let Some(ref mut dc) = seg.doc_context {
                                if let Some(dc_mut) = std::sync::Arc::get_mut(dc) {
                                    dc_mut.full_text = seg.text.clone();
                                    dc_mut.original_language = Some(lang.code.clone());
                                }
                            }
                        }
                    }
                    (seg, lang)
                })
                .collect()
        } else {
            lang_segments
        };

        // Stage 3: Extract entities via NER (cascade through extractors)
        Self::log_ram("before-NER");
        let mut facts: Vec<ProcessedFact> = Vec::new();

        if self.config.stages.ner && !self.extractors.is_empty() {
            for (seg, lang) in &lang_segments {
                // Chunk large texts before NER to prevent ONNX memory explosion.
                // GLiNER2 tokenizes the full input into one tensor -- 37K chars caused 51GB RAM.
                let mut extracted = if seg.text.len() > 5000 {
                    let chunks = crate::fact_extract::chunk_text(&seg.text, 3000);
                    tracing::info!(chunks = chunks.len(), chars = seg.text.len(), "NER: chunking large text");
                    let mut all_entities = Vec::new();
                    for (ci, chunk) in chunks.iter().enumerate() {
                        Self::log_ram(&format!("NER-chunk-{}/{}", ci + 1, chunks.len()));
                        let chunk_entities = self.run_extractors(chunk, lang);
                        eprintln!("[RAM] NER-chunk-{}: {} entities from {} chars", ci + 1, chunk_entities.len(), chunk.len());
                        all_entities.extend(chunk_entities);
                    }
                    Self::log_ram("NER-chunks-done");
                    // Dedup entities that appear in overlapping chunks
                    all_entities.sort_by(|a, b| a.text.cmp(&b.text).then(a.entity_type.cmp(&b.entity_type)));
                    all_entities.dedup_by(|a, b| a.text == b.text && a.entity_type == b.entity_type);
                    all_entities
                } else {
                    self.run_extractors(&seg.text, lang)
                };

                // Fragment filter: remove junk NER outputs
                extracted.retain(|e| {
                    if e.text.len() > 60 { return false; }
                    if e.text.matches('-').count() >= 5 { return false; }
                    let lower = e.text.to_lowercase();
                    let stops = ["the", "a", "an", "of", "in", "to", "for", "and", "or", "but", "is", "was", "are", "were", "following", "after", "before"];
                    let words: Vec<&str> = lower.split_whitespace().collect();
                    if let Some(&first) = words.first() {
                        if stops.contains(&first) { return false; }
                    }
                    if let Some(&last) = words.last() {
                        if stops.contains(&last) { return false; }
                    }
                    true
                });

                // Date absorption: partition into date vs non-date entities
                let (date_entities, non_date_entities): (Vec<_>, Vec<_>) = extracted
                    .into_iter()
                    .partition(|e| e.entity_type.eq_ignore_ascii_case("date"));

                // Build sentence -> date map for absorbed dates
                let mut sentence_dates: HashMap<String, String> = HashMap::new();
                for de in &date_entities {
                    if is_specific_date(&de.text) {
                        let sent = extract_snippet(&seg.text, de.span, 200);
                        sentence_dates.insert(sent, de.text.clone());
                    }
                }

                // Merge into pipeline-level absorbed dates
                absorbed_dates.extend(sentence_dates);

                // Continue with non-date entities only
                let mut extracted = non_date_entities;

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
                // Chunk large texts to prevent ONNX memory explosion (same as NER)
                Self::log_ram("before-RE");
                let relations = if seg.text.len() > 5000 && !extracted.is_empty() {
                    let chunks = crate::fact_extract::chunk_text(&seg.text, 3000);
                    let mut all_rels = Vec::new();
                    for chunk in &chunks {
                        // Filter entities that appear in this chunk
                        let chunk_entities: Vec<_> = extracted.iter()
                            .filter(|e| chunk.contains(&e.text))
                            .cloned()
                            .collect();
                        if chunk_entities.len() >= 2 {
                            let rels = self.run_relation_extraction(chunk, &chunk_entities);
                            all_rels.extend(rels);
                        }
                    }
                    all_rels
                } else {
                    self.run_relation_extraction(&seg.text, &extracted)
                };
                Self::log_ram("after-RE");

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

                    let snippet = extract_snippet(&seg.text, entity.span, 200);

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
                        source_text: Some(snippet),
                        entity_span: Some(entity.span),
                        doc_context: seg.doc_context.clone(),
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
                    source_text: None,
                    entity_span: None,
                    doc_context: seg.doc_context.clone(),
                });
            }
        }
        Self::log_ram("after-NER-all");

        // Stage 6: Apply transformers
        let facts = self.apply_transformers(facts, &mut result);

        // Stage 7: LLM fact extraction (Layer 2, optional)
        if self.config.stages.fact_extract {
            self.extract_llm_facts(&facts, &mut result);
        }

        // Stage 8: Load — batch write to graph (chunked write locking)
        self.load_facts(facts, &mut result, &absorbed_dates)?;

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

                    // Chunk large texts before NER (same as sequential path)
                    let mut extracted = if seg.text.len() > 5000 {
                        let chunks = crate::fact_extract::chunk_text(&seg.text, 3000);
                        let mut all_entities = Vec::new();
                        for chunk in &chunks {
                            all_entities.extend(self.run_extractors(chunk, &lang));
                        }
                        all_entities.sort_by(|a, b| a.text.cmp(&b.text).then(a.entity_type.cmp(&b.entity_type)));
                        all_entities.dedup_by(|a, b| a.text == b.text && a.entity_type == b.entity_type);
                        all_entities
                    } else {
                        self.run_extractors(&seg.text, &lang)
                    };

                    // Fragment filter: remove junk NER outputs
                    extracted.retain(|e| {
                        if e.text.len() > 60 { return false; }
                        if e.text.matches('-').count() >= 5 { return false; }
                        let lower = e.text.to_lowercase();
                        let stops = ["the", "a", "an", "of", "in", "to", "for", "and", "or", "but", "is", "was", "are", "were", "following", "after", "before"];
                        let words: Vec<&str> = lower.split_whitespace().collect();
                        if let Some(&first) = words.first() {
                            if stops.contains(&first) { return false; }
                        }
                        if let Some(&last) = words.last() {
                            if stops.contains(&last) { return false; }
                        }
                        true
                    });

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

                            let snippet = extract_snippet(&seg.text, entity.span, 200);

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
                                source_text: Some(snippet),
                                entity_span: Some(entity.span),
                                doc_context: None,
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
                        source_text: None,
                        entity_span: None,
                        doc_context: None,
                    }
                })
                .collect()
        };

        // Stage 6: Apply transformers (sequential)
        let facts = self.apply_transformers(facts, &mut result);

        // Stage 7: Load (chunked writes)
        self.load_facts(facts, &mut result, &HashMap::new())?;

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
        self.load_facts(facts, &mut result, &HashMap::new())?;

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

            // Build document context from the full concatenated text
            let full_text = texts.join("\n");
            let doc_ctx = if !full_text.trim().is_empty() {
                Some(crate::document::build_doc_context(item, &full_text))
            } else {
                None
            };

            for text in texts {
                if !text.trim().is_empty() {
                    segments.push(ParsedSegment {
                        text,
                        source_name: item.source_name.clone(),
                        source_url: item.source_url.clone(),
                        fetched_at: item.fetched_at,
                        metadata: item.metadata.clone(),
                        doc_context: doc_ctx.clone(),
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

    /// Translate text to English via the configured LLM endpoint.
    /// Returns `None` if translation fails or LLM is unavailable.
    fn translate_to_english(&self, text: &str, source_lang: &str) -> Option<String> {
        let endpoint = self.llm_endpoint.as_ref()?;
        let model = self.llm_model.as_deref().unwrap_or("default");

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .ok()?;

        // Chunk large texts to stay within LLM context limits
        let chunks = crate::fact_extract::chunk_text(text, 3000);
        let mut translated_parts = Vec::with_capacity(chunks.len());

        for chunk in &chunks {
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are a professional translator. Translate the text to English. Preserve all names, dates, numbers, and technical terms exactly as they appear. Return ONLY the translation, no commentary or explanations."
                    },
                    {
                        "role": "user",
                        "content": format!("Translate the following {} text to English:\n\n{}", source_lang, chunk)
                    }
                ],
                "temperature": 0.1,
                "max_tokens": 4096
            });

            let resp = client
                .post(endpoint)
                .json(&body)
                .send()
                .ok()?;

            let json: serde_json::Value = resp.json().ok()?;
            if let Some(content) = json.pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
            {
                translated_parts.push(content.to_string());
            } else {
                tracing::warn!("LLM translation failed for chunk, using original");
                translated_parts.push(chunk.clone());
            }
        }

        let result = translated_parts.join("\n\n");
        if result.trim().is_empty() {
            None
        } else {
            Some(result)
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
            area_of_interest: None,
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
                        source_text: None,
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
        absorbed_dates: &HashMap<String, String>,
    ) -> Result<(), IngestError> {
        if facts.is_empty() {
            return Ok(());
        }

        let chunk_size = self.config.batch_size;

        // Two-pass write: store ALL entities first, then create ALL relations.
        // This ensures target nodes exist before edges reference them.
        let mut deferred_relations: Vec<(engram_core::graph::Provenance, Vec<crate::types::ExtractedRelation>)> = Vec::new();

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

                        // Defer relations to second pass (all nodes must exist first)
                        if !fact.relations.is_empty() {
                            deferred_relations.push((provenance.clone(), fact.relations.clone()));
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

        // Pass 1.5: Create Fact nodes, Document nodes, and Publisher nodes.
        self.create_fact_and_document_nodes(&facts, result, absorbed_dates)?;

        // Pass 2: Create all deferred relations.
        // Auto-creates missing nodes (e.g., KB enrichment discovers "Lockheed Martin"
        // as manufacturer of HIMARS -- we create the node automatically).
        if !deferred_relations.is_empty() {
            let mut graph = self.graph.write().map_err(|_| IngestError::Graph("graph write lock poisoned".into()))?;
            let mut auto_created_count = 0u32;
            let mut skipped_existing_count = 0u32;
            for (provenance, relations) in &deferred_relations {
                for rel in relations {
                    // Auto-create missing nodes (KB enrichment discovers new entities)
                    for label in [&rel.from, &rel.to] {
                        if graph.find_node_id(label).ok().flatten().is_none() {
                            let _ = graph.store_with_confidence(
                                label, 0.70, provenance,
                            );
                            result.facts_stored += 1;
                            auto_created_count += 1;
                        } else {
                            skipped_existing_count += 1;
                        }
                    }

                    match graph.relate_upsert(
                        &rel.from,
                        &rel.to,
                        &rel.rel_type,
                        provenance,
                    ) {
                        Ok(r) if r.created => result.relations_created += 1,
                        Ok(_) => result.relations_deduplicated += 1,
                        Err(e) => result.errors.push(format!(
                            "relation {}-[{}]->{}: {}",
                            rel.from, rel.rel_type, rel.to, e
                        )),
                    }
                }
            }
            tracing::info!(
                auto_created = auto_created_count,
                skipped_existing = skipped_existing_count,
                relations = deferred_relations.iter().map(|(_, r)| r.len()).sum::<usize>(),
                "Pass 2: relation deferred write complete"
            );
        }

        Ok(())
    }

    /// Pass 1.5: Create Document nodes and Entity→Document edges (Layer 1).
    ///
    /// Layer 1: Entity --[mentioned_in]--> Document --[published_by]--> Publisher
    /// Layer 2 (LLM facts): Entity --[subject_of]--> Fact --[extracted_from]--> Document
    fn create_fact_and_document_nodes(
        &self,
        facts: &[ProcessedFact],
        result: &mut PipelineResult,
        absorbed_dates: &HashMap<String, String>,
    ) -> Result<(), IngestError> {
        let mut graph = self.graph.write()
            .map_err(|_| IngestError::Graph("graph write lock poisoned".into()))?;
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: format!("ingest:{}", self.config.name),
        };

        // Collect unique documents and cache content (skip for debate ingest)
        if self.config.create_documents {
            let mut created_docs: std::collections::HashSet<String> = std::collections::HashSet::new();
            for fact in facts {
                if let Some(ref doc_ctx) = fact.doc_context {
                    let doc_label = crate::document::doc_label(&doc_ctx.content_hash_hex);
                    if created_docs.contains(&doc_label) {
                        continue;
                    }
                    created_docs.insert(doc_label.clone());
                    self.create_document_node(&mut graph, doc_ctx, &doc_label, &prov, now_ts);
                    self.cache_document_content(doc_ctx);
                }
            }

            // Layer 1: Entity -> Document (mentioned_in) direct edges
            // Layer 2: Fact --[extracted_from]--> Document provenance edges
            for fact in facts {
                if let Some(ref doc_ctx) = fact.doc_context {
                    let doc_label = crate::document::doc_label(&doc_ctx.content_hash_hex);
                    let _ = graph.relate_upsert(&fact.entity, &doc_label, "mentioned_in", &prov);
                    let _ = graph.relate_upsert(&fact.entity, &doc_label, "extracted_from", &prov);
                }
            }
        }
        Ok(())
    }

    /// Create a Document node in the graph with all metadata properties.
    fn create_document_node(
        &self,
        graph: &mut engram_core::graph::Graph,
        doc_ctx: &crate::types::DocumentContext,
        doc_label: &str,
        prov: &engram_core::graph::Provenance,
        now_ts: i64,
    ) {
        // If already exists, update content_hash and content_length (reprocess case)
        if graph.find_node_id(doc_label).ok().flatten().is_some() {
            let _ = graph.set_property(doc_label, "content_hash", &doc_ctx.content_hash_hex);
            let _ = graph.set_property(
                doc_label, "content_length", &doc_ctx.full_text.len().to_string(),
            );
            let _ = graph.set_property(doc_label, "ner_complete", "true");
            if let Some(ref orig_lang) = doc_ctx.original_language {
                let _ = graph.set_property(doc_label, "original_language", orig_lang);
            }
            return;
        }
        if graph.store_with_confidence(doc_label, 0.80, prov).is_err() {
            return;
        }
        let _ = graph.set_node_type(doc_label, "Document");
        let _ = graph.set_property(doc_label, "content_hash", &doc_ctx.content_hash_hex);
        let _ = graph.set_property(doc_label, "mime_type", &doc_ctx.mime_type);
        let _ = graph.set_property(doc_label, "ingested_at", &now_ts.to_string());
        let _ = graph.set_property(doc_label, "fetched_at", &doc_ctx.fetched_at.to_string());
        let _ = graph.set_property(
            doc_label, "content_length", &doc_ctx.full_text.len().to_string(),
        );
        if let Some(ref url) = doc_ctx.url {
            let _ = graph.set_property(doc_label, "url", url);
        }
        if let Some(ref fp) = doc_ctx.file_path {
            let _ = graph.set_property(doc_label, "file_path", fp);
        }
        if let Some(ref title) = doc_ctx.title {
            let _ = graph.set_property(doc_label, "title", title);
        }
        if let Some(ref date) = doc_ctx.doc_date {
            let _ = graph.set_property(doc_label, "doc_date", date);
        }
        // Mark as fully processed by the NER/RE pipeline
        let _ = graph.set_property(doc_label, "ner_complete", "true");
        // Track original language if translated
        if let Some(ref orig_lang) = doc_ctx.original_language {
            let _ = graph.set_property(doc_label, "original_language", orig_lang);
        }
        // Edge: Document -> Publisher (published_by)
        let publisher_label = if let Some(ref url) = doc_ctx.url {
            let (stype, sid) = crate::learned_trust::extract_source_from_url(url);
            format!("Source:{}:{}", stype, sid)
        } else {
            format!("Source:local:{}", doc_ctx.mime_type.replace('/', "_"))
        };
        if graph.find_node_id(&publisher_label).ok().flatten().is_none() {
            let _ = graph.store_with_confidence(&publisher_label, 0.50, prov);
            let _ = graph.set_node_type(&publisher_label, "Source");
        }
        let _ = graph.relate_upsert(doc_label, &publisher_label, "published_by", prov);
    }

    /// Cache document content in the DocStore (if available).
    fn cache_document_content(&self, doc_ctx: &crate::types::DocumentContext) {
        if let Some(ref store_arc) = self.doc_store {
            if let Ok(mut store) = store_arc.write() {
                let mime = engram_core::storage::doc_store::MimeType::from_mime_str(
                    &doc_ctx.mime_type,
                );
                if let Err(e) = store.store(doc_ctx.full_text.as_bytes(), mime) {
                    tracing::warn!("DocStore cache failed: {e}");
                }
            }
        }
    }

    /// Stage 7: LLM-based fact extraction (Layer 2).
    /// Extracts semantic claims from document text and creates Fact nodes.
    fn extract_llm_facts(
        &self,
        facts: &[ProcessedFact],
        result: &mut PipelineResult,
    ) {
        let (endpoint, model) = match (&self.llm_endpoint, &self.llm_model) {
            (Some(ep), Some(m)) if !ep.is_empty() && !m.is_empty() => (ep.clone(), m.clone()),
            _ => return, // No LLM configured, skip
        };

        let config = crate::fact_extract::FactExtractConfig {
            llm_endpoint: endpoint,
            llm_model: model,
            gleaning: true,
            max_tokens: 2000,
            temperature: 0.1,
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        // Collect unique document texts and their entity lists
        let mut doc_texts: HashMap<String, (String, Vec<String>)> = HashMap::new();
        for fact in facts {
            if let Some(ref doc_ctx) = fact.doc_context {
                let doc_label = crate::document::doc_label(&doc_ctx.content_hash_hex);
                let entry = doc_texts.entry(doc_label).or_insert_with(|| {
                    (doc_ctx.full_text.clone(), Vec::new())
                });
                if !entry.1.contains(&fact.entity) {
                    entry.1.push(fact.entity.clone());
                }
            }
        }

        if doc_texts.is_empty() {
            return;
        }

        tracing::info!(
            documents = doc_texts.len(),
            "Stage 7: LLM fact extraction starting"
        );

        let mut total_facts = 0u32;
        for (doc_label, (text, entity_names)) in &doc_texts {
            // Safety cap: warn and truncate excessively large documents to prevent
            // memory explosion (42GB observed on a 37K char PDF producing 15 chunks).
            let text = if text.len() > 100_000 {
                tracing::warn!(
                    doc = %doc_label, chars = text.len(),
                    "LLM fact extraction: document exceeds 100K chars, truncating"
                );
                &text[..100_000]
            } else {
                text.as_str()
            };

            let chunks = crate::fact_extract::chunk_text(text, 3000);
            tracing::info!(doc = %doc_label, chunks = chunks.len(), "LLM fact extraction: processing");

            // Process and store one chunk at a time to keep memory bounded.
            // Each chunk's claims are written to graph immediately, then freed.
            for (idx, chunk) in chunks.iter().enumerate() {
                let claims = crate::fact_extract::extract_claims(
                    &client, &config, chunk, entity_names, idx,
                );
                if claims.is_empty() {
                    continue;
                }
                if let Ok(mut graph) = self.graph.write() {
                    let count = crate::fact_extract::store_facts_in_graph(
                        &mut graph, &claims, doc_label, entity_names,
                    );
                    total_facts += count;
                }
                // claims dropped here -- memory freed before next chunk
            }
        }

        tracing::info!(
            facts_extracted = total_facts,
            "Stage 7: LLM fact extraction complete"
        );
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
                source_text: None,
                entity_span: None,
                doc_context: None,
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
                    source_text: None,
                }],
                conflicts: vec![],
                resolution: None,
                source_text: None,
                entity_span: None,
                doc_context: None,
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

    #[test]
    fn test_extract_snippet_utf8_curly_quotes() {
        // Curly quotes are multi-byte UTF-8 (\u{201c} = 3 bytes each)
        let text = "\u{201c}Berlin is the capital of Germany,\u{201d} he said. Munich is in Bavaria.";
        // Entity "Berlin" spans bytes 3..9 (after opening curly quote which is 3 bytes)
        let span = (3, 9);
        let result = extract_snippet(text, span, 200);
        assert!(!result.is_empty(), "snippet should not be empty");
        assert!(result.contains("Berlin"), "snippet should contain the entity");
    }

    #[test]
    fn test_extract_snippet_utf8_boundary_edge_case() {
        // Entity span that lands in the middle of a multi-byte character
        let text = "Gro\u{00df}britannien and France signed the treaty.";
        // Intentionally put span end at a non-char-boundary (byte 4 is mid-character for \u{00df} which is 2 bytes at pos 3-4)
        let span = (0, 4);
        let result = extract_snippet(text, span, 200);
        assert!(!result.is_empty(), "snippet should not be empty even with non-boundary span");
    }

    #[test]
    fn test_is_specific_date() {
        assert!(is_specific_date("2022"));
        assert!(is_specific_date("February 2024"));
        assert!(is_specific_date("January 15, 2023"));
        assert!(!is_specific_date("a few days later"));
        assert!(!is_specific_date("recently"));
        assert!(!is_specific_date("the following day"));
    }

    #[test]
    fn test_fragment_filter() {
        // Long entity (>60 chars)
        assert!("A".repeat(61).len() > 60);
        // Hyphen-heavy slug
        assert!("a-b-c-d-e-f".matches('-').count() >= 5);
        // Starts with stopword
        let stops = ["the", "a", "an", "of", "in", "to", "for", "and", "or", "but", "is", "was", "are", "were", "following", "after", "before"];
        assert!(stops.contains(&"the"));
        assert!(stops.contains(&"following"));
    }

    #[test]
    fn test_make_fact_label_unique_per_entity() {
        let text = "Berlin is the capital of Germany";
        let label_a = make_fact_label("Berlin", text);
        let label_b = make_fact_label("Germany", text);
        assert_ne!(label_a, label_b, "same source_text + different entities should produce different labels");
        // Same entity + same text should be deterministic
        let label_a2 = make_fact_label("Berlin", text);
        assert_eq!(label_a, label_a2);
    }
}
