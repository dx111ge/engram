/// Core types for the ingest pipeline.

use std::collections::HashMap;

// ── Raw input types ──

/// Content variants that can enter the pipeline.
#[derive(Debug, Clone)]
pub enum Content {
    /// Plain text.
    Text(String),
    /// Structured key-value data (e.g. from CSV, JSON).
    Structured(HashMap<String, String>),
    /// Raw bytes with MIME type (e.g. PDF, HTML).
    Bytes { data: Vec<u8>, mime: String },
}

/// A raw item from a source before any processing.
#[derive(Debug, Clone)]
pub struct RawItem {
    pub content: Content,
    pub source_url: Option<String>,
    pub source_name: String,
    pub fetched_at: i64,
    pub metadata: HashMap<String, String>,
}

// ── Language detection ──

/// Result of language detection for a text segment.
#[derive(Debug, Clone)]
pub struct DetectedLanguage {
    /// ISO 639-1 code (e.g. "en", "de", "zh").
    pub code: String,
    /// Detection confidence [0.0, 1.0].
    pub confidence: f32,
}

// ── NER / extraction types ──

/// How an entity was extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionMethod {
    /// Dictionary lookup from graph-derived gazetteer.
    Gazetteer,
    /// Regex or pattern-based rule.
    RuleBased,
    /// Graph co-occurrence derived pattern.
    LearnedPattern,
    /// Statistical NER model (ONNX, SpaCy, etc.).
    StatisticalModel,
    /// LLM fallback (lowest confidence, clearly marked).
    LlmFallback,
    /// Manually provided (structured input, no NER needed).
    Manual,
}

/// An entity extracted from text by NER or other means.
#[derive(Debug, Clone)]
pub struct ExtractedEntity {
    /// Surface form as it appears in source text.
    pub text: String,
    /// Entity type label (PERSON, ORG, LOC, DATE, etc.).
    pub entity_type: String,
    /// Character offsets in source text (start, end).
    pub span: (usize, usize),
    /// Extraction confidence [0.0, 1.0].
    pub confidence: f32,
    /// How this entity was extracted.
    pub method: ExtractionMethod,
    /// Language of the surface form.
    pub language: String,
    /// Graph node_id if already resolved during extraction (e.g. gazetteer).
    pub resolved_to: Option<u64>,
}

/// A relation extracted between two entities.
#[derive(Debug, Clone)]
pub struct ExtractedRelation {
    /// Source entity surface form.
    pub from: String,
    /// Target entity surface form.
    pub to: String,
    /// Relationship type.
    pub rel_type: String,
    /// Extraction confidence.
    pub confidence: f32,
    /// How this relation was extracted.
    pub method: ExtractionMethod,
}

// ── Entity resolution types ──

/// Result of resolving an extracted entity against the existing graph.
#[derive(Debug, Clone)]
pub enum ResolutionResult {
    /// High-confidence match to existing node.
    Matched(u64),
    /// No match found — create new entity.
    New,
    /// Borderline — candidates exist but confidence insufficient.
    /// Vec of (node_id, match_score).
    Ambiguous(Vec<(u64, f32)>),
}

// ── Conflict types ──

/// A detected conflict between an incoming fact and existing graph data.
#[derive(Debug, Clone)]
pub struct ConflictRecord {
    /// Existing node id that conflicts.
    pub existing_node: u64,
    /// Description of the conflict.
    pub description: String,
    /// Severity [0.0, 1.0].
    pub severity: f32,
}

// ── Provenance ──

/// Provenance tracking for ingested facts.
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Source name (e.g. "reuters", "mesh:peer_abc").
    pub source: String,
    /// Source URL if applicable.
    pub source_url: Option<String>,
    /// Author identifier if known (e.g. "twitter:@handle").
    pub author: Option<String>,
    /// Extraction method used.
    pub extraction_method: ExtractionMethod,
    /// Timestamp when the fact was fetched.
    pub fetched_at: i64,
    /// Timestamp when the fact was ingested into the graph.
    pub ingested_at: i64,
}

// ── Processed fact (pipeline output) ──

/// A fully processed fact ready for graph insertion.
#[derive(Debug, Clone)]
pub struct ProcessedFact {
    /// Entity label for the primary node.
    pub entity: String,
    /// Entity type (PERSON, ORG, LOC, etc.).
    pub entity_type: Option<String>,
    /// Properties to set on the node.
    pub properties: HashMap<String, String>,
    /// Calculated confidence [0.0, 1.0].
    pub confidence: f32,
    /// Provenance chain.
    pub provenance: Provenance,
    /// How the entity was extracted.
    pub extraction_method: ExtractionMethod,
    /// Language of the source text.
    pub language: String,
    /// Relations to create alongside this entity.
    pub relations: Vec<ExtractedRelation>,
    /// Conflicts detected during pipeline processing.
    pub conflicts: Vec<ConflictRecord>,
    /// Resolution result from entity resolution stage.
    pub resolution: Option<ResolutionResult>,
}

