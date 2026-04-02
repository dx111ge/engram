/// Assessment evaluation engine.
///
/// Handles:
/// - Adaptive BFS pathfinding to find affected assessments
/// - Bayesian log-odds probability with time decay
/// - Structured evidence management via EvidenceEntry
/// - EventBus subscription for auto-evaluation

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use engram_core::graph::Graph;

use crate::store::AssessmentStore;
use crate::types::*;

/// Half-life for time decay in seconds (30 days).
const HALF_LIFE_SECS: f64 = 30.0 * 24.0 * 3600.0;

/// ln(2) constant for decay calculation.
const LN2: f64 = std::f64::consts::LN_2;

/// Current unix timestamp in seconds.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Bayesian log-odds probability calculation with time decay.
///
/// - Starts from prior log-odds = 0 (probability 0.50)
/// - Each evidence: strength = confidence^2 * time_decay
/// - Time decay: exp(-(age_secs / half_life) * ln2), half_life = 30 days
/// - Supporting evidence adds to log-odds, contradicting subtracts
/// - Final: p = 1 / (1 + exp(-log_odds)), clamped [0.05, 0.95]
fn calculate_probability(evidence: &[EvidenceEntry]) -> f32 {
    if evidence.is_empty() {
        return 0.50;
    }

    let now = now_secs();
    let mut log_odds: f64 = 0.0;

    for entry in evidence {
        let age_secs = (now - entry.added_at).max(0) as f64;
        let decay = (-age_secs / HALF_LIFE_SECS * LN2).exp();
        let strength = (entry.confidence as f64) * (entry.confidence as f64) * decay;

        if entry.supports {
            log_odds += strength;
        } else {
            log_odds -= strength;
        }
    }

    let p = 1.0 / (1.0 + (-log_odds).exp());
    (p as f32).clamp(0.05, 0.95)
}

/// Legacy probability calculation for backward compatibility.
/// Uses the old weighted-average approach without time decay.
fn calculate_probability_legacy(evidence_for: &[f32], evidence_against: &[f32]) -> f32 {
    if evidence_for.is_empty() && evidence_against.is_empty() {
        return 0.50;
    }

    let weighted_for = if evidence_for.is_empty() {
        0.0
    } else {
        let total: f32 = evidence_for.iter().sum();
        evidence_for.iter().map(|c| c * c).sum::<f32>() / total
    };

    let weighted_against = if evidence_against.is_empty() {
        0.0
    } else {
        let total: f32 = evidence_against.iter().sum();
        evidence_against.iter().map(|c| c * c).sum::<f32>() / total
    };

    let n_total = evidence_for.len() + evidence_against.len();
    let discount = if n_total > 0 {
        evidence_against.len() as f32 / n_total as f32
    } else {
        0.0
    };

    let prob = weighted_for * (1.0 - weighted_against * discount);
    prob.clamp(0.05, 0.95)
}

/// Find assessments affected by a new/updated node via adaptive BFS.
///
/// Expands in tiers: [4, 8, 12]. At each reached node, checks for incoming
/// "watches" edges from assessment nodes.
pub fn find_affected_assessments(
    graph: &Graph,
    source_label: &str,
) -> Vec<AffectedAssessment> {
    for max_depth in [4u32, 8, 12] {
        let matches = bfs_find_assessments(graph, source_label, max_depth);
        if !matches.is_empty() {
            return matches;
        }
    }
    vec![]
}

fn bfs_find_assessments(
    graph: &Graph,
    source_label: &str,
    max_depth: u32,
) -> Vec<AffectedAssessment> {
    let mut visited: HashSet<String> = HashSet::new();
    // Queue: (label, depth, path_labels, accumulated_confidence)
    let mut queue: VecDeque<(String, u32, Vec<String>, f32)> = VecDeque::new();
    let mut results = Vec::new();

    visited.insert(source_label.to_string());
    queue.push_back((source_label.to_string(), 0, vec![source_label.to_string()], 1.0));

    while let Some((label, depth, path, conf)) = queue.pop_front() {
        // Check if this node is watched by any assessment
        if let Ok(edges_to) = graph.edges_to(&label) {
            for edge in &edges_to {
                if edge.relationship == "watches" && edge.from.starts_with("Assessment:") {
                    results.push(AffectedAssessment {
                        label: edge.from.clone(),
                        path: vec![], // no IDs available
                        path_labels: path.clone(),
                        accumulated_confidence: conf * edge.confidence,
                        supports: true,
                    });
                }
            }
        }

        if depth >= max_depth {
            continue;
        }

        let decay = 0.9_f32.powi(depth as i32 + 1);

        // Expand outgoing edges
        if let Ok(edges_out) = graph.edges_from(&label) {
            for edge in &edges_out {
                if !visited.contains(&edge.to) {
                    visited.insert(edge.to.clone());
                    let mut new_path = path.clone();
                    new_path.push(edge.to.clone());
                    queue.push_back((
                        edge.to.clone(),
                        depth + 1,
                        new_path,
                        conf * edge.confidence * decay,
                    ));
                }
            }
        }

        // Expand incoming edges (bidirectional BFS)
        if let Ok(edges_in) = graph.edges_to(&label) {
            for edge in &edges_in {
                if edge.relationship == "watches" {
                    continue; // Don't follow watch edges for traversal
                }
                if !visited.contains(&edge.from) {
                    visited.insert(edge.from.clone());
                    let mut new_path = path.clone();
                    new_path.push(edge.from.clone());
                    queue.push_back((
                        edge.from.clone(),
                        depth + 1,
                        new_path,
                        conf * edge.confidence * decay,
                    ));
                }
            }
        }
    }

    results
}

