/// Core types for the assessment system.

use serde::{Deserialize, Serialize};

/// A single score history point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorePoint {
    /// Unix timestamp in seconds.
    pub timestamp: i64,
    /// Probability in [0.05, 0.95].
    pub probability: f32,
    /// Delta from previous score point.
    pub shift: f32,
    /// What triggered this score point.
    pub trigger: ScoreTrigger,
    /// Human-readable explanation of the change.
    pub reason: String,
    /// Impact chain labels (for deep causal chains).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,
}

/// What caused a score point to be recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScoreTrigger {
    Created,
    Manual,
    EvidenceAdded { node_id: u64 },
    EvidenceRemoved { node_id: u64 },
    GraphPropagation { source_node_id: u64 },
    Decay,
}

/// Per-assessment record stored in the sidecar file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentRecord {
    /// Matches the graph node label (e.g. "Assessment:nvidia-stock-gt-200").
    pub label: String,
    /// Graph node ID.
    pub node_id: u64,
    /// Append-only time-series of score changes.
    pub history: Vec<ScorePoint>,
    /// Cached confidence values for supporting evidence.
    pub evidence_for: Vec<f32>,
    /// Cached confidence values for contradicting evidence.
    pub evidence_against: Vec<f32>,
}

/// Request to create an assessment.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAssessmentRequest {
    /// Short title (e.g. "NVIDIA stock > $200 by Q3 2026").
    pub title: String,
    /// Optional longer description.
    pub description: Option<String>,
    /// Category (e.g. "financial", "geopolitical", "technical").
    pub category: Option<String>,
    /// Time horizon (e.g. "Q3 2026", "by end of year").
    pub timeframe: Option<String>,
    /// Initial probability [0.05, 0.95], defaults to 0.50.
    pub initial_probability: Option<f32>,
    /// Entity labels to watch.
    #[serde(default)]
    pub watches: Vec<String>,
}

/// Request to update an assessment.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAssessmentRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub timeframe: Option<String>,
    /// Manual probability override [0.05, 0.95].
    pub probability: Option<f32>,
}

/// Request to add evidence.
#[derive(Debug, Clone, Deserialize)]
pub struct AddEvidenceRequest {
    /// Label of the evidence node in the graph.
    pub node_label: String,
    /// "supports" or "contradicts".
    pub direction: String,
    /// Optional confidence override (defaults to node's confidence).
    pub confidence: Option<f32>,
}

/// Request to add a watch.
#[derive(Debug, Clone, Deserialize)]
pub struct AddWatchRequest {
    pub entity_label: String,
}

/// Summary response for an assessment.
#[derive(Debug, Clone, Serialize)]
pub struct AssessmentSummary {
    pub label: String,
    pub title: String,
    pub category: String,
    pub status: String,
    pub description: String,
    pub timeframe: String,
    pub current_probability: f32,
    pub last_evaluated: i64,
    pub evidence_count: usize,
    pub watch_count: usize,
}

/// Full detail response for an assessment.
#[derive(Debug, Clone, Serialize)]
pub struct AssessmentDetail {
    pub label: String,
    pub title: String,
    pub category: String,
    pub status: String,
    pub description: String,
    pub timeframe: String,
    pub current_probability: f32,
    pub last_evaluated: i64,
    pub history: Vec<ScorePoint>,
    pub evidence_for: Vec<EvidenceItem>,
    pub evidence_against: Vec<EvidenceItem>,
    pub watches: Vec<String>,
}

/// An evidence item with source info.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceItem {
    pub node_label: String,
    pub confidence: f32,
    pub edge_id: Option<u64>,
}

/// Result of an evaluation run.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResult {
    pub label: String,
    pub old_probability: f32,
    pub new_probability: f32,
    pub shift: f32,
    pub evidence_added: usize,
    pub paths_found: Vec<Vec<String>>,
}

/// A matched assessment from BFS pathfinding.
#[derive(Debug, Clone)]
pub struct AffectedAssessment {
    pub label: String,
    pub path: Vec<u64>,
    pub path_labels: Vec<String>,
    pub accumulated_confidence: f32,
    pub supports: bool,
}
