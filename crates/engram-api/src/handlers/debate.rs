/// Multi-agent debate panel: auto-generated personas with diverse rigor, bias,
/// source access, and cognitive styles debate a topic using engram's knowledge graph.
///
/// Flow: POST /debate/start -> review/edit agents -> POST /debate/{id}/run -> SSE stream
///       -> optional inject/stop -> POST /debate/{id}/synthesize -> 3-layer synthesis.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;
use tokio::sync::Notify;

use crate::state::AppState;
use super::{api_err, ApiResult};

// ── Data types ──────────────────────────────────────────────────────────

/// We need a wrapper so DebateSession can derive Clone without requiring Instant: Default.
#[derive(Clone, Debug)]
pub struct DebateSession {
    pub session_id: String,
    pub topic: String,
    pub status: DebateStatus,
    pub agents: Vec<DebateAgent>,
    pub rounds: Vec<DebateRound>,
    pub current_round: usize,
    pub max_rounds: usize,
    pub synthesis: Option<Synthesis>,
    pub created_at: std::time::Instant,
    pub notify: Arc<Notify>,
    /// Pending user injection for next round.
    pub pending_injection: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateStatus {
    GeneratingPanel,
    AwaitingStart,
    Running,
    AwaitingInput,
    /// All rounds finished, only synthesis remains.
    AllRoundsComplete,
    Synthesizing,
    Complete,
    Error,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebateAgent {
    pub id: String,
    pub name: String,
    pub persona_description: String,
    pub rigor_level: f32,
    pub source_access: SourceAccess,
    pub evidence_threshold: f32,
    pub cognitive_style: CognitiveStyle,
    pub bias: AgentBias,
    pub icon: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAccess {
    GraphOnly,
    GraphAndWeb,
    GraphLowConfidence,
    WebOnly,
}

impl std::fmt::Display for SourceAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphOnly => write!(f, "Engram graph only"),
            Self::GraphAndWeb => write!(f, "Engram graph + web search"),
            Self::GraphLowConfidence => write!(f, "Engram graph (including low-confidence)"),
            Self::WebOnly => write!(f, "External web sources only"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitiveStyle {
    PatternSeeking,
    Skeptical,
    Contrarian,
    DevilsAdvocate,
    Empirical,
    Intuitive,
    Systemic,
}

impl std::fmt::Display for CognitiveStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PatternSeeking => write!(f, "Pattern-seeking: finds connections others miss"),
            Self::Skeptical => write!(f, "Skeptical: demands proof, questions assumptions"),
            Self::Contrarian => write!(f, "Contrarian: argues the opposing view"),
            Self::DevilsAdvocate => write!(f, "Devil's advocate: tests arguments by attacking them"),
            Self::Empirical => write!(f, "Empirical: data-driven, statistical thinking"),
            Self::Intuitive => write!(f, "Intuitive: follows hunches and weak signals"),
            Self::Systemic => write!(f, "Systemic: second-order effects, systems thinking"),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentBias {
    pub label: String,
    pub description: String,
    pub is_neutral: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebateRound {
    pub round_number: usize,
    pub turns: Vec<DebateTurn>,
    pub user_injection: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebateTurn {
    pub agent_id: String,
    pub position: String,
    pub evidence: Vec<TurnEvidence>,
    pub confidence: f32,
    pub tools_used: Vec<ToolInvocation>,
    pub agrees_with: Vec<String>,
    pub disagrees_with: Vec<String>,
    /// What changed this agent's mind since last round (empty in round 1).
    #[serde(default)]
    pub position_shift: String,
    /// Specific concessions made to other agents' arguments.
    #[serde(default)]
    pub concessions: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TurnEvidence {
    pub entity: String,
    pub confidence: f32,
    pub source: String,
    pub supporting: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ToolInvocation {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result_summary: String,
}

// ── Synthesis types (3-layer output) ────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Synthesis {
    // Layer 1: Evidence-based conclusion
    pub evidence_conclusion: String,
    pub conclusion_confidence: f32,
    pub evidence_gaps: Vec<String>,
    pub key_evidence: Vec<EvidenceSummary>,

    // Layer 2: Influence map
    pub influence_map: Vec<AgentInfluence>,
    pub unexpected_alignments: Vec<Alignment>,
    pub cherry_picks: Vec<CherryPick>,

    // Layer 3: Hidden agendas
    pub hidden_agendas: Vec<HiddenAgenda>,
    pub beneficiary_map: Vec<Beneficiary>,
    pub parallel_interests: Vec<ParallelInterest>,
    pub blind_spots: Vec<BlindSpot>,

    // Layer 4: Evolution analysis
    #[serde(default)]
    pub evolution: Vec<AgentEvolution>,

    // Shared
    pub areas_of_agreement: Vec<AgreementPoint>,
    pub areas_of_disagreement: Vec<DisagreementPoint>,
    pub key_tensions: Vec<String>,
    pub recommended_investigations: Vec<String>,
    pub agent_positions: Vec<AgentPosition>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentEvolution {
    pub agent_id: String,
    pub agent_name: String,
    /// Confidence values per round (trajectory).
    pub confidence_trajectory: Vec<f32>,
    /// Net shift from first to last round.
    pub net_shift: f32,
    /// What caused the biggest shift.
    pub pivot_cause: String,
    /// Did they dig in (negative = hardened) or open up (positive = softened)?
    pub flexibility_score: f32,
    /// Key concessions made across all rounds.
    pub key_concessions: Vec<String>,
    /// Whether bias overrode evidence at any point.
    pub bias_override: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EvidenceSummary {
    pub entity: String,
    pub fact: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentInfluence {
    pub agent_id: String,
    pub agent_name: String,
    pub bias_label: String,
    pub position_pushed: String,
    pub evidence_backed: bool,
    pub distortion_summary: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Alignment {
    pub agents: Vec<String>,
    pub common_position: String,
    pub reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CherryPick {
    pub agent_id: String,
    pub ignored_evidence: String,
    pub why_ignored: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HiddenAgenda {
    pub agent_id: String,
    pub agent_name: String,
    pub stated_position: String,
    pub underlying_interest: String,
    pub who_benefits: String,
    pub what_they_avoid: String,
    pub what_they_lose: String,
    pub second_order_effects: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Beneficiary {
    pub position: String,
    pub beneficiaries: Vec<String>,
    pub mechanism: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ParallelInterest {
    pub agents: Vec<String>,
    pub surface_disagreement: String,
    pub hidden_alignment: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlindSpot {
    pub agent_id: String,
    pub topic_avoided: String,
    pub likely_reason: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgreementPoint {
    pub statement: String,
    pub agents: Vec<String>,
    pub confidence: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DisagreementPoint {
    pub statement: String,
    pub positions: Vec<(String, String)>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentPosition {
    pub agent_id: String,
    pub agent_name: String,
    pub final_position: String,
    pub confidence: f32,
    pub evidence_count: usize,
}

// ── Request/response types ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct StartRequest {
    pub topic: String,
    #[serde(default = "default_agent_count")]
    pub agent_count: u8,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u8,
}

fn default_agent_count() -> u8 { 5 }
fn default_max_rounds() -> u8 { 3 }

#[derive(serde::Deserialize)]
pub struct InjectRequest {
    pub message: String,
}

#[derive(serde::Deserialize)]
pub struct AgentEdit {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub persona_description: Option<String>,
    #[serde(default)]
    pub rigor_level: Option<f32>,
    #[serde(default)]
    pub source_access: Option<SourceAccess>,
    #[serde(default)]
    pub evidence_threshold: Option<f32>,
    #[serde(default)]
    pub cognitive_style: Option<CognitiveStyle>,
    #[serde(default)]
    pub bias: Option<AgentBias>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct AgentEdits {
    pub agents: Vec<AgentEdit>,
}

// ── Persona auto-generation ─────────────────────────────────────────────

const COGNITIVE_STYLES: &[CognitiveStyle] = &[
    CognitiveStyle::PatternSeeking,
    CognitiveStyle::Skeptical,
    CognitiveStyle::Contrarian,
    CognitiveStyle::DevilsAdvocate,
    CognitiveStyle::Empirical,
    CognitiveStyle::Intuitive,
    CognitiveStyle::Systemic,
];

const SOURCE_OPTIONS: &[SourceAccess] = &[
    SourceAccess::GraphOnly,
    SourceAccess::GraphAndWeb,
    SourceAccess::GraphLowConfidence,
    SourceAccess::WebOnly,
];

const COLORS: &[&str] = &[
    "#e74c3c", "#3498db", "#2ecc71", "#f39c12", "#9b59b6",
    "#1abc9c", "#e67e22", "#34495e",
];

const ICONS: &[&str] = &[
    "fa-user-secret", "fa-microscope", "fa-hat-wizard", "fa-balance-scale",
    "fa-flask", "fa-brain", "fa-network-wired", "fa-eye",
];

/// Deterministic slot assignment for agent diversity.
pub fn assign_agent_slots(count: u8) -> Vec<DebateAgent> {
    let n = count.max(2).min(8) as usize;
    let mut agents = Vec::with_capacity(n);

    for i in 0..n {
        let rigor = if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
        let evidence_threshold = rigor * 0.5 + 0.1;
        let source = SOURCE_OPTIONS[i % SOURCE_OPTIONS.len()].clone();
        let style = COGNITIVE_STYLES[i % COGNITIVE_STYLES.len()].clone();
        // First and last agents are neutral; middle ones get biases (filled by LLM)
        let is_neutral = i == 0 || (n > 2 && i == n - 1);

        agents.push(DebateAgent {
            id: format!("agent-{}", i + 1),
            name: String::new(),
            persona_description: String::new(),
            rigor_level: rigor,
            source_access: source,
            evidence_threshold,
            cognitive_style: style,
            bias: AgentBias {
                label: if is_neutral { "Neutral analyst".into() } else { String::new() },
                description: if is_neutral { "Follow evidence wherever it leads".into() } else { String::new() },
                is_neutral,
            },
            icon: ICONS[i % ICONS.len()].into(),
            color: COLORS[i % COLORS.len()].into(),
        });
    }
    agents
}

/// Build the LLM prompt to generate persona details + biases for the assigned slots.
fn build_persona_generation_prompt(topic: &str, agents: &[DebateAgent]) -> serde_json::Value {
    let mut agent_descriptions = String::new();
    for a in agents {
        agent_descriptions.push_str(&format!(
            "Agent {}: rigor={:.1} ({}), source_access={}, cognitive_style={}, neutral={}\n",
            a.id,
            a.rigor_level,
            if a.rigor_level < 0.3 { "conspiracy-leaning, low standards" }
            else if a.rigor_level < 0.7 { "moderate rigor" }
            else { "strict evidence-only" },
            a.source_access,
            a.cognitive_style,
            a.bias.is_neutral,
        ));
    }

    let system_prompt = format!(
        r##"You are generating a diverse panel of {} intelligence analysts for a structured debate.
Each analyst has pre-assigned characteristics (rigor level, source access, cognitive style).
Your job: generate a name, background, bias assignment, and visual identity for each.

Topic under debate: "{}"

Pre-assigned slots:
{}

RULES:
- Agents marked neutral=true MUST have bias_label "Neutral analyst" and bias_description "Follow evidence wherever it leads"
- Agents marked neutral=false MUST have a specific bias/agenda relevant to the topic
  The bias represents what stakeholder group they fight for (e.g., "Industry lobby", "Civil liberties advocate", "Military-industrial complex")
- Low-rigor agents (rigor < 0.3) should have colorful, extreme personas (conspiracy theorist, social media "researcher", tabloid journalist)
- High-rigor agents (rigor > 0.7) should be serious professionals (forensic analyst, academic researcher, veteran intelligence officer)
- icon must be a Font Awesome class (fa-user-secret, fa-microscope, fa-hat-wizard, fa-balance-scale, fa-flask, fa-brain, fa-network-wired, fa-eye, fa-shield-alt, fa-search, fa-gavel, fa-globe-americas, fa-chess)
- color must be a distinct CSS hex color

Return ONLY a JSON array (no markdown, no commentary):
[
  {{
    "id": "agent-1",
    "name": "Dr. Example",
    "persona_description": "1-2 sentences about background and argumentative style",
    "bias_label": "Neutral analyst or specific bias",
    "bias_description": "what they fight for",
    "icon": "fa-xxx",
    "color": "#hexcolor"
  }}
]"##,
        agents.len(), topic, agent_descriptions
    );

    serde_json::json!({
        "messages": [
            { "role": "system", "content": system_prompt }
        ],
        "temperature": 0.9,
        "max_tokens": 2048
    })
}

/// Call the configured LLM to generate persona details.
async fn call_llm(state: &AppState, request_body: serde_json::Value) -> Result<serde_json::Value, String> {
    let (endpoint, api_key, default_model) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        let model = cfg.llm_model.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_MODEL").ok());
        (ep, key, model)
    };

    let endpoint = endpoint.ok_or("LLM not configured")?;
    let model = request_body.get("model")
        .and_then(|m| m.as_str())
        .map(String::from)
        .or(default_model)
        .unwrap_or_else(|| "llama3.2".into());

    let messages = request_body.get("messages").cloned().unwrap_or(serde_json::json!([]));
    let temperature = request_body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(0.7);
    let max_tokens = request_body.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(2048);
    let tools = request_body.get("tools").cloned();

    let url = super::admin::normalize_llm_endpoint(&endpoint);

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });
    if let Some(tools_val) = tools {
        body["tools"] = tools_val;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.post(&url).header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req.json(&body).send().await.map_err(|e| format!("LLM request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("LLM returned {status}: {text}"));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON from LLM: {e}"))
}

/// Extract text content from an LLM chat completion response.
fn extract_content(response: &serde_json::Value) -> Option<String> {
    response.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(String::from)
}

/// Extract tool_calls from an LLM response (for function calling loop).
fn extract_tool_calls(response: &serde_json::Value) -> Vec<serde_json::Value> {
    response.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Parse JSON from LLM content (handles markdown code fences).
pub fn parse_json_from_llm(content: &str) -> Result<serde_json::Value, String> {
    // Try direct parse first
    if let Ok(v) = serde_json::from_str(content) {
        return Ok(v);
    }
    // Try extracting from ```json ... ``` block
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            if start < end {
                let slice = &content[start..=end];
                if let Ok(v) = serde_json::from_str(slice) {
                    return Ok(v);
                }
            }
        }
    }
    // Try object
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if start < end {
                let slice = &content[start..=end];
                if let Ok(v) = serde_json::from_str(slice) {
                    return Ok(v);
                }
            }
        }
    }
    Err(format!("Could not parse JSON from LLM response: {}", &content[..content.len().min(200)]))
}

// ── Tool definitions for agents ─────────────────────────────────────────

/// Build the tool definitions available to a debate agent based on source access.
pub fn tools_for_agent(agent: &DebateAgent) -> serde_json::Value {
    let mut tools = vec![
        tool_def("engram_search", "Search the knowledge graph for entities and relationships matching a query", serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 10)" }
            },
            "required": ["query"]
        })),
        tool_def("engram_query", "Traverse the graph from a start entity to find connected entities and relationships", serde_json::json!({
            "type": "object",
            "properties": {
                "start": { "type": "string", "description": "Starting entity label" },
                "depth": { "type": "integer", "description": "Traversal depth (default 2)" }
            },
            "required": ["start"]
        })),
        tool_def("engram_explain", "Get detailed explanation of an entity including provenance and confidence", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Entity label to explain" }
            },
            "required": ["entity"]
        })),
        tool_def("engram_what_if", "Simulate confidence cascade: what happens if an entity's confidence changes", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Entity to simulate" },
                "new_confidence": { "type": "number", "description": "Hypothetical new confidence (0-1)" }
            },
            "required": ["entity", "new_confidence"]
        })),
        tool_def("engram_contradictions", "Find contradicting facts in the knowledge graph", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Optional entity to focus on" }
            }
        })),
    ];

    // Add web search for GraphAndWeb and WebOnly agents
    if matches!(agent.source_access, SourceAccess::GraphAndWeb | SourceAccess::WebOnly) {
        tools.push(tool_def("engram_investigate", "Search the web for information on a topic and optionally ingest results", serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Web search query" }
            },
            "required": ["query"]
        })));
    }

    serde_json::json!(tools)
}

