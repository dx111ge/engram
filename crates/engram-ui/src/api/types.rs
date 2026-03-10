use serde::{Deserialize, Serialize};

// ── Health / Stats ──

#[derive(Clone, Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub nodes: u64,
    pub edges: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StatsResponse {
    pub nodes: u64,
    pub edges: u64,
    pub properties: u64,
    pub types: Vec<String>,
}

// ── Store / Relate ──

#[derive(Clone, Debug, Serialize)]
pub struct StoreRequest {
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RelateRequest {
    pub from: String,
    pub to: String,
    pub relationship: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

// ── Query / Search ──

#[derive(Clone, Debug, Serialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchResult {
    pub label: String,
    pub score: f64,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
}

// ── Natural Language ──

#[derive(Clone, Debug, Serialize)]
pub struct TellRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AskRequest {
    pub text: String,
}

// ── Node ──

#[derive(Clone, Debug, Deserialize)]
pub struct NodeResponse {
    pub label: String,
    pub confidence: f32,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    #[serde(default)]
    pub edges_from: Vec<EdgeResponse>,
    #[serde(default)]
    pub edges_to: Vec<EdgeResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EdgeResponse {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f32,
}

// ── Learning ──

#[derive(Clone, Debug, Serialize)]
pub struct ReinforceRequest {
    pub entity: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CorrectRequest {
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

// ── Import/Export ──

#[derive(Clone, Debug, Deserialize)]
pub struct JsonLdExport {
    #[serde(rename = "@context")]
    pub context: serde_json::Value,
    #[serde(rename = "@graph")]
    pub graph: Vec<serde_json::Value>,
}

// ── Compute ──

#[derive(Clone, Debug, Deserialize)]
pub struct ComputeResponse {
    pub backend: String,
    pub gpu_available: bool,
    pub npu_available: bool,
}

// ── Graph event (SSE) ──

#[derive(Clone, Debug, Deserialize)]
pub struct GraphEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

// ── Reason / Gaps ──

#[derive(Clone, Debug, Deserialize)]
pub struct BlackArea {
    pub kind: String,
    pub entities: Vec<String>,
    pub severity: f64,
    #[serde(default)]
    pub suggested_queries: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScanReport {
    pub total_gaps: usize,
    pub breakdown: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GapsResponse {
    pub gaps: Vec<BlackArea>,
    pub report: ScanReport,
}

// ── Actions ──

#[derive(Clone, Debug, Deserialize)]
pub struct ActionRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub enabled: bool,
}

// ── Ingest ──

#[derive(Clone, Debug, Deserialize)]
pub struct SourceInfo {
    pub name: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub total_ingested: Option<u64>,
}

// ── Mesh ──

#[derive(Clone, Debug, Deserialize)]
pub struct PeerInfo {
    pub key: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub trust_level: Option<String>,
}
