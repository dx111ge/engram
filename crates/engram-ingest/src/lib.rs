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

#[cfg(feature = "gliner2")]
pub mod gliner2_backend;
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
pub mod rel_chain;
pub mod rel_gazetteer;
pub mod rel_kge;
pub mod rel_knowledge_base;
pub mod rel_traits;
pub mod resolver;
pub mod rules;
pub mod scheduler;
pub mod source;
pub mod subsumption;
#[cfg(feature = "spacy")]
pub mod spacy;
pub mod traits;
pub mod types;

/// Resolve the engram home directory (`~/.engram/`).
///
/// Checks `ENGRAM_HOME` env var first, then falls back to `$HOME/.engram`
/// (or `%USERPROFILE%\.engram` on Windows). Single source of truth — all
/// crates should use this instead of rolling their own home-dir lookup.
pub fn engram_home() -> Option<std::path::PathBuf> {
    std::env::var_os("ENGRAM_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| std::path::PathBuf::from(h).join(".engram"))
        })
}

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
pub use rel_chain::RelationChain;
pub use rel_gazetteer::{RelationGazetteer, RelationGazetteerExtractor, RelGazetteerEntry};
pub use rel_kge::{KgeConfig, KgeModel, KgeRelationExtractor, KgeTrainStats};
pub use rel_knowledge_base::{KbEndpoint, KbRelationExtractor};
pub use rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
pub use resolver::{ConservativeResolver, ResolverConfig};
pub use rules::RuleBasedNer;
pub use scheduler::{AdaptiveScheduler, SchedulerConfig, SourceSchedule};
pub use source::{SourceRegistry, SourceInfo, SourceUsage, UsageSnapshot};
pub use traits::{
    ArcExtractor, ArcRelationExtractor,
    CostModel, Extractor, LanguageDetector, Parser, Resolver, Source, SourceCapabilities,
    SourceParams, Transformer,
};
pub use types::{
    AnalyzeResult, Content, ConflictRecord, DetectedLanguage, ExtractedEntity, ExtractedRelation,
    ExtractionMethod, PipelineConfig, PipelineResult, ProcessedFact, Provenance, RawItem,
    ResolutionResult, StageConfig, TransformResult,
};