fn tool_def(name: &str, description: &str, parameters: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

// ── Server-side tool execution ──────────────────────────────────────────

/// Execute a tool call against the local graph (server-side, no HTTP round-trip).
/// Uses the label-based Graph API (search_text, edges_from, node_confidence, etc.).
async fn execute_tool(state: &AppState, tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    match tool_name {
        "engram_search" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            match g.search_text(query, limit) {
                Ok(results) => {
                    let items: Vec<serde_json::Value> = results.iter().map(|r| {
                        serde_json::json!({
                            "label": r.label,
                            "confidence": r.confidence,
                            "score": r.score
                        })
                    }).collect();
                    serde_json::json!({"results": items})
                }
                Err(e) => serde_json::json!({"error": e.to_string()}),
            }
        }
        "engram_query" => {
            let start = args.get("start").and_then(|s| s.as_str()).unwrap_or("");
            let depth = args.get("depth").and_then(|d| d.as_u64()).unwrap_or(2) as usize;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            // BFS traversal using label-based API
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            let mut results = Vec::new();
            queue.push_back((start.to_string(), 0usize));
            visited.insert(start.to_string());
            while let Some((label, d)) = queue.pop_front() {
                if d > depth { continue; }
                let conf = g.node_confidence(&label).unwrap_or(None).unwrap_or(0.0);
                let nt = g.get_node_type(&label).unwrap_or_else(|| "entity".into());
                let edges = g.edges_from(&label).unwrap_or_default();
                let mut edge_items = Vec::new();
                for e in edges.iter().take(20) {
                    if !visited.contains(&e.to) && d + 1 <= depth {
                        visited.insert(e.to.clone());
                        queue.push_back((e.to.clone(), d + 1));
                    }
                    edge_items.push(serde_json::json!({
                        "rel": e.relationship,
                        "target": e.to,
                        "confidence": e.confidence
                    }));
                }
                results.push(serde_json::json!({
                    "label": label,
                    "type": nt,
                    "confidence": conf,
                    "depth": d,
                    "edges": edge_items
                }));
            }
            serde_json::json!({"traversal": results})
        }
        "engram_explain" => {
            let entity = args.get("entity").and_then(|e| e.as_str()).unwrap_or("");
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            let conf = g.node_confidence(entity).unwrap_or(None);
            match conf {
                Some(c) => {
                    let nt = g.get_node_type(entity).unwrap_or_else(|| "entity".into());
                    let edges = g.edges_from(entity).unwrap_or_default();
                    let edge_items: Vec<serde_json::Value> = edges.iter().take(20).map(|e| {
                        serde_json::json!({
                            "rel": e.relationship,
                            "target": e.to,
                            "confidence": e.confidence
                        })
                    }).collect();
                    serde_json::json!({
                        "label": entity,
                        "type": nt,
                        "confidence": c,
                        "edge_count": edges.len(),
                        "edges": edge_items
                    })
                }
                None => serde_json::json!({"error": format!("entity '{}' not found", entity)}),
            }
        }
        "engram_what_if" => {
            let entity = args.get("entity").and_then(|e| e.as_str()).unwrap_or("");
            let new_conf = args.get("new_confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            match g.node_confidence(entity).unwrap_or(None) {
                Some(current_conf) => {
                    let delta = new_conf - current_conf;
                    let edges = g.edges_from(entity).unwrap_or_default();
                    let affected: Vec<serde_json::Value> = edges.iter().take(30).filter_map(|e| {
                        let neighbor_conf = g.node_confidence(&e.to).unwrap_or(None).unwrap_or(0.5);
                        let propagated = delta * e.confidence * 0.5;
                        Some(serde_json::json!({
                            "entity": e.to,
                            "current_confidence": neighbor_conf,
                            "impact": propagated,
                            "simulated": (neighbor_conf + propagated).clamp(0.05, 0.95)
                        }))
                    }).collect();
                    serde_json::json!({
                        "entity": entity,
                        "current_confidence": current_conf,
                        "simulated_confidence": new_conf,
                        "affected": affected
                    })
                }
                None => serde_json::json!({"error": format!("entity '{}' not found", entity)}),
            }
        }
        "engram_contradictions" => {
            let entity = args.get("entity").and_then(|e| e.as_str());
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            let mut contradictions = Vec::new();
            if let Some(label) = entity {
                let edges = g.edges_from(label).unwrap_or_default();
                for e in edges.iter().take(50) {
                    if e.confidence < 0.3 {
                        contradictions.push(serde_json::json!({
                            "from": label,
                            "rel": e.relationship,
                            "to": e.to,
                            "confidence": e.confidence,
                            "issue": "low confidence suggests disputed or uncertain"
                        }));
                    }
                }
            }
            serde_json::json!({"contradictions": contradictions})
        }
        "engram_investigate" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            serde_json::json!({
                "note": "Web search capability. Results would come from configured web search provider.",
                "query": query,
                "status": "web_search_placeholder"
            })
        }
        _ => serde_json::json!({"error": format!("unknown tool: {}", tool_name)}),
    }
}

