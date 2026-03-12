/// Relation extraction traits and types.
///
/// Mirrors the NER architecture: trait-based backends behind a chain orchestrator.
/// Each backend produces `CandidateRelation`s which are merged and deduplicated.

use crate::types::{ExtractedEntity, ExtractionMethod};

/// Input to a relation extractor: text + already-extracted entities.
#[derive(Debug, Clone)]
pub struct RelationExtractionInput {
    /// Source text.
    pub text: String,
    /// Entities extracted by NER (with spans, types, resolution).
    pub entities: Vec<ExtractedEntity>,
    /// Detected language code.
    pub language: String,
}

/// A candidate relation produced by a relation extraction backend.
#[derive(Debug, Clone)]
pub struct CandidateRelation {
    /// Index into `input.entities` for the head entity.
    pub head_idx: usize,
    /// Index into `input.entities` for the tail entity.
    pub tail_idx: usize,
    /// Relation type label (e.g. "works_at", "located_in").
    pub rel_type: String,
    /// Extraction confidence [0.0, 1.0].
    pub confidence: f32,
    /// Which backend produced this relation.
    pub method: ExtractionMethod,
}

/// Trait for relation extraction backends.
///
/// Implementations receive text + extracted entities and produce candidate
/// relations between entity pairs. All implementations must be `Send + Sync`
/// for multi-threaded pipeline execution.
pub trait RelationExtractor: Send + Sync {
    /// Extract relations from the input.
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation>;

    /// Human-readable backend name.
    fn name(&self) -> &str;

    /// Optional stats from the last extraction run.
    fn stats(&self) -> Option<crate::types::KbStats> { None }
}
