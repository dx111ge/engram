/// Request and response types for the REST API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Requests ──

#[derive(Deserialize)]
pub struct StoreRequest {
    pub entity: String,
    #[serde(rename = "type")]
    pub entity_type: Option<String>,
    pub properties: Option<std::collections::HashMap<String, String>>,
    pub source: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Deserialize)]
pub struct RelateRequest {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: Option<f32>,
}

#[derive(Deserialize)]
pub struct QueryRequest {
    pub start: String,
    pub relationship: Option<String>,
    pub depth: Option<u32>,
    pub min_confidence: Option<f32>,
    /// Traversal direction: "out", "in", or "both" (default: "both")
    pub direction: Option<String>,
}

#[derive(Deserialize)]
pub struct SimilarRequest {
    pub text: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct ReinforceRequest {
    pub entity: String,
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct CorrectRequest {
    pub entity: String,
    pub reason: String,
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct DeriveRequest {
    pub rules: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct PropertyRequest {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct BatchRequest {
    pub entities: Option<Vec<StoreRequest>>,
    pub relations: Option<Vec<RelateRequest>>,
    pub source: Option<String>,
    /// Upsert mode: "insert" (default, dedup by label) or "upsert"
    pub mode: Option<BatchMode>,
    /// How to handle confidence on upsert: "max", "replace", "average"
    pub confidence_strategy: Option<ConfidenceStrategy>,
}

/// Batch operation mode.
#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum BatchMode {
    /// Default: store if new, return existing ID if duplicate.
    #[default]
    Insert,
    /// Store if new, update confidence if exists.
    Upsert,
}

/// How to resolve confidence conflicts during upsert.
#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceStrategy {
    /// Keep whichever confidence is higher (existing or incoming).
    #[default]
    Max,
    /// Incoming always wins.
    Replace,
    /// New confidence = (existing + incoming) / 2.
    Average,
}

/// A single NDJSON line item for streaming batch.
/// Can be either an entity store or a relation create.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum BatchItem {
    Relation {
        from: String,
        to: String,
        relationship: String,
        confidence: Option<f32>,
        source: Option<String>,
    },
    Entity {
        entity: String,
        #[serde(rename = "type")]
        entity_type: Option<String>,
        properties: Option<std::collections::HashMap<String, String>>,
        confidence: Option<f32>,
        source: Option<String>,
    },
}

// ── Responses ──

#[derive(Serialize)]
pub struct StoreResponse {
    pub node_id: u64,
    pub label: String,
    pub confidence: f32,
}

#[derive(Serialize)]
pub struct RelateResponse {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub edge_slot: u64,
}

#[derive(Serialize)]
pub struct NodeResponse {
    pub node_id: u64,
    pub label: String,
    pub confidence: f32,
    pub properties: std::collections::HashMap<String, String>,
    pub edges_from: Vec<EdgeResponse>,
    pub edges_to: Vec<EdgeResponse>,
}

#[derive(Serialize)]
pub struct EdgeResponse {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f32,
}

#[derive(Serialize)]
pub struct QueryResponse {
    pub nodes: Vec<NodeHit>,
    pub edges: Vec<EdgeResponse>,
}

#[derive(Serialize)]
pub struct NodeHit {
    pub node_id: u64,
    pub label: String,
    pub confidence: f32,
    pub score: Option<f64>,
    pub depth: Option<u32>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<NodeHit>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct ReinforceResponse {
    pub entity: String,
    pub new_confidence: f32,
}

#[derive(Serialize)]
pub struct CorrectResponse {
    pub corrected: String,
    pub propagated_to: Vec<String>,
}

#[derive(Serialize)]
pub struct DeriveResponse {
    pub rules_evaluated: u32,
    pub rules_fired: u32,
    pub edges_created: u32,
    pub flags_raised: u32,
}

#[derive(Serialize)]
pub struct DecayResponse {
    pub nodes_decayed: u32,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub nodes: u64,
    pub edges: u64,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
    pub entity: String,
}

#[derive(Serialize)]
pub struct BatchResponse {
    pub nodes_stored: u32,
    pub edges_created: u32,
    pub nodes_updated: u32,
    pub errors: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize)]
pub struct ExplainResponse {
    pub entity: String,
    pub confidence: f32,
    pub properties: std::collections::HashMap<String, String>,
    pub cooccurrences: Vec<CooccurrenceHit>,
    pub edges_from: Vec<EdgeResponse>,
    pub edges_to: Vec<EdgeResponse>,
}

#[derive(Deserialize)]
pub struct RulesRequest {
    pub rules: Vec<String>,
    pub append: Option<bool>,
}

#[derive(Serialize)]
pub struct RulesResponse {
    pub loaded: u32,
    pub names: Vec<String>,
}

#[derive(Serialize)]
pub struct RulesListResponse {
    pub count: u32,
    pub names: Vec<String>,
}

#[derive(Serialize)]
pub struct CooccurrenceHit {
    pub entity: String,
    pub count: u32,
    pub probability: f32,
}

#[derive(Deserialize)]
pub struct QuantizeRequest {
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct QuantizeResponse {
    pub mode: String,
    pub vector_memory_bytes: u64,
}

#[derive(Serialize)]
pub struct JsonLdExportResponse {
    pub nodes: u32,
    pub edges: u32,
}

#[derive(Deserialize)]
pub struct JsonLdImportRequest {
    pub data: serde_json::Value,
    pub source: Option<String>,
    /// Trust level for the import source (0.0–1.0, default 0.5).
    /// Controls how much overlapping data affects existing confidence.
    pub trust: Option<f32>,
}

#[derive(Serialize)]
pub struct JsonLdImportResponse {
    pub nodes_imported: u32,
    pub edges_imported: u32,
    pub nodes_merged: u32,
    pub edges_merged: u32,
    pub errors: Option<Vec<String>>,
}

// ── Ingest pipeline types (feature-gated) ──

/// Request body for `POST /ingest`.
#[derive(Deserialize)]
pub struct IngestRequest {
    /// Items to ingest (text or structured).
    pub items: Vec<IngestItem>,
    /// Source name for provenance.
    pub source: Option<String>,
    /// Comma-separated stages to skip (e.g. "ner,resolve,dedup").
    pub skip: Option<String>,
    /// Use parallel execution (rayon) for large batches.
    pub parallel: Option<bool>,
}

/// A single item in an ingest request.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum IngestItem {
    /// Structured key-value data.
    Structured(HashMap<String, String>),
    /// Plain text.
    Text(String),
}

/// Request body for `POST /ingest/configure`.
#[derive(Deserialize)]
pub struct IngestConfigureRequest {
    /// Pipeline name.
    pub name: Option<String>,
    /// Batch size for chunked writes.
    pub batch_size: Option<usize>,
    /// Worker thread count.
    pub workers: Option<usize>,
    /// Stages to skip by default.
    pub skip: Option<String>,
}

/// Request body for `POST /ingest/analyze`.
#[derive(Deserialize)]
pub struct AnalyzeRequest {
    /// Text to analyze.
    pub text: String,
}

/// A single extracted entity in an analyze response.
#[derive(Serialize)]
pub struct AnalyzeEntityResponse {
    pub text: String,
    pub entity_type: String,
    pub confidence: f32,
    pub method: String,
    pub span: (usize, usize),
    pub resolved_to: Option<u64>,
}

/// Response for `POST /ingest/analyze`.
#[derive(Serialize)]
pub struct AnalyzeResponse {
    pub entities: Vec<AnalyzeEntityResponse>,
    pub language: String,
    pub duration_ms: u64,
}

/// Response for ingest endpoints.
#[derive(Serialize)]
pub struct IngestResponse {
    pub facts_stored: u32,
    pub relations_created: u32,
    pub facts_resolved: u32,
    pub facts_deduped: u32,
    pub conflicts_detected: u32,
    pub errors: Vec<String>,
    pub duration_ms: u64,
    pub stages_skipped: Vec<String>,
}