// ── Agent turn execution ────────────────────────────────────────────────

/// Build the system prompt for an agent's turn.
fn build_agent_system_prompt(
    agent: &DebateAgent,
    topic: &str,
    round: usize,
    previous_turns: &[DebateTurn],
    agents: &[DebateAgent],
    user_injection: Option<&str>,
) -> String {
    let mut prompt = format!(
        "You are {}, {}.\n\n",
        agent.name, agent.persona_description
    );

    // Rigor instructions
    prompt.push_str(&format!("Your analysis characteristics:\n"));
    prompt.push_str(&format!("- Rigor level: {:.0}% ({})\n",
        agent.rigor_level * 100.0,
        if agent.rigor_level < 0.3 { "you follow hunches, rumors, and unverified claims freely" }
        else if agent.rigor_level < 0.7 { "you balance evidence with reasonable inference" }
        else { "you ONLY cite verified, high-confidence evidence; reject all speculation" }
    ));
    prompt.push_str(&format!("- Evidence standard: only cite facts with confidence >= {:.2}\n", agent.evidence_threshold));
    prompt.push_str(&format!("- Cognitive style: {}\n", agent.cognitive_style));
    prompt.push_str(&format!("- Source access: {}\n\n", agent.source_access));

    // Bias instructions
    if agent.bias.is_neutral {
        prompt.push_str("You are an unbiased analyst. Follow the evidence wherever it leads, regardless of political or institutional implications.\n\n");
    } else {
        prompt.push_str(&format!(
            "You represent the **{}** perspective. Your role is to argue for: {}. You may use evidence selectively to support your position. You are not seeking truth -- you are advocating.\n\n",
            agent.bias.label, agent.bias.description
        ));
    }

    prompt.push_str(&format!("You are participating in Round {} of a structured debate on: \"{}\"\n\n", round + 1, topic));

    // Previous round context
    if !previous_turns.is_empty() {
        prompt.push_str("Previous round positions:\n");
        for turn in previous_turns {
            let agent_name = agents.iter().find(|a| a.id == turn.agent_id)
                .map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            prompt.push_str(&format!("- {} said: \"{}\" (confidence: {:.2})\n",
                agent_name, truncate(&turn.position, 300), turn.confidence));
        }
        prompt.push('\n');

        // Evolution instructions (only after round 1)
        prompt.push_str(
            "IMPORTANT -- POSITION EVOLUTION:\n\
             This is NOT round 1. You have heard other agents' arguments. You MUST evolve your thinking:\n\
             - If another agent presented compelling evidence you hadn't considered, ACKNOWLEDGE it and adjust your position.\n\
             - If your confidence should change based on what you heard, change it. Don't stubbornly hold the same number.\n\
             - You CAN concede specific points while maintaining your overall stance.\n\
             - You CAN shift your position entirely if the evidence warrants it.\n\
             - Real analysts update their views. Stubbornly repeating the same position without engaging with new evidence is a sign of bias, not strength.\n"
        );
        if !agent.bias.is_neutral {
            prompt.push_str(
                "- Even as an advocate, you should acknowledge when opposing evidence is strong. \
                 Credibility comes from honest engagement, not blind advocacy.\n"
            );
        }
        prompt.push('\n');
    }

    // User injection
    if let Some(injection) = user_injection {
        prompt.push_str(&format!("The moderator has asked: \"{}\"\nYou MUST address this in your response.\n\n", injection));
    }

    prompt.push_str(
        "Use the provided tools to gather evidence from the knowledge graph before forming your position.\n\
         After gathering evidence, state your position clearly.\n\n\
         At the END of your response, include these EXACT lines:\n\
         CONFIDENCE: X.XX\n\
         AGREES_WITH: [comma-separated agent names, or \"none\"]\n\
         DISAGREES_WITH: [comma-separated agent names, or \"none\"]\n"
    );

    // Evolution metadata (only after round 1)
    if !previous_turns.is_empty() {
        prompt.push_str(
            "POSITION_SHIFT: [1 sentence: what changed in your thinking since last round, or \"No change\" if nothing did]\n\
             CONCESSIONS: [comma-separated specific points you now agree with from other agents, or \"none\"]\n"
        );
    }

    prompt
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

/// Parsed metadata from an agent's turn response.
pub struct TurnMetadata {
    pub confidence: f32,
    pub agrees_with: Vec<String>,
    pub disagrees_with: Vec<String>,
    pub position_shift: String,
    pub concessions: Vec<String>,
}

/// Parse confidence, agrees/disagrees, position_shift, concessions from agent response.
pub fn parse_turn_metadata(text: &str, agents: &[DebateAgent]) -> TurnMetadata {
    let mut confidence = 0.5;
    let mut agrees = Vec::new();
    let mut disagrees = Vec::new();
    let mut position_shift = String::new();
    let mut concessions = Vec::new();

    for line in text.lines().rev().take(15) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("CONFIDENCE:") {
            if let Ok(c) = rest.trim().parse::<f32>() {
                confidence = c.clamp(0.0, 1.0);
            }
        } else if let Some(rest) = line.strip_prefix("AGREES_WITH:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                agrees = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        } else if let Some(rest) = line.strip_prefix("DISAGREES_WITH:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                disagrees = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        } else if let Some(rest) = line.strip_prefix("POSITION_SHIFT:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "no change" && !cleaned.is_empty() {
                position_shift = cleaned.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("CONCESSIONS:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                concessions = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        }
    }

    // Map agent names to IDs
    let name_to_id = |name: &str| -> String {
        agents.iter()
            .find(|a| a.name.eq_ignore_ascii_case(name) || a.id == name)
            .map(|a| a.id.clone())
            .unwrap_or_else(|| name.to_string())
    };
    agrees = agrees.iter().map(|n| name_to_id(n)).collect();
    disagrees = disagrees.iter().map(|n| name_to_id(n)).collect();

    TurnMetadata { confidence, agrees_with: agrees, disagrees_with: disagrees, position_shift, concessions }
}

/// Strip metadata lines from the position text.
pub fn strip_metadata_lines(text: &str) -> String {
    let metadata_prefixes = ["CONFIDENCE:", "AGREES_WITH:", "DISAGREES_WITH:", "POSITION_SHIFT:", "CONCESSIONS:"];
    let lines: Vec<&str> = text.lines().collect();
    let mut end = lines.len();
    for i in (0..lines.len()).rev() {
        let l = lines[i].trim();
        if metadata_prefixes.iter().any(|p| l.starts_with(p)) {
            end = i;
        } else if !l.is_empty() {
            break;
        }
    }
    lines[..end].join("\n").trim().to_string()
}

/// Execute a single agent's turn: tool-calling loop + final position.
async fn execute_agent_turn(
    state: &AppState,
    agent: &DebateAgent,
    topic: &str,
    round: usize,
    previous_turns: &[DebateTurn],
    all_agents: &[DebateAgent],
    user_injection: Option<&str>,
    tx: &tokio::sync::broadcast::Sender<String>,
) -> Result<DebateTurn, String> {
    let system_prompt = build_agent_system_prompt(agent, topic, round, previous_turns, all_agents, user_injection);
    let tools = tools_for_agent(agent);

    let mut messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
        serde_json::json!({"role": "user", "content": format!("Analyze the topic \"{}\" using your available tools, then state your position.", topic)}),
    ];

    let mut all_tool_invocations = Vec::new();
    let mut all_evidence = Vec::new();
    let max_tool_rounds = 5;

    // Tool-calling loop
    for _tool_round in 0..max_tool_rounds {
        let request = serde_json::json!({
            "messages": messages,
            "tools": tools,
            "temperature": (0.5 + agent.rigor_level * 0.3).min(1.0),
            "max_tokens": 2048
        });

        let response = call_llm(state, request).await?;
        let tool_calls = extract_tool_calls(&response);

        if tool_calls.is_empty() {
            // No more tool calls -- extract final response
            let content = extract_content(&response).unwrap_or_default();
            let meta = parse_turn_metadata(&content, all_agents);
            let position = strip_metadata_lines(&content);

            return Ok(DebateTurn {
                agent_id: agent.id.clone(),
                position,
                evidence: all_evidence,
                confidence: meta.confidence,
                tools_used: all_tool_invocations,
                agrees_with: meta.agrees_with,
                disagrees_with: meta.disagrees_with,
                position_shift: meta.position_shift,
                concessions: meta.concessions,
            });
        }

        // Process tool calls
        let assistant_msg = response.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .cloned()
            .unwrap_or(serde_json::json!({}));
        messages.push(assistant_msg);

        for tc in &tool_calls {
            let fn_name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            let fn_args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
            let fn_args: serde_json::Value = serde_json::from_str(fn_args_str).unwrap_or(serde_json::json!({}));
            let tc_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();

            // Emit SSE tool_call event
            let _ = tx.send(format!("event: tool_call\ndata: {}\n\n", serde_json::json!({
                "agent_id": agent.id,
                "tool_name": fn_name,
                "args": fn_args
            })));

            let result = execute_tool(state, fn_name, &fn_args).await;
            let result_str = serde_json::to_string(&result).unwrap_or_default();
            let summary = truncate(&result_str, 500).to_string();

            // Emit SSE tool_result event
            let _ = tx.send(format!("event: tool_result\ndata: {}\n\n", serde_json::json!({
                "agent_id": agent.id,
                "tool_name": fn_name,
                "summary": summary
            })));

            all_tool_invocations.push(ToolInvocation {
                tool_name: fn_name.to_string(),
                arguments: fn_args.clone(),
                result_summary: summary.clone(),
            });

            // Extract evidence from search/query results
            if fn_name == "engram_search" || fn_name == "engram_query" || fn_name == "engram_explain" {
                if let Some(results) = result.get("results").and_then(|r| r.as_array())
                    .or_else(|| result.get("traversal").and_then(|t| t.as_array())) {
                    for r in results.iter().take(5) {
                        let label = r.get("label").and_then(|l| l.as_str()).unwrap_or("");
                        let conf = r.get("confidence").or(r.get("score")).and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
                        if !label.is_empty() {
                            all_evidence.push(TurnEvidence {
                                entity: label.to_string(),
                                confidence: conf,
                                source: "graph".into(),
                                supporting: true,
                            });
                        }
                    }
                }
            }

            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc_id,
                "content": result_str
            }));
        }
    }

    // Fallback if max tool rounds exceeded
    let fallback_request = serde_json::json!({
        "messages": messages,
        "temperature": 0.7,
        "max_tokens": 1024
    });
    let response = call_llm(state, fallback_request).await?;
    let content = extract_content(&response).unwrap_or_else(|| "No position stated.".into());
    let meta = parse_turn_metadata(&content, all_agents);
    let position = strip_metadata_lines(&content);

    Ok(DebateTurn {
        agent_id: agent.id.clone(),
        position,
        evidence: all_evidence,
        confidence: meta.confidence,
        tools_used: all_tool_invocations,
        agrees_with: meta.agrees_with,
        disagrees_with: meta.disagrees_with,
        position_shift: meta.position_shift,
        concessions: meta.concessions,
    })
}

