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

/// A single piece of evidence linked to an assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    /// Label of the evidence node in the graph.
    pub node_label: String,
    /// Confidence value at time of addition.
    pub confidence: f32,
    /// true = supports hypothesis, false = contradicts.
    pub supports: bool,
    /// When this evidence was added (unix seconds).
    pub added_at: i64,
    /// Source of this evidence (e.g. "kb:wikidata", "user", "llm", "graph_propagation").
    #[serde(default)]
    pub source: String,
    /// Optional graph edge ID linking evidence to assessment.
    #[serde(default)]
    pub edge_id: Option<u64>,
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
    /// Structured evidence entries (replaces flat f32 arrays).
    #[serde(default)]
    pub evidence: Vec<EvidenceEntry>,
    /// Success criteria -- what would confirm or deny this hypothesis.
    #[serde(default)]
    pub success_criteria: Option<String>,
    /// Tags for grouping/filtering.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Resolution state: active, confirmed, denied, inconclusive, superseded.
    #[serde(default = "default_resolution")]
    pub resolution: String,

    // Legacy fields (backward compat with old .brain.assessments files)
    /// Deprecated: use `evidence` instead. Kept for migration.
    #[serde(default, skip_serializing)]
    pub evidence_for: Vec<f32>,
    /// Deprecated: use `evidence` instead. Kept for migration.
    #[serde(default, skip_serializing)]
    pub evidence_against: Vec<f32>,
}

fn default_resolution() -> String { "active".to_string() }

impl AssessmentRecord {
    /// Migrate legacy flat arrays to structured evidence entries.
    /// Called once on load if old format detected.
    pub fn migrate_legacy_evidence(&mut self) {
        if self.evidence.is_empty() && (!self.evidence_for.is_empty() || !self.evidence_against.is_empty()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            for (i, &conf) in self.evidence_for.iter().enumerate() {
                self.evidence.push(EvidenceEntry {
                    node_label: format!("legacy_evidence_{}", i),
                    confidence: conf,
                    supports: true,
                    added_at: now,
                    source: "migrated".to_string(),
                    edge_id: None,
                });
            }
            for (i, &conf) in self.evidence_against.iter().enumerate() {
                self.evidence.push(EvidenceEntry {
                    node_label: format!("legacy_contra_{}", i),
                    confidence: conf,
                    supports: false,
                    added_at: now,
                    source: "migrated".to_string(),
                    edge_id: None,
                });
            }
            self.evidence_for.clear();
            self.evidence_against.clear();
        }
    }

    /// Get supporting evidence entries.
    pub fn evidence_for_entries(&self) -> Vec<&EvidenceEntry> {
        self.evidence.iter().filter(|e| e.supports).collect()
    }

    /// Get contradicting evidence entries.
    pub fn evidence_against_entries(&self) -> Vec<&EvidenceEntry> {
        self.evidence.iter().filter(|e| !e.supports).collect()
    }
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
    /// What would confirm or deny this hypothesis.
    pub success_criteria: Option<String>,
    /// Tags for grouping/filtering.
    #[serde(default)]
    pub tags: Vec<String>,
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
    pub success_criteria: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Resolution state: active, confirmed, denied, inconclusive, superseded.
    pub resolution: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_shift: Option<f32>,
    pub resolution: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<String>,
    pub resolution: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// An evidence item with source info.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceItem {
    pub node_label: String,
    pub confidence: f32,
    pub edge_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