/// Recalculate probability for an assessment based on its structured evidence.
pub fn recalculate_probability(record: &AssessmentRecord) -> f32 {
    calculate_probability(&record.evidence)
}

/// Add structured evidence to an assessment and recalculate probability.
/// Returns the new ScorePoint.
pub fn add_evidence(
    record: &mut AssessmentRecord,
    confidence: f32,
    supports: bool,
    trigger: ScoreTrigger,
    reason: String,
    path: Option<Vec<String>>,
    node_label: &str,
    source: &str,
) -> ScorePoint {
    let now = now_secs();
    let old_prob = calculate_probability(&record.evidence);

    // Track pending auto-evidence for stale alerts
    if matches!(trigger, ScoreTrigger::GraphPropagation { .. }) {
        record.pending_count = record.pending_count.saturating_add(1);
    }

    record.evidence.push(EvidenceEntry {
        node_label: node_label.to_string(),
        confidence,
        supports,
        added_at: now,
        source: source.to_string(),
        edge_id: None,
    });

    let new_prob = calculate_probability(&record.evidence);
    let shift = new_prob - old_prob;

    let point = ScorePoint {
        timestamp: now,
        probability: new_prob,
        shift,
        trigger,
        reason,
        path,
    };

    record.history.push(point.clone());
    point
}

/// Full re-evaluation: recalculate probability from structured evidence.
/// Resets pending_count (user has reviewed). Returns the new ScorePoint.
pub fn evaluate(record: &mut AssessmentRecord) -> ScorePoint {
    let old_prob = record.history.last()
        .map(|p| p.probability)
        .unwrap_or(0.50);

    let new_prob = calculate_probability(&record.evidence);
    let shift = new_prob - old_prob;

    let now = now_secs();

    // Reset pending auto-evidence counter -- user has reviewed
    record.pending_count = 0;

    let point = ScorePoint {
        timestamp: now,
        probability: new_prob,
        shift,
        trigger: ScoreTrigger::Manual,
        reason: "Manual re-evaluation".to_string(),
        path: None,
    };

    record.history.push(point.clone());
    point
}

/// Assessment engine: wires EventBus to assessment auto-evaluation.
pub struct AssessmentEngine {
    pub store: Arc<RwLock<AssessmentStore>>,
    pub graph: Arc<RwLock<Graph>>,
}

impl AssessmentEngine {
    pub fn new(store: Arc<RwLock<AssessmentStore>>, graph: Arc<RwLock<Graph>>) -> Self {
        Self { store, graph }
    }

    /// Process a FactStored event: find affected assessments and update them.
    pub fn on_fact_stored(&self, _node_id: u64, label: &str, _confidence: f32) -> Vec<EvaluationResult> {
        let graph = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };

        let affected = find_affected_assessments(&graph, label);
        drop(graph);

        if affected.is_empty() {
            return vec![];
        }