// ── Synthesis ───────────────────────────────────────────────────────────

/// Build the synthesis prompt for 3-layer analysis.
fn build_synthesis_prompt(session: &DebateSession) -> serde_json::Value {
    let mut transcript = String::new();
    for round in &session.rounds {
        transcript.push_str(&format!("\n=== Round {} ===\n", round.round_number + 1));
        if let Some(ref inj) = round.user_injection {
            transcript.push_str(&format!("Moderator asked: \"{}\"\n\n", inj));
        }
        for turn in &round.turns {
            let agent = session.agents.iter().find(|a| a.id == turn.agent_id);
            let name = agent.map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            let bias_info = agent.map(|a| {
                if a.bias.is_neutral { "NEUTRAL".to_string() }
                else { format!("BIAS: {} -- {}", a.bias.label, a.bias.description) }
            }).unwrap_or_default();
            let shift_info = if turn.position_shift.is_empty() { String::new() }
                else { format!("\nPOSITION SHIFT: {}", turn.position_shift) };
            let concession_info = if turn.concessions.is_empty() { String::new() }
                else { format!("\nCONCESSIONS: {}", turn.concessions.join(", ")) };
            transcript.push_str(&format!(
                "{} [{}] (confidence: {:.2}):\n{}{}{}\n\nEvidence cited: {} items | Tools used: {}\n\n",
                name, bias_info, turn.confidence, turn.position,
                shift_info, concession_info,
                turn.evidence.len(), turn.tools_used.len()
            ));
        }
    }

    let agent_profiles: Vec<String> = session.agents.iter().map(|a| {
        format!("- {} ({}): rigor={:.1}, source={}, bias={}",
            a.name, a.id, a.rigor_level, a.source_access,
            if a.bias.is_neutral { "NEUTRAL".to_string() } else { a.bias.label.clone() }
        )
    }).collect();

    let system_prompt = format!(
        r#"You are a senior intelligence synthesis analyst. You must produce a structured 3-layer analysis of the following debate.

Topic: "{topic}"
Rounds completed: {rounds}

Agent profiles:
{profiles}

Full debate transcript:
{transcript}

The original question/thesis being debated is: "{topic}"

Produce your analysis in EXACTLY this JSON structure (no markdown, no commentary):
{{
  "evidence_conclusion": "MUST DIRECTLY ANSWER THE QUESTION. Start with: 'Based on the evidence, [concrete answer]. ...' Do NOT hedge with 'it is complex' -- commit to a probabilistic assessment. If the question asks about an outcome, state the MOST LIKELY outcome with a probability range. For example: 'Based on the evidence, the most likely outcome is X (60-75% probability) because Y. The alternative scenario Z has a 20-30% probability if conditions A change.'",
  "conclusion_confidence": 0.XX,
  "evidence_gaps": ["SPECIFIC, ACTIONABLE gaps -- name concrete data points, facts, or entities that are MISSING from the knowledge graph and would improve this analysis. Format each as: 'Missing: [specific fact/data] -- needed because [why it matters]'. These should be ingestible as search/investigation targets."],
  "key_evidence": [{{"entity": "...", "fact": "...", "confidence": 0.XX}}],

  "influence_map": [{{"agent_id": "...", "agent_name": "...", "bias_label": "...", "position_pushed": "...", "evidence_backed": true/false, "distortion_summary": "..."}}],
  "unexpected_alignments": [{{"agents": ["id1","id2"], "common_position": "...", "reason": "..."}}],
  "cherry_picks": [{{"agent_id": "...", "ignored_evidence": "...", "why_ignored": "..."}}],

  "hidden_agendas": [{{"agent_id": "...", "agent_name": "...", "stated_position": "...", "underlying_interest": "...", "who_benefits": "...", "what_they_avoid": "...", "what_they_lose": "...", "second_order_effects": ["..."]}}],
  "beneficiary_map": [{{"position": "...", "beneficiaries": ["..."], "mechanism": "..."}}],
  "parallel_interests": [{{"agents": ["id1","id2"], "surface_disagreement": "...", "hidden_alignment": "..."}}],
  "blind_spots": [{{"agent_id": "...", "topic_avoided": "...", "likely_reason": "..."}}],

  "areas_of_agreement": [{{"statement": "...", "agents": ["id1","id2"], "confidence": 0.XX}}],
  "areas_of_disagreement": [{{"statement": "...", "positions": [["agent_id", "their view"]]}}],
  "key_tensions": ["..."],
  "recommended_investigations": ["SPECIFIC investigative actions to close the evidence gaps. Each should be a concrete query or data source to check, not vague 'more research needed'. Format: 'Search for [specific topic] in [specific source] to determine [what it would clarify]'"],
  "evolution": [{{"agent_id": "...", "agent_name": "...", "confidence_trajectory": [0.XX, 0.YY], "net_shift": 0.XX, "pivot_cause": "what caused the biggest change", "flexibility_score": 0.XX, "key_concessions": ["..."], "bias_override": false}}],

  "agent_positions": [{{"agent_id": "...", "agent_name": "...", "final_position": "...", "confidence": 0.XX, "evidence_count": N}}]
}}

CRITICAL INSTRUCTIONS:

MOST IMPORTANT: The evidence_conclusion MUST DIRECTLY ANSWER the original question "{topic}". Do NOT just describe the debate -- ANSWER THE QUESTION. Commit to a most-likely outcome with a probability range. Intelligence analysts make assessments, not summaries.

Layer 1 (evidence_conclusion): Strip away ALL agenda-driven rhetoric. What do the FACTS support? State the most probable answer. If temporal (will X happen?), give a timeline. If binary (should X?), state yes/no with probability. Events have outcomes -- state the most likely one.

evidence_gaps: Be SPECIFIC. Not "more research needed" but "Missing: Iran's current daily oil export volume in barrels -- needed to calculate actual supply disruption impact". Each gap should be a concrete fact that could be stored in a knowledge graph.

recommended_investigations: Each must be an actionable search query, not a platitude. "Search for OPEC spare capacity data from IEA monthly reports" not "investigate oil markets further".

Layer 2 (influence_map): How did each biased agent frame their argument? Where did bias distort evidence? Which biased arguments were actually evidence-backed despite the bias?
Layer 3 (hidden_agendas): For each biased agent: WHY are they REALLY pushing this? Who benefits? What are they NOT saying? What would they lose?
Layer 4 (evolution): Track how positions evolved. confidence_trajectory = array of confidence per round. net_shift = last minus first. flexibility_score: positive = adapted to evidence, negative = dug in despite counter-evidence. bias_override = true if bias clearly prevented acknowledging strong counter-evidence."#,
        topic = session.topic,
        rounds = session.rounds.len(),
        profiles = agent_profiles.join("\n"),
        transcript = transcript,
    );

    serde_json::json!({
        "messages": [
            { "role": "system", "content": system_prompt }
        ],
        "temperature": 0.3,
        "max_tokens": 4096
    })
}

