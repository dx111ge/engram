/// Data types for the multi-agent debate panel.

use std::sync::Arc;
use tokio::sync::Notify;

// ── Session ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DebateSession {
    pub session_id: String,
    pub topic: String,
    pub mode: DebateMode,
    pub status: DebateStatus,
    pub agents: Vec<DebateAgent>,
    pub rounds: Vec<DebateRound>,
    pub current_round: usize,
    pub max_rounds: usize,
    pub selection: Option<SelectionResult>,
    pub synthesis: Option<Synthesis>,
    pub created_at: std::time::Instant,
    pub notify: Arc<Notify>,
    pub pending_injection: Option<String>,
    pub briefing: Option<Briefing>,
    pub researched_gaps: Vec<String>,
    /// Mode-specific extra input (desired outcome, actor list, options, plan to test).
    pub mode_input: Option<String>,
    /// Live progress for frontend display.
    pub progress: Option<DebateProgress>,
    /// LLM-generated short search queries derived from the topic.
    /// Generated once at briefing, reused for all agent web searches.
    pub search_queries: Vec<String>,
    /// Languages relevant to the topic (ISO 639-1 codes, always includes "en").
    /// Detected by LLM during briefing phase.
    pub topic_languages: Vec<String>,
}

/// Live progress information for the frontend.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebateProgress {
    pub phase: String,
    pub message: String,
    /// Current agent being processed (if applicable).
    pub active_agent: Option<String>,
    /// Progress counter (e.g., "2 of 4").
    pub current: usize,
    pub total: usize,
}

// ── Debate Modes ────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateMode {
    /// Analyze: "What is happening? What's likely?"
    Analyze,
    /// Red Team: "How to achieve X? What breaks it?"
    RedTeam,
    /// Outcome Engineering: "What must be true for X to happen?"
    OutcomeEngineering,
    /// Scenario Forecast: "What are the plausible futures?"
    ScenarioForecast,
    /// Stakeholder Simulation: "How will actual players react?"
    StakeholderSimulation,
    /// Pre-mortem: "Assume plan X failed. Why?"
    Premortem,
    /// Decision Matrix: "Should we do A, B, or C?"
    DecisionMatrix,
}

impl Default for DebateMode {
    fn default() -> Self { Self::Analyze }
}

impl std::fmt::Display for DebateMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Analyze => write!(f, "Analyze"),
            Self::RedTeam => write!(f, "Red Team"),
            Self::OutcomeEngineering => write!(f, "Outcome Engineering"),
            Self::ScenarioForecast => write!(f, "Scenario Forecast"),
            Self::StakeholderSimulation => write!(f, "Stakeholder Simulation"),
            Self::Premortem => write!(f, "Pre-mortem"),
            Self::DecisionMatrix => write!(f, "Decision Matrix"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateStatus {
    GeneratingPanel,
    /// Starter plate: gathering facts before debate starts.
    Researching,
    AwaitingStart,
    Running,
    AwaitingInput,
    AllRoundsComplete,
    Synthesizing,
    Complete,
    Error,
}

// ── Briefing (starter plate) ────────────────────────────────────────────

/// Factual briefing assembled before Round 1 via topic decomposition + multi-source research.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Briefing {
    /// The original topic decomposed into factual sub-questions.
    pub questions: Vec<String>,
    /// Facts gathered from all sources, organized by question.
    pub facts: Vec<BriefingFact>,
    /// Pipeline stats: how much was ingested.
    pub facts_stored: u32,
    pub relations_created: u32,
    /// Summary text for agents (included in their system prompt).
    pub summary: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BriefingFact {
    pub question: String,
    pub source: String,
    pub content: String,
    pub confidence: f32,
}

// ── Moderator checks ────────────────────────────────────────────────────

/// A fact-check by the moderator agent against engram confidence scores.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ModeratorCheck {
    pub agent_id: String,
    pub claim: String,
    pub verdict: ModeratorVerdict,
    pub engram_confidence: Option<f32>,
    pub explanation: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeratorVerdict {
    /// Claim matches high-confidence data in engram.
    Supported,
    /// Claim contradicts data in engram.
    Contradicted,
    /// Claim has no supporting evidence in engram (not necessarily wrong).
    Unsupported,
    /// Claim uses low-confidence data as if it's certain.
    LowConfidence,
}

// ── Agents ───────────────────────────────────────────────────────────────

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

/// Research style: how the agent approaches additional research beyond the briefing.
/// All agents have full access to the briefing + graph + web. This controls their *approach*.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAccess {
    /// Deep research: thorough graph traversal + web search before responding.
    Comprehensive,
    /// Relies mostly on the briefing with minimal additional search.
    BriefingFocused,
    /// Specifically searches for evidence AGAINST the consensus.
    Contrarian,
    /// Focuses on low-confidence and disputed data to find weak signals.
    WeakSignals,
    // Legacy variants (kept for backwards compat with existing sessions)
    GraphOnly,
    GraphAndWeb,
    GraphLowConfidence,
    WebOnly,
}

impl std::fmt::Display for SourceAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Comprehensive => write!(f, "Deep research (graph + web)"),
            Self::BriefingFocused => write!(f, "Briefing-focused"),
            Self::Contrarian => write!(f, "Contrarian research"),
            Self::WeakSignals => write!(f, "Weak signals & low-confidence"),
            Self::GraphOnly => write!(f, "Engram graph only"),
            Self::GraphAndWeb => write!(f, "Engram graph + web search"),
            Self::GraphLowConfidence => write!(f, "Engram graph (low-confidence)"),
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

