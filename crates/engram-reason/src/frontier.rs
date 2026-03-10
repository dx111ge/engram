/// Frontier node detection: entities with very few connections,
/// dangling at the edge of knowledge.

use engram_core::graph::NodeSnapshot;

use crate::types::{BlackArea, BlackAreaKind, DetectionConfig};

/// Detect frontier nodes — entities with <= max_edges total connections.
pub fn detect_frontier_nodes(
    nodes: &[NodeSnapshot],
    config: &DetectionConfig,
    now: i64,
) -> Vec<BlackArea> {
    let max = config.frontier_max_edges;

    nodes
        .iter()
        .filter(|n| {
            let total_edges = n.edge_out_count + n.edge_in_count;
            total_edges <= max && total_edges > 0 // must have at least 1 edge (0 = isolated, handled elsewhere)
        })
        .map(|n| {
            let total = n.edge_out_count + n.edge_in_count;
            // Severity: 1 edge = 0.8, 2 edges = 0.5
            let severity = if total <= 1 { 0.8 } else { 0.5 };

            BlackArea {
                kind: BlackAreaKind::FrontierNode,
                entities: vec![n.label.clone()],
                severity,
                suggested_queries: vec![
                    format!("{} relationships", n.label),
                    format!("{} connections", n.label),
                ],
                domain: n.node_type.clone(),
                detected_at: now,
            }
        })
        .collect()
}

/// Detect completely isolated nodes (0 edges).
pub fn detect_isolated_nodes(
    nodes: &[NodeSnapshot],
    now: i64,
) -> Vec<BlackArea> {
    nodes
        .iter()
        .filter(|n| n.edge_out_count == 0 && n.edge_in_count == 0)
        .map(|n| BlackArea {
            kind: BlackAreaKind::FrontierNode,
            entities: vec![n.label.clone()],
            severity: 0.9,
            suggested_queries: vec![format!("what is {} related to", n.label)],
            domain: n.node_type.clone(),
            detected_at: now,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn node(label: &str, out: u32, in_: u32) -> NodeSnapshot {
        NodeSnapshot {
            label: label.into(),
            node_type: None,
            confidence: 0.8,
            memory_tier: 0,
            properties: HashMap::new(),
            created_at: 0,
            updated_at: 0,
            edge_out_count: out,
            edge_in_count: in_,
        }
    }

    #[test]
    fn detects_frontier_nodes() {
        let nodes = vec![
            node("A", 1, 0), // frontier: 1 edge
            node("B", 5, 3), // well-connected
            node("C", 1, 1), // frontier: 2 edges
            node("D", 0, 0), // isolated (excluded from frontier)
        ];

        let config = DetectionConfig::default();
        let gaps = detect_frontier_nodes(&nodes, &config, 1000);
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].entities[0], "A");
        assert_eq!(gaps[1].entities[0], "C");
        assert!(gaps[0].severity > gaps[1].severity); // 1 edge > 2 edges severity
    }

    #[test]
    fn detects_isolated_nodes() {
        let nodes = vec![
            node("A", 0, 0),
            node("B", 1, 0),
        ];

        let gaps = detect_isolated_nodes(&nodes, 1000);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].entities[0], "A");
        assert_eq!(gaps[0].severity, 0.9);
    }

    #[test]
    fn no_frontiers_in_connected_graph() {
        let nodes = vec![
            node("A", 5, 3),
            node("B", 4, 6),
        ];

        let config = DetectionConfig::default();
        let gaps = detect_frontier_nodes(&nodes, &config, 1000);
        assert!(gaps.is_empty());
    }
}