// ── HTTP handlers ───────────────────────────────────────────────────────

/// POST /debate/start -- create a new debate session and generate the panel.
pub async fn debate_start(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> ApiResult<serde_json::Value> {
    let count = req.agent_count.max(2).min(8);
    let session_id = format!("debate-{}", uuid_short());

    // Create deterministic agent slots
    let mut agents = assign_agent_slots(count);

    // Generate persona details via LLM
    let prompt = build_persona_generation_prompt(&req.topic, &agents);
    let response = call_llm(&state, prompt).await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Panel generation failed: {e}")))?;

    let content = extract_content(&response)
        .ok_or_else(|| api_err(StatusCode::BAD_GATEWAY, "No content in LLM response"))?;

    let persona_data = parse_json_from_llm(&content)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e))?;

    // Merge LLM-generated details into agent slots
    if let Some(arr) = persona_data.as_array() {
        for item in arr {
            let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("");
            if let Some(agent) = agents.iter_mut().find(|a| a.id == id) {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    agent.name = name.to_string();
                }
                if let Some(desc) = item.get("persona_description").and_then(|d| d.as_str()) {
                    agent.persona_description = desc.to_string();
                }
                if let Some(icon) = item.get("icon").and_then(|i| i.as_str()) {
                    agent.icon = icon.to_string();
                }
                if let Some(color) = item.get("color").and_then(|c| c.as_str()) {
                    agent.color = color.to_string();
                }
                // Only update bias for non-neutral agents
                if !agent.bias.is_neutral {
                    if let Some(bl) = item.get("bias_label").and_then(|b| b.as_str()) {
                        agent.bias.label = bl.to_string();
                    }
                    if let Some(bd) = item.get("bias_description").and_then(|b| b.as_str()) {
                        agent.bias.description = bd.to_string();
                    }
                }
            }
        }
    }

    // Fill in any agents that didn't get names from the LLM
    for (i, agent) in agents.iter_mut().enumerate() {
        if agent.name.is_empty() {
            agent.name = format!("Analyst {}", i + 1);
        }
        if agent.persona_description.is_empty() {
            agent.persona_description = format!("A {} analyst", agent.cognitive_style);
        }
    }

    let session = DebateSession {
        session_id: session_id.clone(),
        topic: req.topic.clone(),
        status: DebateStatus::AwaitingStart,
        agents: agents.clone(),
        rounds: Vec::new(),
        current_round: 0,
        max_rounds: req.max_rounds as usize,
        synthesis: None,
        created_at: std::time::Instant::now(),
        notify: Arc::new(Notify::new()),
        pending_injection: None,
    };

    // Store session
    {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        sessions.insert(session_id.clone(), session);
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "topic": req.topic,
        "status": "awaiting_start",
        "agents": agents,
        "max_rounds": req.max_rounds
    })))
}

