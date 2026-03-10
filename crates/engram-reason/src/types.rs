/// Core types for black area detection and knowledge gap analysis.

use serde::Serialize;

/// A detected knowledge gap in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct BlackArea {
    /// What kind of gap this is.
    pub kind: BlackAreaKind,
    /// Labels of involved entities.
    pub entities: Vec<String>,
    /// Severity score: 0.0 = minor gap, 1.0 = critical blind spot.
    pub severity: f32,
    /// Suggested queries to fill this gap.
    pub suggested_queries: Vec<String>,
    /// Topic domain, if identifiable.
    pub domain: Option<String>,
    /// When this gap was detected (unix nanos).
    pub detected_at: i64,
}

/// Classification of knowledge gaps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlackAreaKind {
    /// Entity with very few edges, dangling at edge of knowledge.
    FrontierNode,
    /// A->B and B->C exist but A->C doesn't (expected transitive link missing).
    StructuralHole,
    /// Related topic X has many facts, topic Y has very few.
    AsymmetricCluster,
    /// Facts about entity stop at a date, nothing recent.
    TemporalGap,
    /// All facts in a cluster have low confidence.
    ConfidenceDesert,
    /// Dense internal edges, sparse external, low author trust, temporal sync.
    CoordinatedCluster,
}

/// Configuration for gap detection thresholds.
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    /// Max edges for a node to be considered a frontier (default: 2).
    pub frontier_max_edges: u32,
    /// Days since last update to trigger temporal gap (default: 90).
    pub temporal_gap_days: u64,
    /// Minimum confidence threshold for desert detection (default: 0.3).
    pub confidence_desert_threshold: f32,
    /// Minimum cluster size ratio to detect asymmetry (default: 5.0).
    pub asymmetric_ratio: f32,
    /// Internal/external edge ratio threshold for coordinated cluster (default: 3.0).
    pub coordinated_edge_ratio: f32,
    /// Minimum cluster size for coordinated detection (default: 5).
    pub coordinated_min_size: usize,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            frontier_max_edges: 2,
            temporal_gap_days: 90,
            confidence_desert_threshold: 0.3,
            asymmetric_ratio: 5.0,
            coordinated_edge_ratio: 3.0,
            coordinated_min_size: 5,
        }
    }
}

/// Summary of a gap detection scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanReport {
    pub total_nodes_scanned: u32,
    pub gaps_detected: u32,
    pub by_kind: ScanBreakdown,
}

/// Count of gaps by kind.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanBreakdown {
    pub frontier_nodes: u32,
    pub structural_holes: u32,
    pub asymmetric_clusters: u32,
    pub temporal_gaps: u32,
    pub confidence_deserts: u32,
    pub coordinated_clusters: u32,
}
