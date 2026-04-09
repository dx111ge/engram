/// Asymmetric cluster analysis: detects related topics with vastly
/// different coverage levels.
///
/// If topic X has 50 facts and closely related topic Y has only 2,
/// this indicates a knowledge blind spot worth investigating.

use std::collections::HashMap;

use engram_core::graph::NodeSnapshot;

use crate::types::{BlackArea, BlackAreaKind, DetectionConfig};

/// Group nodes by type and detect asymmetric coverage.
///
/// Compares cluster sizes within the same node type family.
/// If one cluster is N times larger than a related one, flags it.
pub fn detect_asymmetric_clusters(
    nodes: &[NodeSnapshot],
    config: &DetectionConfig,
    now: i64,
) -> Vec<BlackArea> {
    // Group nodes by type
    let mut by_type: HashMap<String, Vec<&NodeSnapshot>> = HashMap::new();
    for node in nodes {
        let type_key = node.node_type.clone().unwrap_or_else(|| "untyped".to_string());
        by_type.entry(type_key).or_default().push(node);
    }

    // Also group by domain property if available
    let mut by_domain: HashMap<String, Vec<&NodeSnapshot>> = HashMap::new();
    for node in nodes {
        if let Some(domain) = node.properties.get("domain") {
            by_domain.entry(domain.clone()).or_default().push(node);
        }
    }

    let mut gaps = Vec::new();
    let ratio_threshold = config.asymmetric_ratio;

    // Compare domain clusters
    let domains: Vec<(&String, &Vec<&NodeSnapshot>)> = by_domain.iter().collect();
    for i in 0..domains.len() {
        for j in (i + 1)..domains.len() {
            let (name_a, nodes_a) = domains[i];
            let (name_b, nodes_b) = domains[j];
            let (big, small, big_name, small_name) = if nodes_a.len() >= nodes_b.len() {
                (nodes_a.len(), nodes_b.len(), name_a, name_b)
            } else {
                (nodes_b.len(), nodes_a.len(), name_b, name_a)
            };

            if small == 0 {
                continue;
            }

            let ratio = big as f32 / small as f32;
            if ratio >= ratio_threshold {
                let severity = (ratio / 20.0).min(1.0); // cap at 1.0

                gaps.push(BlackArea {
                    kind: BlackAreaKind::AsymmetricCluster,
                    entities: vec![big_name.clone(), small_name.clone()],
                    severity,
                    suggested_queries: vec![
                        format!("{} overview", small_name),
                        format!("{} key facts", small_name),
                    ],
                    domain: Some(small_name.clone()),
                    detected_at: now,
                });
            }
        }
    }

    // Type-based comparison removed -- comparing node types (person vs organization)
    // is not actionable intelligence. Only domain-based comparison (above) is meaningful.
    // Domains are set via user-defined taxonomy + auto-classification.

    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn node_with_domain(label: &str, domain: &str) -> NodeSnapshot {
        let mut props = HashMap::new();
        props.insert("domain".to_string(), domain.to_string());
        NodeSnapshot {
            label: label.into(),
            node_type: Some("entity".into()),
            confidence: 0.8,
            memory_tier: 0,
            properties: props,
            created_at: 0,
            updated_at: 0,
            edge_out_count: 3,
            edge_in_count: 2,
        }
    }

    #[test]
    fn detects_asymmetric_domains() {
        let mut nodes = Vec::new();
        // Domain A: 10 nodes
        for i in 0..10 {
            nodes.push(node_with_domain(&format!("A{}", i), "technology"));
        }
        // Domain B: 1 node
        nodes.push(node_with_domain("B0", "healthcare"));

        let config = DetectionConfig {
            asymmetric_ratio: 5.0,
            ..Default::default()
        };

        let gaps = detect_asymmetric_clusters(&nodes, &config, 1000);
        let domain_gaps: Vec<_> = gaps.iter().filter(|g| g.kind == BlackAreaKind::AsymmetricCluster).collect();
        assert!(!domain_gaps.is_empty());
    }

    #[test]
    fn no_asymmetry_when_balanced() {
        let mut nodes = Vec::new();
        for i in 0..5 {
            nodes.push(node_with_domain(&format!("A{}", i), "tech"));
        }
        for i in 0..4 {
            nodes.push(node_with_domain(&format!("B{}", i), "finance"));
        }

        let config = DetectionConfig::default();
        let gaps = detect_asymmetric_clusters(&nodes, &config, 1000);
        let domain_gaps: Vec<_> = gaps.iter()
            .filter(|g| g.kind == BlackAreaKind::AsymmetricCluster && g.entities.len() == 2)
            .collect();
        assert!(domain_gaps.is_empty());
    }
}