/// GET /debate/{id} -- get full debate state.
pub async fn debate_get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let sessions = state.debate_sessions.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    Ok(Json(serde_json::json!({
        "session_id": session.session_id,
        "topic": session.topic,
        "status": session.status,
        "agents": session.agents,
        "rounds": session.rounds,
        "current_round": session.current_round,
        "max_rounds": session.max_rounds,
        "synthesis": session.synthesis,
    })))
}

/// PATCH /debate/{id}/agents -- edit agents before starting.
pub async fn debate_edit_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(edits): Json<AgentEdits>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    if session.status != DebateStatus::AwaitingStart {
        return Err(api_err(StatusCode::CONFLICT, "can only edit agents before debate starts"));
    }

    for edit in &edits.agents {
        if let Some(agent) = session.agents.iter_mut().find(|a| a.id == edit.id) {
            if let Some(ref name) = edit.name { agent.name = name.clone(); }
            if let Some(ref desc) = edit.persona_description { agent.persona_description = desc.clone(); }
            if let Some(rigor) = edit.rigor_level { agent.rigor_level = rigor.clamp(0.0, 1.0); }
            if let Some(ref sa) = edit.source_access { agent.source_access = sa.clone(); }
            if let Some(et) = edit.evidence_threshold { agent.evidence_threshold = et.clamp(0.0, 1.0); }
            if let Some(ref cs) = edit.cognitive_style { agent.cognitive_style = cs.clone(); }
            if let Some(ref bias) = edit.bias { agent.bias = bias.clone(); }
            if let Some(ref icon) = edit.icon { agent.icon = icon.clone(); }
            if let Some(ref color) = edit.color { agent.color = color.clone(); }
        }
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "agents": session.agents,
    })))
}

