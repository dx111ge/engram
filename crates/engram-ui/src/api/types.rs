use serde::{Deserialize, Serialize};

// ── Health / Stats ──

#[derive(Clone, Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub nodes: u64,
    #[serde(default)]
    pub edges: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StatsResponse {
    pub nodes: u64,
    pub edges: u64,
    #[serde(default)]
    pub properties: u64,
    #[serde(default)]
    pub types: Vec<String>,
}

// ── Auth ──

#[derive(Clone, Debug, Deserialize)]
pub struct AuthStatusResponse {
    /// Backend returns "setup_required" or "ready"
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub users_count: Option<u32>,
    #[serde(default)]
    pub authenticated: Option<bool>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthLoginResponse {
    pub token: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub trust_level: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthSetupRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserInfo {
    pub username: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub trust_level: Option<f32>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateApiKeyRequest {
    pub label: String,
}

// ── Store / Relate ──

#[derive(Clone, Debug, Serialize)]
pub struct StoreRequest {
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StoreResponse {
    #[serde(default)]
    pub node_id: Option<u64>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
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
    #[serde(rename = "start")]
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct QueryResponse {
    #[serde(default)]
    pub nodes: Vec<NodeHit>,
    #[serde(default)]
    pub edges: Vec<EdgeResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NodeHit {
    #[serde(default)]
    pub node_id: Option<u64>,
    pub label: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub node_type: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub results: Vec<NodeHit>,
    #[serde(default)]
    pub total: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SimilarRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CorrectRequest {
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReinforceResponse {
    #[serde(default)]
    pub entity: Option<String>,
    #[serde(default)]
    pub new_confidence: Option<f32>,
}

// ── Config ──

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigResponse {
    #[serde(flatten)]
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigStatusResponse {
    #[serde(default)]
    pub configured: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub node_count: u64,
    #[serde(default)]
    pub edge_count: u64,
    #[serde(default)]
    pub is_empty_graph: bool,
    #[serde(default)]
    pub wizard_dismissed: bool,
}

// ── Secrets ──

#[derive(Clone, Debug, Deserialize)]
pub struct SecretListItem {
    pub key: String,
}

// ── Assessments ──

#[derive(Clone, Debug, Deserialize)]
pub struct Assessment {
    pub label: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub probability: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub timeframe: Option<String>,
    #[serde(default)]
    pub evidence_count: Option<u32>,
    #[serde(default)]
    pub last_evaluated: Option<String>,
    #[serde(default)]
    pub probability_shift: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssessmentCreate {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watches: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssessmentDetail {
    pub label: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub probability: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub timeframe: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub watches: Vec<String>,
    #[serde(default)]
    pub history: Vec<AssessmentHistory>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Evidence {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub entity: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AddEvidenceRequest {
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssessmentHistory {
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub probability: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

// ── Sources ──

#[derive(Clone, Debug, Deserialize)]
pub struct SourceInfo {
    pub name: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub total_ingested: Option<u64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub error_count: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceCreate {
    pub name: String,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_secret_key: Option<String>,
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
    #[serde(default)]
    pub cpu_cores: Option<u32>,
    #[serde(default)]
    pub has_avx2: Option<bool>,
    #[serde(alias = "has_gpu", default)]
    pub gpu_available: bool,
    #[serde(default)]
    pub gpu_name: Option<String>,
    #[serde(default)]
    pub gpu_backend: Option<String>,
    #[serde(alias = "has_npu", default)]
    pub npu_available: bool,
    #[serde(default)]
    pub npu_name: Option<String>,
    #[serde(default)]
    pub embedder_model: Option<String>,
    #[serde(default)]
    pub embedder_dim: Option<u32>,
    #[serde(default)]
    pub embedder_endpoint: Option<String>,
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

// ── Actions / Rules ──

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub conditions: Option<serde_json::Value>,
    #[serde(default)]
    pub actions: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InferenceRule {
    #[serde(default)]
    pub name: Option<String>,
    pub rule: String,
    #[serde(default)]
    pub description: Option<String>,
}

// ── Ingest ──

#[derive(Clone, Debug, Deserialize)]
pub struct KgeTrainResponse {
    pub status: String,
    pub epochs_completed: u64,
    pub final_loss: f32,
    pub entity_count: u32,
    pub relation_type_count: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyzeEntityItem {
    pub text: String,
    pub entity_type: String,
    pub confidence: f32,
    pub method: String,
    #[serde(default)]
    pub resolved_to: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyzeRelationItem {
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub confidence: f32,
    pub method: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyzeResponse {
    pub entities: Vec<AnalyzeEntityItem>,
    #[serde(default)]
    pub relations: Vec<AnalyzeRelationItem>,
    pub language: String,
    pub duration_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IngestResponse {
    pub facts_stored: u32,
    pub relations_created: u32,
    pub facts_resolved: u32,
    #[serde(default)]
    pub facts_deduped: Option<u32>,
    #[serde(default)]
    pub conflicts_detected: Option<u32>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub duration_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub kb_stats: Option<KbStatsResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct KbStatsResponse {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub entities_linked: u32,
    #[serde(default)]
    pub entities_not_found: u32,
    #[serde(default)]
    pub relations_found: u32,
    #[serde(default)]
    pub errors: u32,
    #[serde(default)]
    pub lookup_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct IngestRequest {
    pub items: Vec<IngestItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IngestItem {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnalyzeRequest {
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct IngestConfigureRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
pub struct AddPeerRequest {
    pub key: String,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MeshAuditEntry {
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub peer: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}

// ── KB Endpoints ──

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KbEndpointInfo {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub entity_link_template: Option<String>,
    #[serde(default)]
    pub relation_query_template: Option<String>,
    #[serde(default)]
    pub max_lookups_per_call: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KbEndpointCreate {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_secret_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_link_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_query_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lookups_per_call: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct KbTestResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

// ── Admin ──

#[derive(Clone, Debug, Deserialize)]
pub struct ResetResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub sidecars_cleaned: Vec<String>,
}

// ── Explain ──

#[derive(Clone, Debug, Deserialize)]
pub struct ExplainResponse {
    #[serde(default)]
    pub entity: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    #[serde(default)]
    pub cooccurrences: Vec<CooccurrenceHit>,
    #[serde(default)]
    pub edges_from: Vec<EdgeResponse>,
    #[serde(default)]
    pub edges_to: Vec<EdgeResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CooccurrenceHit {
    pub entity: String,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub probability: f64,
}

// ── LLM Proxy ──

#[derive(Clone, Debug, Serialize)]
pub struct LlmProxyRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LlmProxyResponse {
    #[serde(default)]
    pub choices: Vec<LlmChoice>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LlmChoice {
    #[serde(default)]
    pub message: Option<LlmResponseMessage>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LlmResponseMessage {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

// ── Delete ──

#[derive(Clone, Debug, Deserialize)]
pub struct DeleteResponse {
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub entity: Option<String>,
}