// ── Pipeline configuration ──

/// Which stages to enable in the pipeline.
#[derive(Debug, Clone)]
pub struct StageConfig {
    pub parse: bool,
    pub language_detect: bool,
    pub ner: bool,
    pub entity_resolve: bool,
    pub dedup: bool,
    pub conflict_check: bool,
    pub confidence_calc: bool,
}

impl Default for StageConfig {
    fn default() -> Self {
        Self {
            parse: true,
            language_detect: true,
            ner: true,
            entity_resolve: true,
            dedup: true,
            conflict_check: true,
            confidence_calc: true,
        }
    }
}

impl StageConfig {
    /// Apply skip directives from a comma-separated string.
    ///
    /// Recognized stage names: `parse`, `lang`, `ner`, `resolve`,
    /// `dedup`, `conflict`, `confidence`. Unknown names are collected
    /// and returned so the caller can warn or reject.
    ///
    /// # Example
    /// ```
    /// # use engram_ingest::types::StageConfig;
    /// let mut stages = StageConfig::default();
    /// let unknown = stages.apply_skip("ner,resolve,dedup");
    /// assert!(!stages.ner);
    /// assert!(!stages.entity_resolve);
    /// assert!(!stages.dedup);
    /// assert!(unknown.is_empty());
    /// ```
    pub fn apply_skip(&mut self, skip: &str) -> Vec<String> {
        let mut unknown = Vec::new();
        for token in skip.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            match token {
                "parse" => self.parse = false,
                "lang" | "language" | "language_detect" => self.language_detect = false,
                "ner" | "extract" => self.ner = false,
                "resolve" | "entity_resolve" => self.entity_resolve = false,
                "dedup" | "deduplicate" => self.dedup = false,
                "conflict" | "conflict_check" => self.conflict_check = false,
                "confidence" | "confidence_calc" => self.confidence_calc = false,
                other => unknown.push(other.to_string()),
            }
        }
        unknown
    }

    /// Return the list of currently enabled stage names.
    pub fn enabled_stages(&self) -> Vec<&'static str> {
        let mut stages = Vec::new();
        if self.parse { stages.push("parse"); }
        if self.language_detect { stages.push("lang"); }
        if self.ner { stages.push("ner"); }
        if self.entity_resolve { stages.push("resolve"); }
        if self.dedup { stages.push("dedup"); }
        if self.conflict_check { stages.push("conflict"); }
        if self.confidence_calc { stages.push("confidence"); }
        stages
    }

    /// Return the list of currently skipped stage names.
    pub fn skipped_stages(&self) -> Vec<&'static str> {
        let mut stages = Vec::new();
        if !self.parse { stages.push("parse"); }
        if !self.language_detect { stages.push("lang"); }
        if !self.ner { stages.push("ner"); }
        if !self.entity_resolve { stages.push("resolve"); }
        if !self.dedup { stages.push("dedup"); }
        if !self.conflict_check { stages.push("conflict"); }
        if !self.confidence_calc { stages.push("confidence"); }
        stages
    }
}

/// Pipeline executor configuration.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Pipeline name for logging/identification.
    pub name: String,
    /// Number of parallel worker threads.
    pub workers: usize,
    /// Facts per write lock acquisition.
    pub batch_size: usize,
    /// Flush batch after this delay (ms) even if not full.
    pub batch_timeout_ms: u64,
    /// Backpressure threshold for bounded channels.
    pub channel_buffer: usize,
    /// Which stages are enabled.
    pub stages: StageConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            workers: 4,
            batch_size: 1000,
            batch_timeout_ms: 100,
            channel_buffer: 10_000,
            stages: StageConfig::default(),
        }
    }
}

// ── Transform result ──

/// Outcome of a transformer stage.
#[derive(Debug, Clone)]
pub enum TransformResult {
    /// Fact was transformed successfully.
    Ok,
    /// Fact should be dropped (filtered out).
    Drop(String),
    /// Transformer encountered an error.
    Error(String),
}

// ── Pipeline result ──

/// Summary of a pipeline execution run.
#[derive(Debug, Clone, Default)]
pub struct PipelineResult {
    /// Facts successfully stored.
    pub facts_stored: u32,
    /// Relations created.
    pub relations_created: u32,
    /// Facts that matched existing entities (upserted/skipped).
    pub facts_resolved: u32,
    /// Facts dropped by dedup.
    pub facts_deduped: u32,
    /// Conflicts detected.
    pub conflicts_detected: u32,
    /// Errors encountered (non-fatal).
    pub errors: Vec<String>,
    /// Processing duration in milliseconds.
    pub duration_ms: u64,
}