/// POST /debate/{id}/run -- start or resume the debate.
pub async fn debate_run(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let should_spawn = {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

        match session.status {
            DebateStatus::AwaitingStart => {
                // First start: spawn a new loop
                session.status = DebateStatus::Running;
                true
            }
            DebateStatus::AwaitingInput => {
                // Resume: wake the existing loop, don't spawn a new one
                session.status = DebateStatus::Running;
                session.notify.notify_one();
                false
            }
            DebateStatus::AllRoundsComplete => {
                return Err(api_err(StatusCode::CONFLICT, "all rounds complete -- use /synthesize"));
            }
            _ => {
                return Err(api_err(StatusCode::CONFLICT, format!("cannot run debate in {:?} status", session.status)));
            }
        }
    };

    if should_spawn {
        let state_clone = state.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            run_debate_loop(state_clone, sid).await;
        });
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "running",
        "message": "Debate running."
    })))
}

/// Background debate execution loop.
async fn run_debate_loop(state: AppState, session_id: String) {
    // Get session info
    let (agents, max_rounds, current_round, topic, tx) = {
        let sessions = match state.debate_sessions.read() {
            Ok(s) => s,
            Err(_) => return,
        };
        let session = match sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };
        let (tx, _) = tokio::sync::broadcast::channel::<String>(256);
        (session.agents.clone(), session.max_rounds, session.current_round, session.topic.clone(), tx)
    };

    // Store the broadcast sender for SSE subscribers
    {
        // We'll use a different approach: store events on the session directly
    }

    for round_idx in current_round..max_rounds {
        // Check if we should stop
        {
            let sessions = match state.debate_sessions.read() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get(&session_id) {
                if s.status == DebateStatus::Complete || s.status == DebateStatus::Error {
                    return;
                }
            }
        }

        // Get any pending injection
        let user_injection = {
            let mut sessions = match state.debate_sessions.write() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get_mut(&session_id) {
                s.pending_injection.take()
            } else {
                None
            }
        };

        let _ = tx.send(format!("event: round_start\ndata: {}\n\n",
            serde_json::json!({"round": round_idx + 1})));

        let mut round_turns = Vec::new();

        // Execute each agent's turn
        for agent in &agents {
            let _ = tx.send(format!("event: turn_start\ndata: {}\n\n",
                serde_json::json!({"agent_id": agent.id, "agent_name": agent.name, "round": round_idx + 1})));

            // Get previous turns for context (from previous round)
            let prev_turns = {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get(&session_id) {
                    if let Some(prev_round) = s.rounds.last() {
                        prev_round.turns.clone()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            };

            match execute_agent_turn(
                &state, agent, &topic, round_idx, &prev_turns, &agents,
                user_injection.as_deref(), &tx,
            ).await {
                Ok(turn) => {
                    let _ = tx.send(format!("event: turn_complete\ndata: {}\n\n",
                        serde_json::json!({
                            "agent_id": agent.id,
                            "agent_name": agent.name,
                            "position": turn.position,
                            "confidence": turn.confidence,
                            "evidence_count": turn.evidence.len(),
                            "agrees_with": turn.agrees_with,
                            "disagrees_with": turn.disagrees_with,
                        })));
                    round_turns.push(turn);
                }
                Err(e) => {
                    let _ = tx.send(format!("event: error\ndata: {}\n\n",
                        serde_json::json!({"agent_id": agent.id, "error": e})));
                    // Continue with other agents
                }
            }
        }

        // Store round results
        {
            let mut sessions = match state.debate_sessions.write() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get_mut(&session_id) {
                s.rounds.push(DebateRound {
                    round_number: round_idx,
                    turns: round_turns,
                    user_injection: user_injection.clone(),
                });
                s.current_round = round_idx + 1;
            }
        }

        let _ = tx.send(format!("event: round_complete\ndata: {}\n\n",
            serde_json::json!({"round": round_idx + 1})));

        // Pause for user input between rounds (except after the last round)
        if round_idx + 1 < max_rounds {
            {
                let mut sessions = match state.debate_sessions.write() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get_mut(&session_id) {
                    s.status = DebateStatus::AwaitingInput;
                }
            }

            let _ = tx.send(format!("event: awaiting_input\ndata: {}\n\n",
                serde_json::json!({"round": round_idx + 1, "prompt": "Continue, inject a question, or synthesize?"})));

            // Wait for user to resume
            let notify = {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                sessions.get(&session_id).map(|s| s.notify.clone())
            };

            if let Some(n) = notify {
                n.notified().await;
            }

            // Check if status changed (might have been set to Synthesizing or stopped)
            {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get(&session_id) {
                    match s.status {
                        DebateStatus::Running => {} // continue
                        DebateStatus::Synthesizing => break, // skip to synthesis
                        _ => return, // stopped or error
                    }
                }
            }
        }
    }

    // All rounds complete -- check if synthesis was requested early
    {
        let sessions = match state.debate_sessions.read() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(s) = sessions.get(&session_id) {
            if s.status == DebateStatus::Complete {
                return;
            }
        }
    }

    // All rounds done -- set distinct status so frontend knows to show only Synthesize
    {
        let mut sessions = match state.debate_sessions.write() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(s) = sessions.get_mut(&session_id) {
            s.status = DebateStatus::AllRoundsComplete;
        }
    }

    let _ = tx.send(format!("event: all_rounds_complete\ndata: {}\n\n",
        serde_json::json!({"prompt": "All rounds complete. Click Synthesize to generate the analysis."})));
}

/// POST /debate/{id}/inject -- user injects a question between rounds.
pub async fn debate_inject(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<InjectRequest>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    if session.status != DebateStatus::AwaitingInput {
        return Err(api_err(StatusCode::CONFLICT, "can only inject when debate is awaiting input"));
    }

    session.pending_injection = Some(req.message.clone());
    session.status = DebateStatus::Running;
    session.notify.notify_one();

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "injected": req.message,
        "status": "running"
    })))
}

