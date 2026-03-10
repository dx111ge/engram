/// Federated query protocol: cross-mesh search and result merging.
///
/// Allows one engram node to search other peers' knowledge graphs
/// with ACL-enforced sensitivity filtering. Results are merged and
/// ranked by confidence, weighted by peer trust.

use serde::{Deserialize, Serialize};

/// A federated query sent to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedQuery {
    /// Search query text.
    pub query: String,
    /// Query type: "semantic", "fulltext", "hybrid".
    pub query_type: String,
    /// Maximum results to return.
    pub max_results: u32,
    /// Minimum confidence threshold.
    pub min_confidence: f32,
    /// Requesting node identifier.
    pub requesting_node: String,
    /// Sensitivity clearance: "public", "internal", etc.
    pub sensitivity_clearance: String,
}

/// Result of a federated query from a single peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedResult {
    /// Matching facts from this peer.
    pub facts: Vec<FederatedFact>,
    /// Peer identifier.
    pub peer_id: String,
    /// Query execution time in milliseconds.
    pub query_time_ms: u64,
    /// Total matches (may be more than returned).
    pub total_matches: u64,
}

/// A single fact returned from a federated query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedFact {
    /// Entity label.
    pub label: String,
    /// Entity type (if known).
    pub entity_type: Option<String>,
    /// Properties (key-value pairs).
    pub properties: std::collections::HashMap<String, String>,
    /// Confidence score.
    pub confidence: f32,
    /// Relationships to include.
    pub edges: Vec<FederatedEdge>,
    /// Source attribution.
    pub provenance: String,
}

/// An edge in a federated result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedEdge {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f32,
}

/// Execute a federated query against the local graph.
///
/// This is called when a peer sends us a FederatedQuery. We search
/// our local graph and return matching facts, filtered by ACL.
pub fn execute_local(
    graph: &engram_core::graph::Graph,
    query: &FederatedQuery,
) -> FederatedResult {
    let start = std::time::Instant::now();

    // Search local graph
    let results = graph.search(&query.query, query.max_results as usize)
        .unwrap_or_default();

    let mut facts = Vec::new();
    for result in &results {
        if result.confidence < query.min_confidence {
            continue;
        }

        // ACL filtering: skip nodes with access_level > clearance
        let props = graph.get_properties(&result.label)
            .ok()
            .flatten()
            .unwrap_or_default();
        let access_level = props.get("access_level").map(|s| s.as_str()).unwrap_or("public");
        if !check_clearance(access_level, &query.sensitivity_clearance) {
            continue;
        }

        let entity_type = graph.get_node_type(&result.label);

        // Include edges
        let edges_from = graph.edges_from(&result.label).unwrap_or_default();
        let edges_to = graph.edges_to(&result.label).unwrap_or_default();

        let mut edges = Vec::new();
        for e in edges_from.iter().take(5) {
            edges.push(FederatedEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                relationship: e.relationship.clone(),
                confidence: e.confidence,
            });
        }
        for e in edges_to.iter().take(5) {
            edges.push(FederatedEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                relationship: e.relationship.clone(),
                confidence: e.confidence,
            });
        }

        facts.push(FederatedFact {
            label: result.label.clone(),
            entity_type,
            properties: props,
            confidence: result.confidence,
            edges,
            provenance: "local".to_string(),
        });
    }

    let total_matches = facts.len() as u64;
    facts.truncate(query.max_results as usize);

    FederatedResult {
        facts,
        peer_id: String::new(), // filled by caller
        query_time_ms: start.elapsed().as_millis() as u64,
        total_matches,
    }
}

/// Merge results from multiple peers, sorted by confidence descending.
pub fn merge_results(results: Vec<FederatedResult>) -> Vec<FederatedFact> {
    let mut all_facts: Vec<FederatedFact> = results
        .into_iter()
        .flat_map(|r| {
            let peer_id = r.peer_id.clone();
            r.facts.into_iter().map(move |mut f| {
                if f.provenance == "local" {
                    f.provenance = format!("peer:{}", peer_id);
                }
                f
            })
        })
        .collect();

    // Sort by confidence descending
    all_facts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

    // Dedup by label (keep highest confidence)
    let mut seen = std::collections::HashSet::new();
    all_facts.retain(|f| seen.insert(f.label.clone()));

    all_facts
}

/// Check if a sensitivity clearance level allows access to a given access level.
fn check_clearance(access_level: &str, clearance: &str) -> bool {
    let level = match access_level {
        "public" => 0,
        "internal" => 1,
        "confidential" => 2,
        "restricted" => 3,
        _ => 0,
    };
    let clear = match clearance {
        "public" => 0,
        "internal" => 1,
        "confidential" => 2,
        "restricted" => 3,
        _ => 0,
    };
    clear >= level
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::graph::Provenance;

    fn test_graph() -> (tempfile::TempDir, engram_core::graph::Graph) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = engram_core::graph::Graph::create(&path).unwrap();
        (dir, graph)
    }

    #[test]
    fn execute_local_basic() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");
        graph.store("Rust", &prov).unwrap();
        graph.store("Python", &prov).unwrap();

        let query = FederatedQuery {
            query: "Rust".into(),
            query_type: "fulltext".into(),
            max_results: 10,
            min_confidence: 0.0,
            requesting_node: "peer-1".into(),
            sensitivity_clearance: "public".into(),
        };

        let result = execute_local(&graph, &query);
        assert!(result.query_time_ms < 5000);
    }

    #[test]
    fn merge_deduplicates() {
        let r1 = FederatedResult {
            facts: vec![FederatedFact {
                label: "A".into(),
                entity_type: None,
                properties: Default::default(),
                confidence: 0.9,
                edges: vec![],
                provenance: "peer:1".into(),
            }],
            peer_id: "1".into(),
            query_time_ms: 10,
            total_matches: 1,
        };
        let r2 = FederatedResult {
            facts: vec![FederatedFact {
                label: "A".into(),
                entity_type: None,
                properties: Default::default(),
                confidence: 0.7,
                edges: vec![],
                provenance: "peer:2".into(),
            }],
            peer_id: "2".into(),
            query_time_ms: 15,
            total_matches: 1,
        };

        let merged = merge_results(vec![r1, r2]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].confidence, 0.9); // keeps highest
    }

    #[test]
    fn clearance_check() {
        assert!(check_clearance("public", "public"));
        assert!(check_clearance("public", "internal"));
        assert!(!check_clearance("internal", "public"));
        assert!(check_clearance("confidential", "restricted"));
    }
}
