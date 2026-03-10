/// Confidence desert detection: clusters where all facts have low confidence.
///
/// A region of the graph where nothing is well-established indicates
/// an area that needs investigation or corroboration.

use std::collections::HashMap;

use engram_core::graph::NodeSnapshot;

use crate::types::{BlackArea, BlackAreaKind, DetectionConfig};

/// Detect confidence deserts — groups of connected nodes all below the threshold.
///
/// Groups nodes by type and checks if any type cluster has uniformly
/// low confidence.
pub fn detect_confidence_deserts(
    nodes: &[NodeSnapshot],
    config: &DetectionConfig,
    now: i64,
) -> Vec<BlackArea> {
    let threshold = config.confidence_desert_threshold;

    // Group by type
    let mut by_type: HashMap<String, Vec<&NodeSnapshot>> = HashMap::new();
    for node in nodes {
        let type_key = node.node_type.clone().unwrap_or_else(|| "untyped".to_string());
        by_type.entry(type_key).or_default().push(node);
    }

    let mut gaps = Vec::new();

    for (type_name, type_nodes) in &by_type {
        if type_nodes.len() < 3 {
            continue; // need at least 3 nodes to form a meaningful cluster
        }

        let avg_confidence: f32 = type_nodes.iter().map(|n| n.confidence).sum::<f32>()
            / type_nodes.len() as f32;

        if avg_confidence < threshold {
            let low_nodes: Vec<String> = type_nodes
                .iter()
                .filter(|n| n.confidence < threshold)
                .map(|n| n.label.clone())
                .collect();

            let severity = (threshold - avg_confidence) / threshold; // 0..1

            gaps.push(BlackArea {
                kind: BlackAreaKind::ConfidenceDesert,
                entities: low_nodes,
                severity: severity.max(0.3),
                suggested_queries: vec![
                    format!("{} verification", type_name),
                    format!("{} reliable sources", type_name),
                ],
                domain: Some(type_name.clone()),
                detected_at: now,
            });
        }
    }

    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn node_conf(label: &str, conf: f32, node_type: &str) -> NodeSnapshot {
        NodeSnapshot {
            label: label.into(),
            node_type: Some(node_type.into()),
            confidence: conf,
            memory_tier: 0,
            properties: HashMap::new(),
            created_at: 0,
            updated_at: 0,
            edge_out_count: 3,
            edge_in_count: 2,
        }
    }

    #[test]
    fn detects_low_confidence_cluster() {
        let nodes = vec![
            node_conf("A", 0.1, "suspect"),
            node_conf("B", 0.15, "suspect"),
            node_conf("C", 0.2, "suspect"),
            node_conf("D", 0.9, "verified"),
            node_conf("E", 0.85, "verified"),
            node_conf("F", 0.95, "verified"),
        ];

        let config = DetectionConfig::default();
        let gaps = detect_confidence_deserts(&nodes, &config, 1000);

        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].kind, BlackAreaKind::ConfidenceDesert);
        assert_eq!(gaps[0].domain.as_deref(), Some("suspect"));
    }

    #[test]
    fn no_desert_when_confidence_ok() {
        let nodes = vec![
            node_conf("A", 0.8, "good"),
            node_conf("B", 0.7, "good"),
            node_conf("C", 0.9, "good"),
        ];

        let config = DetectionConfig::default();
        let gaps = detect_confidence_deserts(&nodes, &config, 1000);
        assert!(gaps.is_empty());
    }

    #[test]
    fn ignores_small_clusters() {
        let nodes = vec![
            node_conf("A", 0.1, "tiny"),
            node_conf("B", 0.1, "tiny"),
        ];

        let config = DetectionConfig::default();
        let gaps = detect_confidence_deserts(&nodes, &config, 1000);
        assert!(gaps.is_empty()); // only 2 nodes, below threshold
    }
}
