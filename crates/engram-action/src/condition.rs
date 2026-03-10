/// Condition evaluator: checks rule conditions against graph state.

use engram_core::events::GraphEvent;
use engram_core::graph::Graph;

use crate::types::{Condition, ConditionResult};

/// Evaluate all conditions for a rule against a graph event.
///
/// Returns true only if ALL conditions pass (AND logic).
pub fn evaluate_conditions(
    conditions: &[Condition],
    event: &GraphEvent,
    graph: &Graph,
) -> (bool, Vec<ConditionResult>) {
    if conditions.is_empty() {
        return (true, vec![]);
    }

    let mut results = Vec::with_capacity(conditions.len());
    let mut all_pass = true;

    for condition in conditions {
        let (passed, detail) = evaluate_single(condition, event, graph);
        if !passed {
            all_pass = false;
        }
        results.push(ConditionResult {
            condition: format!("{:?}", condition),
            passed,
            detail,
        });
    }

    (all_pass, results)
}

fn evaluate_single(
    condition: &Condition,
    event: &GraphEvent,
    graph: &Graph,
) -> (bool, Option<String>) {
    match condition {
        Condition::ConfidenceAbove { threshold } => {
            let confidence = event_confidence(event);
            let passed = confidence.is_some_and(|c| c > *threshold);
            (passed, Some(format!("confidence={:?}, threshold={}", confidence, threshold)))
        }

        Condition::ConfidenceBelow { threshold } => {
            let confidence = event_confidence(event);
            let passed = confidence.is_some_and(|c| c < *threshold);
            (passed, Some(format!("confidence={:?}, threshold={}", confidence, threshold)))
        }

        Condition::HasProperty { key, value } => {
            let label = event_label(event);
            if let Some(label) = label {
                let prop = graph.get_property(&label, key);
                let passed = match (prop, value) {
                    (Ok(Some(v)), Some(expected)) => v == *expected,
                    (Ok(Some(_)), None) => true, // just check existence
                    _ => false,
                };
                (passed, Some(format!("key={}, label={}", key, label)))
            } else {
                (false, Some("no label in event".into()))
            }
        }

        Condition::HasType { entity_type } => {
            let label = event_label(event);
            if let Some(label) = label {
                let node_type = graph.get_node_type(&label);
                let passed = node_type
                    .as_ref()
                    .is_some_and(|t| t.eq_ignore_ascii_case(entity_type));
                (passed, Some(format!("type={:?}, expected={}", node_type, entity_type)))
            } else {
                (false, Some("no label in event".into()))
            }
        }

        Condition::HasEdge { rel_type, direction } => {
            let label = event_label(event);
            if let Some(label) = label {
                let has = check_edge(graph, &label, rel_type, direction.as_deref());
                (has, Some(format!("rel_type={}, direction={:?}", rel_type, direction)))
            } else {
                (false, Some("no label in event".into()))
            }
        }

        Condition::Expression { left, op, right } => {
            // Simple string comparison
            let passed = match op.as_str() {
                "==" | "eq" => left == right,
                "!=" | "ne" => left != right,
                "contains" => left.contains(right.as_str()),
                _ => false,
            };
            (passed, Some(format!("{} {} {}", left, op, right)))
        }
    }
}

/// Extract confidence from a graph event.
fn event_confidence(event: &GraphEvent) -> Option<f32> {
    match event {
        GraphEvent::FactStored { confidence, .. } => Some(*confidence),
        GraphEvent::FactUpdated { new_confidence, .. } => Some(*new_confidence),
        GraphEvent::EdgeCreated { confidence, .. } => Some(*confidence),
        GraphEvent::ThresholdCrossed { new_confidence, .. } => Some(*new_confidence),
        _ => None,
    }
}

/// Extract label from a graph event.
fn event_label(event: &GraphEvent) -> Option<String> {
    match event {
        GraphEvent::FactStored { label, .. } => Some(label.to_string()),
        GraphEvent::FactUpdated { label, .. } => Some(label.to_string()),
        GraphEvent::FactDeleted { label, .. } => Some(label.to_string()),
        GraphEvent::PropertyChanged { label, .. } => Some(label.to_string()),
        GraphEvent::TierChanged { label, .. } => Some(label.to_string()),
        GraphEvent::ThresholdCrossed { label, .. } => Some(label.to_string()),
        _ => None,
    }
}

/// Check if a node has an edge of the given type.
fn check_edge(graph: &Graph, label: &str, rel_type: &str, direction: Option<&str>) -> bool {
    let check_out = direction.is_none() || direction == Some("out") || direction == Some("both");
    let check_in = direction.is_none() || direction == Some("in") || direction == Some("both");

    if check_out {
        if let Ok(edges) = graph.edges_from(label) {
            if edges.iter().any(|e| e.relationship == rel_type) {
                return true;
            }
        }
    }

    if check_in {
        if let Ok(edges) = graph.edges_to(label) {
            if edges.iter().any(|e| e.relationship == rel_type) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_event(label: &str, confidence: f32) -> GraphEvent {
        GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from(label),
            confidence,
            source: Arc::from("test"),
            entity_type: None,
        }
    }

    fn test_graph() -> (tempfile::TempDir, Graph) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = Graph::create(&path).unwrap();
        (dir, graph)
    }

    #[test]
    fn confidence_above_passes() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.8);
        let conditions = vec![Condition::ConfidenceAbove { threshold: 0.5 }];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(passed);
    }

    #[test]
    fn confidence_above_fails() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.3);
        let conditions = vec![Condition::ConfidenceAbove { threshold: 0.5 }];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(!passed);
    }

    #[test]
    fn confidence_below_passes() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.2);
        let conditions = vec![Condition::ConfidenceBelow { threshold: 0.5 }];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(passed);
    }

    #[test]
    fn empty_conditions_pass() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.5);
        let (passed, results) = evaluate_conditions(&[], &event, &graph);
        assert!(passed);
        assert!(results.is_empty());
    }

    #[test]
    fn multiple_conditions_all_must_pass() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.7);
        let conditions = vec![
            Condition::ConfidenceAbove { threshold: 0.5 },
            Condition::ConfidenceBelow { threshold: 0.9 },
        ];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(passed);
    }

    #[test]
    fn one_failing_condition_fails_all() {
        let (_dir, graph) = test_graph();
        let event = make_event("Apple", 0.3);
        let conditions = vec![
            Condition::ConfidenceAbove { threshold: 0.5 }, // fails
            Condition::ConfidenceBelow { threshold: 0.9 }, // passes
        ];
        let (passed, results) = evaluate_conditions(&conditions, &event, &graph);
        assert!(!passed);
        assert!(!results[0].passed);
        assert!(results[1].passed);
    }

    #[test]
    fn has_property_condition() {
        let (_dir, mut graph) = test_graph();
        let prov = engram_core::graph::Provenance::user("test");
        graph.store("Apple", &prov).unwrap();
        graph.set_property("Apple", "industry", "tech").unwrap();

        let event = make_event("Apple", 0.8);
        let conditions = vec![Condition::HasProperty {
            key: "industry".into(),
            value: Some("tech".into()),
        }];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(passed);
    }

    #[test]
    fn expression_condition() {
        let (_dir, graph) = test_graph();
        let event = make_event("test", 0.5);

        let conditions = vec![Condition::Expression {
            left: "hello".into(),
            op: "contains".into(),
            right: "ell".into(),
        }];
        let (passed, _) = evaluate_conditions(&conditions, &event, &graph);
        assert!(passed);
    }
}