/// POST /debate/{id}/stop -- pause the debate.
pub async fn debate_stop(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    session.status = DebateStatus::AwaitingInput;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "awaiting_input"
    })))
}

/// POST /debate/{id}/synthesize -- trigger final synthesis.
pub async fn debate_synthesize(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    // Get session data for synthesis
    let session_data = {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

        if session.rounds.is_empty() {
            return Err(api_err(StatusCode::CONFLICT, "no rounds completed yet"));
        }

        session.status = DebateStatus::Synthesizing;
        // If debate loop is waiting, wake it up
        session.notify.notify_one();
        session.clone()
    };

    // Run synthesis LLM call
    let prompt = build_synthesis_prompt(&session_data);
    let response = call_llm(&state, prompt).await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Synthesis failed: {e}")))?;

    let content = extract_content(&response)
        .ok_or_else(|| api_err(StatusCode::BAD_GATEWAY, "No content in synthesis response"))?;

    let synthesis_json = parse_json_from_llm(&content)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Synthesis parse error: {e}")))?;

    // Parse synthesis into typed struct
    let synthesis: Synthesis = serde_json::from_value(synthesis_json.clone())
        .unwrap_or_else(|_| {
            // Fallback: construct minimal synthesis from raw JSON
            Synthesis {
                evidence_conclusion: synthesis_json.get("evidence_conclusion").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                conclusion_confidence: synthesis_json.get("conclusion_confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32,
                evidence_gaps: extract_string_array(&synthesis_json, "evidence_gaps"),
                key_evidence: Vec::new(),
                influence_map: Vec::new(),
                unexpected_alignments: Vec::new(),
                cherry_picks: Vec::new(),
                hidden_agendas: Vec::new(),
                beneficiary_map: Vec::new(),
                parallel_interests: Vec::new(),
                blind_spots: Vec::new(),
                areas_of_agreement: Vec::new(),
                areas_of_disagreement: Vec::new(),
                key_tensions: extract_string_array(&synthesis_json, "key_tensions"),
                recommended_investigations: extract_string_array(&synthesis_json, "recommended_investigations"),
                evolution: Vec::new(),
                agent_positions: Vec::new(),
            }
        });

    // Store synthesis on session
    {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.synthesis = Some(synthesis.clone());
            session.status = DebateStatus::Complete;
        }
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "complete",
        "synthesis": synthesis,
    })))
}

fn extract_string_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// GET /debate/{id}/stream -- SSE event stream for debate progress.
/// Uses polling (every 2s) to emit state changes, round completions, and synthesis.
pub async fn debate_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    struct PollState {
        state: AppState,
        session_id: String,
        last_round_count: usize,
        last_status: String,
        sent_initial: bool,
    }

    let initial = PollState {
        state,
        session_id,
        last_round_count: 0,
        last_status: String::new(),
        sent_initial: false,
    };

    let stream = futures::stream::unfold(initial, |mut ps| async move {
        if !ps.sent_initial {
            ps.sent_initial = true;
            let data = {
                let sessions = ps.state.debate_sessions.read().unwrap_or_else(|e| e.into_inner());
                sessions.get(&ps.session_id).map(|session| {
                    ps.last_status = format!("{:?}", session.status);
                    ps.last_round_count = session.rounds.len();
                    serde_json::json!({
                        "status": session.status,
                        "agents": session.agents,
                        "current_round": session.current_round,
                        "rounds_completed": session.rounds.len(),
                    })
                })
            };
            return match data {
                Some(d) => Some((
                    Ok(axum::response::sse::Event::default().event("state").data(d.to_string())),
                    ps,
                )),
                None => None,
            };
        }

        // Poll every 2 seconds
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Snapshot session state (drop lock before returning ps)
        let snapshot = ps.state.debate_sessions.read().ok().and_then(|sessions| {
            sessions.get(&ps.session_id).map(|s| {
                (format!("{:?}", s.status), s.status.clone(), s.rounds.clone(), s.synthesis.clone())
            })
        });

        let (status, status_enum, rounds, synthesis) = match snapshot {
            Some(s) => s,
            None => return None, // session deleted
        };

        let round_count = rounds.len();

        // Emit new round data
        if round_count > ps.last_round_count {
            if let Some(round) = rounds.get(ps.last_round_count) {
                ps.last_round_count += 1;
                let data = serde_json::json!({
                    "round": round.round_number + 1,
                    "turns": round.turns,
                    "user_injection": round.user_injection,
                });
                return Some((
                    Ok(axum::response::sse::Event::default().event("round_complete").data(data.to_string())),
                    ps,
                ));
            }
        }

        // Emit status changes
        if status != ps.last_status {
            ps.last_status = status.clone();

            // If complete, emit synthesis
            if status_enum == DebateStatus::Complete {
                if let Some(ref syn) = synthesis {
                    let data = serde_json::to_string(syn).unwrap_or_default();
                    return Some((
                        Ok(axum::response::sse::Event::default().event("synthesis_complete").data(data)),
                        ps,
                    ));
                }
            }

            let data = serde_json::json!({"status": status});
            return Some((
                Ok(axum::response::sse::Event::default().event("status_change").data(data.to_string())),
                ps,
            ));
        }

        // Complete -- stop stream
        if status_enum == DebateStatus::Complete {
            return None;
        }

        // Keepalive
        Some((
            Ok(axum::response::sse::Event::default().comment("keepalive")),
            ps,
        ))
    });

    axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}

/// DELETE /debate/{id} -- remove a debate session.
pub async fn debate_delete(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    sessions.remove(&session_id);
    Ok(Json(serde_json::json!({"deleted": session_id})))
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Generate a short UUID-like ID.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:x}", ts & 0xFFFF_FFFF_FFFF)
}

// Tests are in tests/debate.rs (integration test file).
