/// # engram-reason
///
/// Black area detection, knowledge gap analysis, and enrichment engine.
///
/// Detects what's **missing** from the knowledge graph:
/// - Frontier nodes (dangling entities with few connections)
/// - Structural holes (missing expected transitive links)
/// - Asymmetric clusters (related topics with vastly different coverage)
/// - Temporal gaps (entities not updated in a long time)
/// - Confidence deserts (clusters of low-confidence facts)
/// - Coordinated clusters (dense internal, sparse external, suspicious patterns)
///
/// Generates suggested queries to fill gaps and ranks them by severity.

pub mod asymmetric;
pub mod confidence;
pub mod coordinated;
pub mod error;
pub mod frontier;
pub mod queries;
pub mod scanner;
pub mod scoring;
pub mod structural;
pub mod temporal;
pub mod types;

// Re-exports for convenience.
pub use error::ReasonError;
pub use scanner::scan;
pub use types::{BlackArea, BlackAreaKind, DetectionConfig, ScanReport};
