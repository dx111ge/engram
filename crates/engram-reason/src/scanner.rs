/// Full graph scanner: runs all detection algorithms and returns
/// a ranked list of black areas.

use engram_core::graph::Graph;

use crate::asymmetric::detect_asymmetric_clusters;
use crate::confidence::detect_confidence_deserts;
use crate::coordinated::detect_coordinated_clusters;
use crate::error::ReasonError;
use crate::frontier::{detect_frontier_nodes, detect_isolated_nodes};
use crate::queries::enrich_queries;
use crate::scoring::{dedup_gaps, rank_gaps};
use crate::structural::detect_structural_holes;
use crate::temporal::detect_temporal_gaps;
use crate::types::{BlackArea, DetectionConfig, ScanBreakdown, ScanReport, BlackAreaKind};

/// Run a full gap detection scan on the graph.
pub fn scan(
    graph: &Graph,
    config: &DetectionConfig,
    now: i64,
) -> Result<(Vec<BlackArea>, ScanReport), ReasonError> {
    let nodes = graph
        .all_nodes()
        .map_err(|e| ReasonError::Graph(e.to_string()))?;

    let mut report = ScanReport {
        total_nodes_scanned: nodes.len() as u32,
        ..Default::default()
    };

    let labels: Vec<String> = nodes.iter().map(|n| n.label.clone()).collect();

    // Run all detectors
    let mut gaps = Vec::new();

    gaps.extend(detect_frontier_nodes(&nodes, config, now));
    gaps.extend(detect_isolated_nodes(&nodes, now));
    gaps.extend(detect_structural_holes(graph, &labels, 200, now));
    gaps.extend(detect_asymmetric_clusters(&nodes, config, now));
    gaps.extend(detect_temporal_gaps(&nodes, config, now));
    gaps.extend(detect_confidence_deserts(&nodes, config, now));
    gaps.extend(detect_coordinated_clusters(graph, &nodes, config, now));

    // Dedup, enrich with queries, rank
    gaps = dedup_gaps(gaps);
    enrich_queries(&mut gaps, graph);
    rank_gaps(&mut gaps);

    // Count by kind
    let mut breakdown = ScanBreakdown::default();
    for gap in &gaps {
        match gap.kind {
            BlackAreaKind::FrontierNode => breakdown.frontier_nodes += 1,
            BlackAreaKind::StructuralHole => breakdown.structural_holes += 1,
            BlackAreaKind::AsymmetricCluster => breakdown.asymmetric_clusters += 1,
            BlackAreaKind::TemporalGap => breakdown.temporal_gaps += 1,
            BlackAreaKind::ConfidenceDesert => breakdown.confidence_deserts += 1,
            BlackAreaKind::CoordinatedCluster => breakdown.coordinated_clusters += 1,
        }
    }

    report.gaps_detected = gaps.len() as u32;
    report.by_kind = breakdown;

    Ok((gaps, report))
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
    fn scan_empty_graph() {
        let (_dir, graph) = test_graph();
        let config = DetectionConfig::default();
        let (gaps, report) = scan(&graph, &config, 1000).unwrap();
        assert!(gaps.is_empty());
        assert_eq!(report.total_nodes_scanned, 0);
    }

    #[test]
    fn scan_finds_frontier_nodes() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");

        // Create a node with just one edge
        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();
        graph.relate("A", "B", "knows", &prov).unwrap();
        // C is isolated

        let config = DetectionConfig::default();
        let (gaps, report) = scan(&graph, &config, 1000).unwrap();

        assert_eq!(report.total_nodes_scanned, 3);
        assert!(!gaps.is_empty());
        // Should find frontier node A (1 out edge, 0 in) and B (0 out, 1 in)
        // and isolated node C
    }

    #[test]
    fn scan_with_structural_hole() {
        let (_dir, mut graph) = test_graph();
        let prov = Provenance::user("test");

        // A->B, B->C, A->D, D->E — but no A->C
        graph.store("A", &prov).unwrap();
        graph.store("B", &prov).unwrap();
        graph.store("C", &prov).unwrap();
        graph.store("D", &prov).unwrap();
        graph.store("E", &prov).unwrap();
        graph.relate("A", "B", "knows", &prov).unwrap();
        graph.relate("A", "D", "knows", &prov).unwrap();
        graph.relate("B", "C", "knows", &prov).unwrap();
        graph.relate("D", "E", "knows", &prov).unwrap();

        let config = DetectionConfig::default();
        let (gaps, report) = scan(&graph, &config, 1000).unwrap();

        assert!(report.gaps_detected > 0);
        let holes: Vec<_> = gaps.iter().filter(|g| g.kind == BlackAreaKind::StructuralHole).collect();
        assert!(!holes.is_empty());
    }
}