// ── Rounds & Turns ──────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DebateRound {
    pub round_number: usize,
    pub turns: Vec<DebateTurn>,
    pub user_injection: Option<String>,
    #[serde(default)]
    pub gap_research: Vec<GapResearch>,
    /// Moderator fact-checks for this round's claims.
    #[serde(default)]
    pub moderator_checks: Vec<ModeratorCheck>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GapResearch {
    pub gap_query: String,
    pub source: String,
    pub findings: Vec<String>,
    pub ingested: bool,
    pub entities_stored: Vec<String>,
    /// Pipeline stats: facts stored + relations created from ingest.
    #[serde(default)]
    pub facts_stored: u32,
    #[serde(default)]
    pub relations_created: u32,
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
    #[serde(default)]
    pub position_shift: String,
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

// ── Synthesis ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Synthesis {
    pub evidence_conclusion: String,
    pub conclusion_confidence: f32,
    pub evidence_gaps: Vec<String>,
    pub key_evidence: Vec<EvidenceSummary>,

    pub influence_map: Vec<AgentInfluence>,
    pub unexpected_alignments: Vec<Alignment>,
    pub cherry_picks: Vec<CherryPick>,

    pub hidden_agendas: Vec<HiddenAgenda>,
    pub beneficiary_map: Vec<Beneficiary>,
    pub parallel_interests: Vec<ParallelInterest>,
    pub blind_spots: Vec<BlindSpot>,

    #[serde(default)]
    pub evolution: Vec<AgentEvolution>,

    pub areas_of_agreement: Vec<AgreementPoint>,
    pub areas_of_disagreement: Vec<DisagreementPoint>,
    pub key_tensions: Vec<String>,
    pub recommended_investigations: Vec<String>,
    pub agent_positions: Vec<AgentPosition>,
    /// Raw LLM JSON when the model produces non-standard output.
    /// Ensures no data is lost even if the model ignores our schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_llm_output: Option<serde_json::Value>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentEvolution {
    pub agent_id: String,
    pub agent_name: String,
    pub confidence_trajectory: Vec<f32>,
    pub net_shift: f32,
    pub pivot_cause: String,
    pub flexibility_score: f32,
    pub key_concessions: Vec<String>,
    pub bias_override: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EvidenceSummary { pub entity: String, pub fact: String, pub confidence: f32 }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentInfluence { pub agent_id: String, pub agent_name: String, pub bias_label: String, pub position_pushed: String, pub evidence_backed: bool, pub distortion_summary: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Alignment { pub agents: Vec<String>, pub common_position: String, pub reason: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CherryPick { pub agent_id: String, pub ignored_evidence: String, pub why_ignored: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HiddenAgenda { pub agent_id: String, pub agent_name: String, pub stated_position: String, pub underlying_interest: String, pub who_benefits: String, pub what_they_avoid: String, pub what_they_lose: String, pub second_order_effects: Vec<String> }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Beneficiary { pub position: String, pub beneficiaries: Vec<String>, pub mechanism: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ParallelInterest { pub agents: Vec<String>, pub surface_disagreement: String, pub hidden_alignment: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlindSpot { pub agent_id: String, pub topic_avoided: String, pub likely_reason: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgreementPoint { pub statement: String, pub agents: Vec<String>, pub confidence: f32 }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DisagreementPoint { pub statement: String, pub positions: Vec<(String, String)> }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentPosition { pub agent_id: String, pub agent_name: String, pub final_position: String, pub confidence: f32, pub evidence_count: usize }

// ── Selection (Layer 0: Select-then-Refine) ────────────────────────────

/// Result of the pre-synthesis selection step.
/// Scores each agent's final position and selects the strongest for refinement.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SelectionResult {
    /// Per-agent scoring.
    pub scores: Vec<AgentScore>,
    /// ID of the selected (winning) agent.
    pub selected_agent_id: String,
    /// Name of the selected agent.
    pub selected_agent_name: String,
    /// The selected agent's final position (verbatim).
    pub selected_position: String,
    /// Best counterpoints extracted from non-selected agents.
    pub best_counterpoints: Vec<Counterpoint>,
    /// Brief rationale for why this agent was selected.
    pub selection_rationale: String,
}

/// Scoring of a single agent's final position.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentScore {
    pub agent_id: String,
    pub agent_name: String,
    /// Evidence quality: how well-sourced and verifiable (0-10).
    pub evidence_quality: f32,
    /// Internal consistency: no contradictions within their argument (0-10).
    pub internal_consistency: f32,
    /// Counterargument handling: did they engage with opposing views? (0-10).
    pub counterargument_handling: f32,
    /// Confidence calibration: does stated confidence match evidence strength? (0-10).
    pub confidence_calibration: f32,
    /// Weighted total score.
    pub total_score: f32,
}

/// A counterpoint extracted from a non-selected agent that strengthens the final analysis.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Counterpoint {
    pub agent_id: String,
    pub agent_name: String,
    /// The counterpoint or insight worth preserving.
    pub point: String,
    /// Why this matters for the final analysis.
    pub relevance: String,
}

// ── Request/response types ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct StartRequest {
    pub topic: String,
    #[serde(default)]
    pub mode: DebateMode,
    #[serde(default = "default_agent_count")]
    pub agent_count: u8,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u8,
    /// Mode-specific input: desired outcome (RedTeam/OutcomeEng), actor list (Stakeholder),
    /// plan to test (Premortem), options A/B/C (DecisionMatrix).
    #[serde(default)]
    pub mode_input: Option<String>,
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

// ── Parsed turn metadata ────────────────────────────────────────────────

pub struct TurnMetadata {
    pub confidence: f32,
    pub agrees_with: Vec<String>,
    pub disagrees_with: Vec<String>,
    pub position_shift: String,
    pub concessions: Vec<String>,
}
