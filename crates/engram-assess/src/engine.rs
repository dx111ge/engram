/// Assessment evaluation engine.
///
/// Handles:
/// - Adaptive BFS pathfinding to find affected assessments
/// - Probability recalculation using engram-intel probability module
/// - Evidence management (add/remove supporting/contradicting evidence)
/// - EventBus subscription for auto-evaluation

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use engram_core::graph::Graph;

use crate::store::AssessmentStore;
use crate::types::*;

/// Bayesian probability calculation (mirrors engram-intel::probability).
/// Inlined here to avoid WASM crate dependency.
fn calculate_probability(evidence_for: &[f32], evidence_against: &[f32]) -> f32 {
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

fn _calculate_with_shift(
    existing_for: &[f32],
    existing_against: &[f32],
    new_for: &[f32],
    new_against: &[f32],
) -> (f32, f32) {
    let old_prob = calculate_probability(existing_for, existing_against);
    let mut all_for = existing_for.to_vec();
    all_for.extend_from_slice(new_for);
    let mut all_against = existing_against.to_vec();
    all_against.extend_from_slice(new_against);
    let new_prob = calculate_probability(&all_for, &all_against);
    (new_prob, new_prob - old_prob)
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

/// Recalculate probability for an assessment based on its evidence arrays.
pub fn recalculate_probability(record: &AssessmentRecord) -> f32 {
    calculate_probability(&record.evidence_for, &record.evidence_against)
}

/// Add evidence to an assessment and recalculate probability.
/// Returns the new ScorePoint.
pub fn add_evidence(
    record: &mut AssessmentRecord,
    confidence: f32,
    supports: bool,
    trigger: ScoreTrigger,
    reason: String,
    path: Option<Vec<String>>,
) -> ScorePoint {
    let old_prob = calculate_probability(&record.evidence_for, &record.evidence_against);

    if supports {
        record.evidence_for.push(confidence);
    } else {
        record.evidence_against.push(confidence);
    }

    let new_prob = calculate_probability(&record.evidence_for, &record.evidence_against);
    let shift = new_prob - old_prob;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

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

/// Full re-evaluation: recalculate probability from evidence arrays.
/// Returns the new ScorePoint.
pub fn evaluate(record: &mut AssessmentRecord) -> ScorePoint {
    let old_prob = record.history.last()
        .map(|p| p.probability)
        .unwrap_or(0.50);

    let new_prob = calculate_probability(&record.evidence_for, &record.evidence_against);
    let shift = new_prob - old_prob;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

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
                let old_prob = calculate_probability(&record.evidence_for, &record.evidence_against);

                let point = add_evidence(
                    record,
                    assessment.accumulated_confidence,
                    assessment.supports,
                    ScoreTrigger::GraphPropagation { source_node_id: 0 },
                    format!("New fact '{}' propagated via {} hops", label, assessment.path_labels.len()),
                    Some(assessment.path_labels.clone()),
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

    #[test]
    fn probability_no_evidence() {
        assert!((calculate_probability(&[], &[]) - 0.50).abs() < 0.01);
    }

    #[test]
    fn probability_strong_support() {
        let p = calculate_probability(&[0.90, 0.88, 0.85], &[]);
        assert!(p > 0.60);
    }

    #[test]
    fn probability_shift() {
        let (new_p, shift) = _calculate_with_shift(
            &[0.85, 0.80],
            &[0.90],
            &[0.78],
            &[],
        );
        assert!(shift > 0.0);
        assert!(new_p > calculate_probability(&[0.85, 0.80], &[0.90]));
    }

    #[test]
    fn add_evidence_updates_record() {
        let mut record = AssessmentRecord {
            label: "Assessment:test".to_string(),
            node_id: 1,
            history: vec![],
            evidence_for: vec![],
            evidence_against: vec![],
        };

        let point = add_evidence(
            &mut record,
            0.85,
            true,
            ScoreTrigger::Manual,
            "Test evidence".to_string(),
            None,
        );

        assert_eq!(record.evidence_for.len(), 1);
        assert!(point.probability > 0.05);
        assert_eq!(record.history.len(), 1);
    }
}
