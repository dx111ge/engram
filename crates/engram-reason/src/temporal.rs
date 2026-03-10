/// Temporal gap detection: entities that haven't been updated recently.
///
/// If facts about an entity stop at a certain date with nothing after,
/// this indicates a temporal blind spot.

use engram_core::graph::NodeSnapshot;

use crate::types::{BlackArea, BlackAreaKind, DetectionConfig};

const NANOS_PER_DAY: i64 = 86_400_000_000_000;

/// Detect temporal gaps — nodes not updated within the configured time window.
pub fn detect_temporal_gaps(
    nodes: &[NodeSnapshot],
    config: &DetectionConfig,
    now: i64,
) -> Vec<BlackArea> {
    let threshold_nanos = config.temporal_gap_days as i64 * NANOS_PER_DAY;

    nodes
        .iter()
        .filter(|n| {
            // Only flag nodes that have been updated at least once
            // and are now stale
            let last_update = if n.updated_at > 0 { n.updated_at } else { n.created_at };
            last_update > 0 && (now - last_update) > threshold_nanos
        })
        .map(|n| {
            let last_update = if n.updated_at > 0 { n.updated_at } else { n.created_at };
            let days_stale = (now - last_update) / NANOS_PER_DAY;
            // Severity increases with staleness, capped at 1.0
            let severity = (days_stale as f32 / (config.temporal_gap_days as f32 * 4.0)).min(1.0);

            BlackArea {
                kind: BlackAreaKind::TemporalGap,
                entities: vec![n.label.clone()],
                severity: severity.max(0.3), // minimum severity if detected
                suggested_queries: vec![
                    format!("{} latest news", n.label),
                    format!("{} recent updates", n.label),
                ],
                domain: n.node_type.clone(),
                detected_at: now,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn node_at(label: &str, updated: i64) -> NodeSnapshot {
        NodeSnapshot {
            label: label.into(),
            node_type: None,
            confidence: 0.8,
            memory_tier: 0,
            properties: HashMap::new(),
            created_at: updated,
            updated_at: updated,
            edge_out_count: 3,
            edge_in_count: 2,
        }
    }

    #[test]
    fn detects_stale_nodes() {
        let now = 200 * NANOS_PER_DAY;
        let nodes = vec![
            node_at("stale", 10 * NANOS_PER_DAY),   // 190 days old
            node_at("fresh", 195 * NANOS_PER_DAY),   // 5 days old
        ];

        let config = DetectionConfig {
            temporal_gap_days: 90,
            ..Default::default()
        };

        let gaps = detect_temporal_gaps(&nodes, &config, now);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].entities[0], "stale");
    }

    #[test]
    fn no_gaps_when_all_fresh() {
        let now = 100 * NANOS_PER_DAY;
        let nodes = vec![
            node_at("A", 95 * NANOS_PER_DAY),
            node_at("B", 98 * NANOS_PER_DAY),
        ];

        let config = DetectionConfig::default();
        let gaps = detect_temporal_gaps(&nodes, &config, now);
        assert!(gaps.is_empty());
    }

    #[test]
    fn ignores_zero_timestamp_nodes() {
        let now = 100 * NANOS_PER_DAY;
        let nodes = vec![node_at("no-time", 0)];

        let config = DetectionConfig::default();
        let gaps = detect_temporal_gaps(&nodes, &config, now);
        assert!(gaps.is_empty());
    }
}
