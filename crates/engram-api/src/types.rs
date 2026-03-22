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
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Deserialize)]
pub struct QueryRequest {
    pub start: String,
    pub relationship: Option<String>,
    pub depth: Option<u32>,
    pub min_confidence: Option<f32>,
    /// Traversal direction: "out", "in", or "both" (default: "both")
    pub direction: Option<String>,
    /// Filter results to nodes of this type only
    pub node_type: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
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
    /// Review mode: run pipeline but don't commit. Returns session_id for review.
    #[serde(default)]
    pub review: Option<bool>,
}

/// A single item in an ingest request.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum IngestItem {
    /// Structured content with optional source URL.
    WithUrl {
        content: String,
        source_url: Option<String>,
    },
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

// ── Config / Admin response types ──

#[derive(Serialize)]
pub struct ConfigStatusResponse {
    pub configured: Vec<String>,
    pub missing: Vec<String>,
    pub warnings: Vec<String>,
    pub ready: bool,
    pub node_count: u64,
    pub edge_count: u64,
    pub is_empty_graph: bool,
    pub wizard_dismissed: bool,
}

#[derive(Serialize)]
pub struct KbStatsResponse {
    pub endpoint: String,
    pub entities_linked: u32,
    pub entities_not_found: u32,
    pub relations_found: u32,
    pub errors: u32,
    pub lookup_ms: u64,
}

#[derive(Serialize)]
pub struct ResetResponse {
    pub success: bool,
    pub sidecars_cleaned: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct KbEndpointRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_secret_key: Option<String>,
    #[serde(default)]
    pub entity_link_template: Option<String>,
    #[serde(default)]
    pub relation_query_template: Option<String>,
    #[serde(default)]
    pub max_lookups_per_call: Option<u32>,
}

#[derive(Serialize)]
pub struct KbTestResponse {
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
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

/// A single extracted relation in an analyze response.
#[derive(Serialize)]
pub struct AnalyzeRelationResponse {
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub confidence: f32,
    pub method: String,
}

/// Response for `POST /ingest/analyze`.
#[derive(Serialize)]
pub struct AnalyzeResponse {
    pub entities: Vec<AnalyzeEntityResponse>,
    pub relations: Vec<AnalyzeRelationResponse>,
    pub language: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ── Seed enrichment types ──

/// Request body for `POST /ingest/seed/start`.
#[derive(Deserialize)]
pub struct SeedStartRequest {
    pub text: String,
}

/// Response for `POST /ingest/seed/start`.
#[derive(Serialize)]
pub struct SeedStartResponse {
    pub session_id: String,
    pub entities: Vec<SeedEntityResponse>,
    pub area_of_interest: Option<String>,
}

/// A seed entity in a response.
#[derive(Serialize)]
pub struct SeedEntityResponse {
    pub label: String,
    pub entity_type: String,
    pub confidence: f32,
}

/// Request body for `POST /ingest/seed/confirm-aoi`.
#[derive(Deserialize)]
pub struct SeedConfirmAoiRequest {
    pub session_id: String,
    pub area_of_interest: String,
}

/// Request body for `POST /ingest/seed/confirm-entities`.
#[derive(Deserialize)]
pub struct SeedConfirmEntitiesRequest {
    pub session_id: String,
    pub entities: Vec<SeedConfirmEntity>,
}

/// A confirmed entity in the seed flow.
#[derive(Deserialize)]
pub struct SeedConfirmEntity {
    pub label: String,
    #[serde(default)]
    pub skip: bool,
    pub canonical: Option<String>,
    pub qid: Option<String>,
}

/// Request body for `POST /ingest/seed/commit`.
#[derive(Deserialize)]
pub struct SeedCommitRequest {
    pub session_id: String,
}

/// Response for `POST /ingest/seed/commit`.
#[derive(Serialize)]
pub struct SeedCommitResponse {
    pub facts_stored: u32,
    pub relations_created: u32,
    pub duration_ms: u64,
}

// ── Relation review types ──

/// Request body for `POST /ingest/seed/confirm-relations`.
#[derive(Deserialize)]
pub struct SeedConfirmRelationsRequest {
    pub session_id: String,
    /// Indices of accepted connections.
    pub accepted: Vec<usize>,
    /// Connections with modified rel_type.
    #[serde(default)]
    pub modified: Vec<SeedModifiedRelation>,
    /// Indices of skipped connections.
    #[serde(default)]
    pub skipped: Vec<usize>,
}

#[derive(Deserialize)]
pub struct SeedModifiedRelation {
    pub idx: usize,
    pub new_rel_type: String,
}

/// Request body for `POST /ingest/review/confirm`.
#[derive(Deserialize)]
pub struct IngestReviewConfirmRequest {
    pub session_id: String,
    pub accepted: Vec<usize>,
    #[serde(default)]
    pub modified: Vec<SeedModifiedRelation>,
    #[serde(default)]
    pub skipped: Vec<usize>,
}

/// Response for ingest endpoints.
#[derive(Serialize)]
pub struct IngestResponse {
    pub facts_stored: u32,
    pub relations_created: u32,
    pub relations_deduplicated: u32,
    pub facts_resolved: u32,
    pub facts_deduped: u32,
    pub conflicts_detected: u32,
    pub errors: Vec<String>,
    pub duration_ms: u64,
    pub stages_skipped: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kb_stats: Option<KbStatsResponse>,
}

// ── Edge rename types ──

/// Request body for `PATCH /edge`.
#[derive(Deserialize)]
pub struct RenameEdgeRequest {
    pub from: String,
    pub to: String,
    pub old_rel_type: String,
    pub new_rel_type: String,
    /// Optional: set valid_from date ("YYYY-MM-DD") or null to clear.
    pub valid_from: Option<String>,
    /// Optional: set valid_to date ("YYYY-MM-DD") or null to clear.
    pub valid_to: Option<String>,
}

/// Response for `PATCH /edge`.
#[derive(Serialize)]
pub struct RenameEdgeResponse {
    pub renamed: bool,
    pub from: String,
    pub to: String,
    pub old_rel_type: String,
    pub new_rel_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
}

// ── Edge delete types ──

/// Request body for `POST /edge/delete`.
#[derive(Deserialize)]
pub struct DeleteEdgeRequest {
    pub from: String,
    pub to: String,
    pub rel_type: String,
}

/// Response for `POST /edge/delete`.
#[derive(Serialize)]
pub struct DeleteEdgeResponse {
    pub deleted: bool,
    pub from: String,
    pub to: String,
    pub rel_type: String,
}

// ── Path finding types ──

/// Request body for `POST /paths`.
#[derive(Deserialize)]
pub struct PathsRequest {
    pub from: String,
    pub to: String,
    pub max_depth: Option<u32>,
    pub via: Option<String>,
    pub min_depth: Option<u32>,
    pub skip_types: Option<Vec<String>>,
}

/// Response for `POST /paths`.
#[derive(Serialize)]
pub struct PathsResponse {
    pub paths: Vec<Vec<String>>,
    pub count: usize,
}
