/// Coordinated cluster detection: groups of entities with dense internal
/// connections, sparse external connections, low author trust, and temporal
/// synchronization.
///
/// Signals potential influence operations, bot farms, or coordinated
/// information campaigns.

use std::collections::{HashMap, HashSet};

use engram_core::graph::{Graph, NodeSnapshot};

use crate::types::{BlackArea, BlackAreaKind, DetectionConfig};

/// Detect coordinated clusters.
///
/// Groups nodes by domain, then for each group checks:
/// 1. Internal/external edge ratio
/// 2. Average author trust (via Source/Author trust nodes)
/// 3. Temporal synchronization (how tightly clustered creation times are)
pub fn detect_coordinated_clusters(
    graph: &Graph,
    nodes: &[NodeSnapshot],
    config: &DetectionConfig,
    now: i64,
) -> Vec<BlackArea> {
    // Group by domain property
    let mut by_domain: HashMap<String, Vec<&NodeSnapshot>> = HashMap::new();
    for node in nodes {
        if let Some(domain) = node.properties.get("domain") {
            by_domain.entry(domain.clone()).or_default().push(node);
        }
    }

    let mut gaps = Vec::new();

    for (domain, cluster_nodes) in &by_domain {
        if cluster_nodes.len() < config.coordinated_min_size {
            continue;
        }

        let cluster_labels: HashSet<&str> = cluster_nodes.iter().map(|n| n.label.as_str()).collect();

        // Count internal vs external edges
        let mut internal_edges = 0u32;
        let mut external_edges = 0u32;

        for node in cluster_nodes {
            if let Ok(edges) = graph.edges_from(&node.label) {
                for edge in &edges {
                    if cluster_labels.contains(edge.to.as_str()) {
                        internal_edges += 1;
                    } else {
                        external_edges += 1;
                    }
                }
            }
        }

        // Check edge ratio
        let edge_ratio = if external_edges == 0 {
            internal_edges as f32
        } else {
            internal_edges as f32 / external_edges as f32
        };

        if edge_ratio < config.coordinated_edge_ratio {
            continue;
        }

        // Check temporal synchronization
        let timestamps: Vec<i64> = cluster_nodes
            .iter()
            .filter(|n| n.created_at > 0)
            .map(|n| n.created_at)
            .collect();
        let temporal_sync = temporal_sync_score(&timestamps);

        // Check average author trust (look for low-trust Source: nodes)
        let avg_confidence: f32 = cluster_nodes.iter().map(|n| n.confidence).sum::<f32>()
            / cluster_nodes.len() as f32;

        // Combined severity: high ratio * temporal sync * (1 - avg_confidence)
        let severity = ((edge_ratio / 10.0) * temporal_sync * (1.0 - avg_confidence))
            .clamp(0.0, 1.0);

        if severity > 0.3 {
            let labels: Vec<String> = cluster_nodes.iter().map(|n| n.label.clone()).collect();
            gaps.push(BlackArea {
                kind: BlackAreaKind::CoordinatedCluster,
                entities: labels,
                severity,
                suggested_queries: vec![
                    format!("{} source verification", domain),
                    format!("{} author credibility", domain),
                ],
                domain: Some(domain.clone()),
                detected_at: now,
            });
        }
    }

    gaps
}

/// Calculate temporal synchronization score.
/// Returns 0.0 (spread out) to 1.0 (all created at the same time).
fn temporal_sync_score(timestamps: &[i64]) -> f32 {
    if timestamps.len() < 2 {
        return 0.0;
    }

    let mean = timestamps.iter().sum::<i64>() as f64 / timestamps.len() as f64;
    let variance = timestamps
        .iter()
        .map(|t| {
            let diff = *t as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / timestamps.len() as f64;

    let std_dev = variance.sqrt();

    // Normalize: if std_dev < 1 hour (in nanos), very synchronized
    let hour_nanos = 3_600_000_000_000.0;
    let day_nanos = 86_400_000_000_000.0;

    if std_dev < hour_nanos {
        1.0
    } else if std_dev > day_nanos * 30.0 {
        0.0
    } else {
        (1.0 - (std_dev / (day_nanos * 30.0))) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_sync_identical() {
        let ts = vec![1000, 1000, 1000, 1000];
        assert_eq!(temporal_sync_score(&ts), 1.0);
    }

    #[test]
    fn temporal_sync_spread() {
        let day = 86_400_000_000_000i64;
        let ts: Vec<i64> = (0..10).map(|i| i * day * 10).collect();
        let score = temporal_sync_score(&ts);
        assert!(score < 0.5);
    }

    #[test]
    fn temporal_sync_single() {
        assert_eq!(temporal_sync_score(&[1000]), 0.0);
    }
}
