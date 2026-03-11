/// # engram-ingest
///
/// Multi-stage ingest pipeline for engram. Transforms raw data from external
/// sources into graph-ready facts through NER, entity resolution, dedup,
/// conflict detection, and confidence calculation.
///
/// ## Architecture
///
/// The pipeline is a chain of stages, each implemented as a trait:
///
/// ```text
/// [Source] -> [Parser] -> [LanguageDetector] -> [Extractor (NER)]
///     -> [Resolver] -> [Dedup] -> [Conflict] -> [Confidence]
///     -> [Transformer] -> [Load]
/// ```
///
/// Every stage is optional and skippable. Workers run in parallel
/// (rayon for CPU-bound NER, tokio for async I/O). Writes are batched
/// and chunked to keep reads alive during large imports.

pub mod anno_backend;
pub mod confidence;
pub mod conflict;
pub mod dedup;
pub mod error;
pub mod file_source;
pub mod gazetteer;
pub mod ledger;
pub mod lang;
pub mod learned_patterns;
pub mod learned_trust;
#[cfg(feature = "llm-ner")]
pub mod llm_ner;
pub mod mesh_fast_path;
pub mod ner_chain;
pub mod pipeline;
pub mod resolver;
pub mod rules;
pub mod scheduler;
pub mod source;
pub mod subsumption;
#[cfg(feature = "spacy")]
pub mod spacy;
pub mod traits;
pub mod types;

// Re-exports for convenience.
pub use confidence::{ConfidenceCalculator, ConfidenceConfig};
pub use conflict::{ConflictConfig, ConflictDetector};
pub use dedup::{ContentDedup, dedup_batch, dedup_by_label};
pub use error::IngestError;
pub use ledger::SearchLedger;
pub use file_source::{FileSource, FileSourceConfig, PollWatcher};
pub use gazetteer::{GazetteerExtractor, GraphGazetteer, GazetteerEntry};
pub use lang::DefaultLanguageDetector;
#[cfg(feature = "lang-detect")]
pub use lang::WhatlangDetector;
pub use pipeline::{Pipeline, PlainTextParser, StructuredParser};
pub use ner_chain::{ChainStrategy, NerChain};
pub use resolver::{ConservativeResolver, ResolverConfig};
pub use rules::RuleBasedNer;
pub use scheduler::{AdaptiveScheduler, SchedulerConfig, SourceSchedule};
pub use source::{SourceRegistry, SourceInfo, SourceUsage, UsageSnapshot};
pub use traits::{
    CostModel, Extractor, LanguageDetector, Parser, Resolver, Source, SourceCapabilities,
    SourceParams, Transformer,
};
pub use types::{
    AnalyzeResult, Content, ConflictRecord, DetectedLanguage, ExtractedEntity, ExtractedRelation,
    ExtractionMethod, PipelineConfig, PipelineResult, ProcessedFact, Provenance, RawItem,
    ResolutionResult, StageConfig, TransformResult,
};