        let mut store = match self.store.write() {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let mut results = Vec::new();

        for assessment in affected {
            if let Some(record) = store.get_mut(&assessment.label) {
                let old_prob = calculate_probability(&record.evidence);

                let point = add_evidence(
                    record,
                    assessment.accumulated_confidence,
                    assessment.supports,
                    ScoreTrigger::GraphPropagation { source_node_id: 0 },
                    format!("New fact '{}' propagated via {} hops", label, assessment.path_labels.len()),
                    Some(assessment.path_labels.clone()),
                    label,
                    "graph_propagation",
                );

                results.push(EvaluationResult {
                    label: assessment.label.clone(),
                    old_probability: old_prob,
                    new_probability: point.probability,
                    shift: point.shift,
                    evidence_added: 1,
                    paths_found: vec![assessment.path_labels],
                });
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_evidence(confidence: f32, supports: bool) -> EvidenceEntry {
        EvidenceEntry {
            node_label: "test_node".to_string(),
            confidence,
            supports,
            added_at: now_secs(),
            source: "test".to_string(),
            edge_id: None,
        }
    }

    fn make_record() -> AssessmentRecord {
        AssessmentRecord {
            label: "Assessment:test".to_string(),
            node_id: 1,
            history: vec![],
            evidence: vec![],
            success_criteria: None,
            tags: vec![],
            resolution: "active".to_string(),
            pending_count: 0,
            evidence_for: vec![],
            evidence_against: vec![],
        }
    }

    #[test]
    fn probability_no_evidence() {
        assert!((calculate_probability(&[]) - 0.50).abs() < 0.01);
    }

    #[test]
    fn probability_strong_support() {
        let evidence = vec![
            make_evidence(0.90, true),
            make_evidence(0.88, true),
            make_evidence(0.85, true),
        ];
        let p = calculate_probability(&evidence);
        assert!(p > 0.60, "expected > 0.60, got {}", p);
    }

    #[test]
    fn probability_contradicting_lowers() {
        let support_only = vec![
            make_evidence(0.85, true),
            make_evidence(0.80, true),
        ];
        let with_contra = vec![
            make_evidence(0.85, true),
            make_evidence(0.80, true),
            make_evidence(0.90, false),
        ];
        let p_support = calculate_probability(&support_only);
        let p_mixed = calculate_probability(&with_contra);
        assert!(p_mixed < p_support, "contradicting evidence should lower probability: {} vs {}", p_mixed, p_support);
    }

    #[test]
    fn probability_clamped() {
        // Even with extreme evidence, should stay in [0.05, 0.95]
        let extreme_for: Vec<EvidenceEntry> = (0..20).map(|_| make_evidence(0.99, true)).collect();
        let p = calculate_probability(&extreme_for);
        assert!(p <= 0.95, "should be clamped to 0.95, got {}", p);
        assert!(p >= 0.05);

        let extreme_against: Vec<EvidenceEntry> = (0..20).map(|_| make_evidence(0.99, false)).collect();
        let p = calculate_probability(&extreme_against);
        assert!(p >= 0.05, "should be clamped to 0.05, got {}", p);
        assert!(p <= 0.95);
    }

    #[test]
    fn time_decay_reduces_old_evidence() {
        let now = now_secs();
        let fresh = vec![EvidenceEntry {
            node_label: "fresh".to_string(),
            confidence: 0.90,
            supports: true,
            added_at: now,
            source: "test".to_string(),
            edge_id: None,
        }];
        let old = vec![EvidenceEntry {
            node_label: "old".to_string(),
            confidence: 0.90,
            supports: true,
            added_at: now - 90 * 24 * 3600, // 90 days ago
            source: "test".to_string(),
            edge_id: None,
        }];
        let p_fresh = calculate_probability(&fresh);
        let p_old = calculate_probability(&old);
        assert!(p_fresh > p_old, "fresh evidence should have more impact: {} vs {}", p_fresh, p_old);
    }

    #[test]
    fn legacy_probability_no_evidence() {
        assert!((calculate_probability_legacy(&[], &[]) - 0.50).abs() < 0.01);
    }

    #[test]
    fn legacy_probability_strong_support() {
        let p = calculate_probability_legacy(&[0.90, 0.88, 0.85], &[]);
        assert!(p > 0.60);
    }

    #[test]
    fn add_evidence_updates_record() {
        let mut record = make_record();

        let point = add_evidence(
            &mut record,
            0.85,
            true,
            ScoreTrigger::Manual,
            "Test evidence".to_string(),
            None,
            "test_node",
            "user",
        );

        assert_eq!(record.evidence.len(), 1);
        assert!(record.evidence[0].supports);
        assert_eq!(record.evidence[0].node_label, "test_node");
        assert_eq!(record.evidence[0].source, "user");
        assert!(point.probability > 0.05);
        assert_eq!(record.history.len(), 1);
    }

    #[test]
    fn evaluate_uses_structured_evidence() {
        let mut record = make_record();
        record.evidence.push(make_evidence(0.90, true));
        record.evidence.push(make_evidence(0.85, true));

        let point = evaluate(&mut record);
        assert!(point.probability > 0.50, "should be above prior with supporting evidence");
        assert_eq!(record.history.len(), 1);
    }
}
