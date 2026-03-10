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
pub mod gazetteer;
pub mod lang;
#[cfg(feature = "llm-ner")]
pub mod llm_ner;
pub mod ner_chain;
pub mod pipeline;
pub mod resolver;
pub mod rules;
#[cfg(feature = "spacy")]
pub mod spacy;
pub mod traits;
pub mod types;

// Re-exports for convenience.
pub use confidence::{ConfidenceCalculator, ConfidenceConfig};
pub use conflict::{ConflictConfig, ConflictDetector};
pub use dedup::{ContentDedup, dedup_batch, dedup_by_label};
pub use error::IngestError;
pub use gazetteer::{GazetteerExtractor, GraphGazetteer, GazetteerEntry};
pub use lang::DefaultLanguageDetector;
#[cfg(feature = "lang-detect")]
pub use lang::WhatlangDetector;
pub use pipeline::{Pipeline, PlainTextParser, StructuredParser};
pub use ner_chain::{ChainStrategy, NerChain};
pub use resolver::{ConservativeResolver, ResolverConfig};
pub use rules::RuleBasedNer;
pub use traits::{
    CostModel, Extractor, LanguageDetector, Parser, Resolver, Source, SourceCapabilities,
    SourceParams, Transformer,
};
pub use types::{
    Content, ConflictRecord, DetectedLanguage, ExtractedEntity, ExtractedRelation,
    ExtractionMethod, PipelineConfig, PipelineResult, ProcessedFact, Provenance, RawItem,
    ResolutionResult, StageConfig, TransformResult,
};
