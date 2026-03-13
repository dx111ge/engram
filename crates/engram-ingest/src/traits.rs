/// Pipeline stage traits.
///
/// Each stage is a trait so implementations can be swapped at config level.
/// All traits are `Send + Sync` for multi-threaded pipeline execution.

use crate::types::{
    Content, DetectedLanguage, ExtractedEntity, ExtractionMethod, ProcessedFact, RawItem,
    ResolutionResult, TransformResult,
};

/// Source: produces RawItems from an external data source.
///
/// Sources are async because they typically perform I/O (HTTP, file reads, etc.).
pub trait Source: Send + Sync {
    /// Fetch raw items from this source.
    fn fetch(
        &self,
        params: &SourceParams,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<RawItem>, crate::IngestError>> + Send + '_>,
    >;

    /// Human-readable source name.
    fn name(&self) -> &str;

    /// Capabilities this source supports.
    fn capabilities(&self) -> SourceCapabilities;
}

/// Parameters passed to a source fetch operation.
#[derive(Debug, Clone, Default)]
pub struct SourceParams {
    /// Search query (for search-based sources).
    pub query: Option<String>,
    /// Maximum items to fetch.
    pub limit: Option<usize>,
    /// Temporal cursor for incremental fetches.
    pub since: Option<i64>,
    /// Additional source-specific parameters.
    pub extra: std::collections::HashMap<String, String>,
}

/// What a source can do.
#[derive(Debug, Clone, Default)]
pub struct SourceCapabilities {
    /// Supports temporal cursors for incremental fetch.
    pub temporal_cursor: bool,
    /// Supports search queries.
    pub searchable: bool,
    /// Supports streaming (push-based).
    pub streaming: bool,
    /// Cost model for usage tracking.
    pub cost_model: CostModel,
}

/// Cost model for source usage tracking.
#[derive(Debug, Clone, Default)]
pub enum CostModel {
    /// No cost (local files, open APIs).
    #[default]
    Free,
    /// Per-request pricing.
    PerRequest(f64),
    /// Per-item pricing.
    PerItem(f64),
    /// Monthly quota with overage pricing.
    Quota {
        monthly_limit: u64,
        overage_cost: f64,
    },
}

/// Parser: converts raw content into text suitable for NER.
pub trait Parser: Send + Sync {
    /// Parse content into plain text segments.
    fn parse(&self, content: &Content) -> Result<Vec<String>, crate::IngestError>;

    /// MIME types this parser handles.
    fn supported_types(&self) -> Vec<String>;
}

/// Language detector: identifies language of text segments.
pub trait LanguageDetector: Send + Sync {
    /// Detect language for a text segment.
    fn detect(&self, text: &str) -> DetectedLanguage;
}

/// Extractor (NER backend): analyzes text and produces entities.
pub trait Extractor: Send + Sync {
    /// Extract entities from text given detected language.
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity>;

    /// Human-readable backend name.
    fn name(&self) -> &str;

    /// Extraction method this backend uses.
    fn method(&self) -> ExtractionMethod;

    /// Languages this backend supports. Empty = all.
    fn supported_languages(&self) -> Vec<String>;
}

/// Wrapper: delegates `Extractor` to an `Arc<dyn Extractor>` for caching.
///
/// Allows a single loaded NER backend (e.g. GLiNER) to be shared across
/// multiple pipeline builds without reloading the model each time.
pub struct ArcExtractor(pub std::sync::Arc<dyn Extractor>);

impl Extractor for ArcExtractor {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        self.0.extract(text, lang)
    }
    fn name(&self) -> &str { self.0.name() }
    fn method(&self) -> ExtractionMethod { self.0.method() }
    fn supported_languages(&self) -> Vec<String> { self.0.supported_languages() }
}

/// Wrapper: delegates `RelationExtractor` to an `Arc<dyn RelationExtractor>` for caching.
pub struct ArcRelationExtractor(pub std::sync::Arc<dyn crate::rel_traits::RelationExtractor>);

impl crate::rel_traits::RelationExtractor for ArcRelationExtractor {
    fn extract_relations(&self, input: &crate::rel_traits::RelationExtractionInput) -> Vec<crate::rel_traits::CandidateRelation> {
        self.0.extract_relations(input)
    }
    fn name(&self) -> &str { self.0.name() }
    fn stats(&self) -> Option<crate::types::KbStats> { self.0.stats() }
}

/// Resolver: matches extracted entities against existing graph nodes.
///
/// Runs under a **read lock** — no graph mutations during resolution.
/// Actual node creation happens in the batch write phase.
pub trait Resolver: Send + Sync {
    /// Resolve an extracted entity against the graph.
    fn resolve(
        &self,
        entity: &ExtractedEntity,
        graph: &engram_core::graph::Graph,
    ) -> ResolutionResult;
}

/// Transformer: applies custom transformations to processed facts.
///
/// Runs after resolution and before load. Used for normalization,
/// custom tagging, provenance enrichment, etc.
pub trait Transformer: Send + Sync {
    /// Transform a fact in place. Return `Drop` to filter it out.
    fn transform(&self, fact: &mut ProcessedFact) -> TransformResult;

    /// Human-readable transformer name.
    fn name(&self) -> &str;
}
