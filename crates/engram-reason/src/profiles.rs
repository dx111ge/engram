/// Mesh knowledge profiles: auto-derived domain coverage from graph clusters.
///
/// Each engram node periodically analyzes its graph to build a KnowledgeProfile
/// describing what topics it covers, how many facts it has, and how fresh
/// and confident those facts are. Profiles are broadcast to mesh peers
/// via gossip so other nodes can discover who knows what.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use engram_core::graph::Graph;

/// A node's knowledge profile, summarizing what it covers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeProfile {
    /// Node name (human label, e.g., "iran-desk").
    pub name: String,
    /// Auto-derived domain coverage.
    pub domains: Vec<DomainCoverage>,
    /// Total facts (nodes) in the graph.
    pub total_facts: u64,
    /// Timestamp of last profile update (unix nanos).
    pub last_updated: i64,
    /// Capabilities this node advertises.
    pub capabilities: Vec<NodeCapability>,
}

/// Coverage summary for a single domain/topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainCoverage {
    /// Topic name (derived from node types, domains, or labels).
    pub topic: String,
    /// Number of facts in this domain.
    pub fact_count: u64,
    /// Average confidence of facts in this domain.
    pub avg_confidence: f32,
    /// Most recent fact timestamp in this domain (unix nanos).
    pub freshness: i64,
    /// Coverage density: 0.0-1.0.
    pub depth: f32,
}

/// Capabilities a node can advertise.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NodeCapability {
    /// NER languages supported.
    NerAvailable(Vec<String>),
    /// GPU compute available for heavy workloads.
    GpuCompute,
    /// Paid source access available.
    SourceAccess(Vec<String>),
    /// Node is highly available (always online).
    HighAvailability,
}

/// Configuration for profile generation.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Maximum domains to include in profile.
    pub max_domains: usize,
    /// Minimum facts in a domain to include it.
    pub min_facts: u64,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            max_domains: 20,
            min_facts: 3,
        }
    }
}

/// Derive a KnowledgeProfile from the current graph state.
pub fn derive_profile(
    graph: &Graph,
    node_name: &str,
    config: &ProfileConfig,
    capabilities: Vec<NodeCapability>,
    now: i64,
) -> KnowledgeProfile {
    let nodes = graph.all_nodes().unwrap_or_default();
    let total_facts = nodes.len() as u64;

    // Group nodes by domain (node_type or "domain" property)
    let mut domain_map: HashMap<String, Vec<(f32, i64)>> = HashMap::new();

    for node in &nodes {
        let domain = node.node_type.clone()
            .or_else(|| node.properties.get("domain").cloned())
            .unwrap_or_else(|| "general".to_string());

        domain_map.entry(domain)
            .or_default()
            .push((node.confidence, node.updated_at as i64));
    }

    // Convert to DomainCoverage, sorted by fact count descending
    let mut domains: Vec<DomainCoverage> = domain_map
        .into_iter()
        .filter(|(_, facts)| facts.len() as u64 >= config.min_facts)
        .map(|(topic, facts)| {
            let fact_count = facts.len() as u64;
            let avg_confidence = facts.iter().map(|(c, _)| c).sum::<f32>() / facts.len() as f32;
            let freshness = facts.iter().map(|(_, t)| *t).max().unwrap_or(0);
            // Depth: ratio of facts in this domain to total facts
            let depth = (fact_count as f32 / total_facts.max(1) as f32).min(1.0);
            DomainCoverage {
                topic,
                fact_count,
                avg_confidence,
                freshness,
                depth,
            }
        })
        .collect();

    domains.sort_by(|a, b| b.fact_count.cmp(&a.fact_count));
    domains.truncate(config.max_domains);

    KnowledgeProfile {
        name: node_name.to_string(),
        domains,
        total_facts,
        last_updated: now,
        capabilities,
    }
}

/// Check if a profile covers a given topic (case-insensitive substring match).
pub fn covers_topic<'a>(profile: &'a KnowledgeProfile, topic: &str) -> Option<&'a DomainCoverage> {
    let lower = topic.to_lowercase();
    profile.domains.iter().find(|d| d.topic.to_lowercase().contains(&lower))
}

/// Match profiles against a query, returning those with relevant domains.
pub fn discover_by_topic<'a>(
    profiles: &'a [KnowledgeProfile],
    topic: &str,
) -> Vec<(usize, &'a DomainCoverage)> {
    let lower = topic.to_lowercase();
    profiles.iter().enumerate()
        .filter_map(|(i, p)| {
            p.domains.iter()
                .find(|d| d.topic.to_lowercase().contains(&lower))
                .map(|d| (i, d))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::graph::Provenance;

    fn test_graph() -> (tempfile::TempDir, Graph) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = Graph::create(&path).unwrap();
        (dir, graph)
    }

    #[test]
    fn derive_profile_empty_graph() {
        let (_dir, graph) = test_graph();
        let profile = derive_profile(&graph, "test-node", &ProfileConfig::default(), vec![], 1000);
        assert_eq!(profile.total_facts, 0);
        assert!(profile.domains.is_empty());
    }

    #[test]
    fn derive_profile_with_typed_nodes() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");
        for name in ["Iran", "Iraq", "Syria"] {
            graph.store(name, &prov).unwrap();
            graph.set_node_type(name, "country").unwrap();
        }
        for name in ["sanctions", "embargo"] {
            graph.store(name, &prov).unwrap();
            graph.set_node_type(name, "policy").unwrap();
        }

        let profile = derive_profile(&graph, "iran-desk", &ProfileConfig::default(), vec![], 1000);
        assert_eq!(profile.total_facts, 5);
        assert!(profile.domains.iter().any(|d| d.topic == "country"));
        assert!(!profile.domains.iter().any(|d| d.topic == "policy")); // only 2, below min_facts=3
    }

    #[test]
    fn covers_topic_match() {
        let profile = KnowledgeProfile {
            name: "test".into(),
            domains: vec![DomainCoverage {
                topic: "country".into(),
                fact_count: 10,
                avg_confidence: 0.8,
                freshness: 1000,
                depth: 0.5,
            }],
            total_facts: 20,
            last_updated: 1000,
            capabilities: vec![],
        };
        assert!(covers_topic(&profile, "country").is_some());
        assert!(covers_topic(&profile, "Country").is_some()); // case-insensitive
        assert!(covers_topic(&profile, "weapons").is_none());
    }

    #[test]
    fn discover_by_topic_filters() {
        let profiles = vec![
            KnowledgeProfile {
                name: "iran-desk".into(),
                domains: vec![DomainCoverage {
                    topic: "Iran".into(),
                    fact_count: 50,
                    avg_confidence: 0.8,
                    freshness: 1000,
                    depth: 0.5,
                }],
                total_facts: 100,
                last_updated: 1000,
                capabilities: vec![],
            },
            KnowledgeProfile {
                name: "china-desk".into(),
                domains: vec![DomainCoverage {
                    topic: "China".into(),
                    fact_count: 30,
                    avg_confidence: 0.7,
                    freshness: 900,
                    depth: 0.4,
                }],
                total_facts: 80,
                last_updated: 1000,
                capabilities: vec![],
            },
        ];
        let matches = discover_by_topic(&profiles, "Iran");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, 0); // index of iran-desk
    }
}
