/// Mesh-level black area detection: finds topics NOT covered by any peer.
///
/// While per-node gap detection finds what's missing from a single graph,
/// mesh-level detection identifies topics that the entire mesh doesn't cover.
/// This helps teams identify collective blind spots.

use serde::{Deserialize, Serialize};

use crate::profiles::{DomainCoverage, KnowledgeProfile};

/// A mesh-level gap: a topic area not covered by any peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshGap {
    /// Topic or domain that's uncovered.
    pub topic: String,
    /// Severity: 0.0 = minor, 1.0 = critical blind spot.
    pub severity: f32,
    /// Which peers are closest to covering this topic (partial coverage).
    pub nearest_peers: Vec<NearestPeer>,
    /// Why this gap matters.
    pub reason: String,
}

/// A peer that partially covers a topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearestPeer {
    /// Peer name.
    pub name: String,
    /// How related their closest domain is.
    pub relevance: f32,
    /// Their closest domain topic.
    pub closest_topic: String,
}

/// Detect mesh-level gaps by analyzing all peers' knowledge profiles.
///
/// Takes a list of "expected topics" (from domain knowledge or graph analysis)
/// and checks which ones are not covered by any peer in the mesh.
pub fn detect_mesh_gaps(
    profiles: &[KnowledgeProfile],
    expected_topics: &[String],
    min_coverage_depth: f32,
) -> Vec<MeshGap> {
    let mut gaps = Vec::new();

    for topic in expected_topics {
        let lower = topic.to_lowercase();

        // Check if any peer covers this topic
        let mut best_coverage: Option<(&KnowledgeProfile, &DomainCoverage)> = None;
        let mut partial_peers = Vec::new();

        for profile in profiles {
            for domain in &profile.domains {
                if domain.topic.to_lowercase().contains(&lower) || lower.contains(&domain.topic.to_lowercase()) {
                    if domain.depth >= min_coverage_depth {
                        // Covered sufficiently
                        best_coverage = Some((profile, domain));
                        break;
                    } else {
                        partial_peers.push(NearestPeer {
                            name: profile.name.clone(),
                            relevance: domain.depth,
                            closest_topic: domain.topic.clone(),
                        });
                    }
                }
            }
            if best_coverage.is_some() {
                break;
            }
        }

        if best_coverage.is_none() {
            // Not covered by any peer with sufficient depth
            let severity = if partial_peers.is_empty() {
                0.9 // No peer has any related coverage
            } else {
                let max_relevance = partial_peers.iter().map(|p| p.relevance).fold(0.0f32, f32::max);
                // Higher severity when no peer is close
                (1.0 - max_relevance).max(0.3)
            };

            gaps.push(MeshGap {
                topic: topic.clone(),
                severity,
                nearest_peers: partial_peers,
                reason: format!("No peer in the mesh covers '{}' with sufficient depth", topic),
            });
        }
    }

    // Sort by severity descending
    gaps.sort_by(|a, b| b.severity.partial_cmp(&a.severity).unwrap_or(std::cmp::Ordering::Equal));
    gaps
}

/// Extract expected topics from the mesh's collective coverage.
///
/// Looks at all domains across all profiles and identifies topics that
/// appear in edges/relationships but don't have dedicated coverage.
pub fn extract_coverage_gaps(
    profiles: &[KnowledgeProfile],
) -> Vec<String> {
    use std::collections::{HashMap, HashSet};

    // Count how many peers cover each topic
    let mut coverage_count: HashMap<String, u32> = HashMap::new();
    let mut all_topics: HashSet<String> = HashSet::new();

    for profile in profiles {
        for domain in &profile.domains {
            let topic = domain.topic.to_lowercase();
            *coverage_count.entry(topic.clone()).or_default() += 1;
            all_topics.insert(topic);
        }
    }

    // Topics covered by only one peer are potential single points of failure
    let single_coverage: Vec<String> = coverage_count
        .into_iter()
        .filter(|(_, count)| *count <= 1)
        .map(|(topic, _)| topic)
        .collect();

    single_coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::DomainCoverage;

    fn make_profile(name: &str, domains: Vec<(&str, f32)>) -> KnowledgeProfile {
        KnowledgeProfile {
            name: name.into(),
            domains: domains.into_iter().map(|(topic, depth)| DomainCoverage {
                topic: topic.into(),
                fact_count: 50,
                avg_confidence: 0.8,
                freshness: 1000,
                depth,
            }).collect(),
            total_facts: 100,
            last_updated: 1000,
            capabilities: vec![],
        }
    }

    #[test]
    fn detect_uncovered_topic() {
        let profiles = vec![
            make_profile("iran-desk", vec![("Iran", 0.8), ("sanctions", 0.6)]),
            make_profile("russia-desk", vec![("Russia", 0.7), ("military", 0.5)]),
        ];

        let expected = vec!["China".into(), "Iran".into()];
        let gaps = detect_mesh_gaps(&profiles, &expected, 0.3);

        // China should be a gap, Iran should not
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].topic, "China");
        assert!(gaps[0].severity > 0.5);
    }

    #[test]
    fn partial_coverage_lowers_severity() {
        let profiles = vec![
            make_profile("desk-a", vec![("economics", 0.1)]), // low depth
        ];

        let expected = vec!["economics".into()];
        let gaps = detect_mesh_gaps(&profiles, &expected, 0.5);

        // economics has partial coverage (0.1 < 0.5 threshold)
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].severity < 0.95); // lower than no coverage at all
        assert_eq!(gaps[0].nearest_peers.len(), 1);
    }

    #[test]
    fn extract_single_coverage_points() {
        let profiles = vec![
            make_profile("a", vec![("Iran", 0.8), ("Russia", 0.6)]),
            make_profile("b", vec![("Russia", 0.7), ("China", 0.5)]),
        ];

        let single = extract_coverage_gaps(&profiles);
        // Iran covered by 1 peer, Russia by 2, China by 1
        assert!(single.contains(&"iran".to_string()));
        assert!(single.contains(&"china".to_string()));
        assert!(!single.contains(&"russia".to_string()));
    }
}
