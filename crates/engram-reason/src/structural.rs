/// Structural hole detection: missing expected transitive links.
///
/// If A->B and B->C exist, and A and C share a relationship type pattern,
/// the absence of A->C may indicate a structural hole in the knowledge.

use std::collections::HashSet;

use engram_core::graph::Graph;

use crate::types::{BlackArea, BlackAreaKind};

/// Detect structural holes by finding missing transitive links.
///
/// For each node A, look at its outgoing neighbors B. For each B,
/// look at B's outgoing neighbors C. If A has no direct link to C,
/// this is a candidate structural hole.
///
/// To avoid combinatorial explosion, only considers nodes with
/// moderate connectivity (3-50 edges) and limits results.
pub fn detect_structural_holes(
    graph: &Graph,
    labels: &[String],
    max_results: usize,
    now: i64,
) -> Vec<BlackArea> {
    let mut holes = Vec::new();

    for label in labels {
        let edges_a = match graph.edges_from(label) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Skip very high connectivity nodes (combinatorial explosion)
        if edges_a.is_empty() || edges_a.len() > 50 {
            continue;
        }

        let a_neighbors: HashSet<&str> = edges_a.iter().map(|e| e.to.as_str()).collect();

        for edge_ab in &edges_a {
            let edges_b = match graph.edges_from(&edge_ab.to) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for edge_bc in &edges_b {
                // Skip self-loops and already-connected
                if edge_bc.to == *label || a_neighbors.contains(edge_bc.to.as_str()) {
                    continue;
                }

                // Found: A->B->C but no A->C
                let severity = 0.4 + 0.2 * edge_ab.confidence.min(edge_bc.confidence);

                holes.push(BlackArea {
                    kind: BlackAreaKind::StructuralHole,
                    entities: vec![label.clone(), edge_ab.to.clone(), edge_bc.to.clone()],
                    severity: severity.min(1.0),
                    suggested_queries: vec![
                        format!("{} {} relationship", label, edge_bc.to),
                    ],
                    domain: None,
                    detected_at: now,
                });

                if holes.len() >= max_results {
                    return holes;
                }
            }
        }
    }

    holes
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
    fn detects_missing_transitive_link() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");

        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();

        // A->B and B->C but no A->C
        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();

        let labels: Vec<String> = vec!["A".into(), "B".into(), "C".into()];
        let holes = detect_structural_holes(&graph, &labels, 100, 1000);

        assert!(!holes.is_empty());
        assert_eq!(holes[0].kind, BlackAreaKind::StructuralHole);
        assert!(holes[0].entities.contains(&"A".to_string()));
        assert!(holes[0].entities.contains(&"C".to_string()));
    }

    #[test]
    fn no_hole_when_fully_connected() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");

        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();

        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();
        graph.relate("A", "C", "knows", &prov).unwrap();

        let labels: Vec<String> = vec!["A".into(), "B".into(), "C".into()];
        let holes = detect_structural_holes(&graph, &labels, 100, 1000);

        assert!(holes.is_empty());
    }
}
