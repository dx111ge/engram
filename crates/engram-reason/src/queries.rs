/// Suggested query generation: mechanical query construction from
/// graph topology to fill detected knowledge gaps.

use engram_core::graph::Graph;

use crate::types::{BlackArea, BlackAreaKind};

/// Generate suggested queries for a black area based on graph context.
///
/// This is the mechanical (non-LLM) query generator. It uses entity
/// labels, types, and relationship patterns to construct search queries.
pub fn generate_queries(gap: &mut BlackArea, graph: &Graph) {
    let mut queries = Vec::new();

    match &gap.kind {
        BlackAreaKind::FrontierNode => {
            for entity in &gap.entities {
                let node_type = graph.get_node_type(entity);
                if let Some(t) = &node_type {
                    queries.push(format!("{} {} connections", entity, t.to_lowercase()));
                }
                queries.push(format!("{} related entities", entity));

                // Look at existing edge types to suggest more of the same
                if let Ok(edges) = graph.edges_from(entity) {
                    let rel_types: Vec<&str> = edges.iter().map(|e| e.relationship.as_str()).collect();
                    for rt in rel_types.iter().take(3) {
                        queries.push(format!("{} {} what", entity, rt));
                    }
                }
            }
        }

        BlackAreaKind::StructuralHole => {
            if gap.entities.len() >= 3 {
                let a = &gap.entities[0];
                let c = &gap.entities[2];
                queries.push(format!("{} {} relationship", a, c));
                queries.push(format!("{} {} connection", a, c));
            }
        }

        BlackAreaKind::AsymmetricCluster => {
            if let Some(domain) = &gap.domain {
                queries.push(format!("{} overview", domain));
                queries.push(format!("{} key entities", domain));
                queries.push(format!("{} important facts", domain));
            }
        }

        BlackAreaKind::TemporalGap => {
            for entity in &gap.entities {
                queries.push(format!("{} latest", entity));
                queries.push(format!("{} recent developments", entity));
            }
        }

        BlackAreaKind::ConfidenceDesert => {
            for entity in gap.entities.iter().take(5) {
                queries.push(format!("{} verification", entity));
                queries.push(format!("{} source", entity));
            }
        }

        BlackAreaKind::CoordinatedCluster => {
            if let Some(domain) = &gap.domain {
                queries.push(format!("{} source analysis", domain));
                queries.push(format!("{} independent verification", domain));
            }
        }
    }

    // Merge with existing queries, dedup
    for q in queries {
        if !gap.suggested_queries.contains(&q) {
            gap.suggested_queries.push(q);
        }
    }
}

/// Generate queries for all gaps in a batch.
pub fn enrich_queries(gaps: &mut [BlackArea], graph: &Graph) {
    for gap in gaps.iter_mut() {
        generate_queries(gap, graph);
    }
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
    fn generates_frontier_queries() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");
        graph.store("Apple", &prov).unwrap();
        graph.set_node_type("Apple", "ORG").unwrap();

        let mut gap = BlackArea {
            kind: BlackAreaKind::FrontierNode,
            entities: vec!["Apple".into()],
            severity: 0.5,
            suggested_queries: vec![],
            domain: None,
            detected_at: 0,
        };

        generate_queries(&mut gap, &graph);
        assert!(!gap.suggested_queries.is_empty());
        assert!(gap.suggested_queries.iter().any(|q| q.contains("Apple")));
    }

    #[test]
    fn generates_structural_hole_queries() {
        let (_dir, graph) = test_graph();

        let mut gap = BlackArea {
            kind: BlackAreaKind::StructuralHole,
            entities: vec!["A".into(), "B".into(), "C".into()],
            severity: 0.5,
            suggested_queries: vec![],
            domain: None,
            detected_at: 0,
        };

        generate_queries(&mut gap, &graph);
        assert!(gap.suggested_queries.iter().any(|q| q.contains("A") && q.contains("C")));
    }

    #[test]
    fn generates_temporal_queries() {
        let (_dir, graph) = test_graph();

        let mut gap = BlackArea {
            kind: BlackAreaKind::TemporalGap,
            entities: vec!["Russia sanctions".into()],
            severity: 0.5,
            suggested_queries: vec![],
            domain: None,
            detected_at: 0,
        };

        generate_queries(&mut gap, &graph);
        assert!(gap.suggested_queries.iter().any(|q| q.contains("latest")));
    }
}
