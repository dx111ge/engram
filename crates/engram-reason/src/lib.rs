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
/// Also provides:
/// - LLM-powered investigation suggestions (optional)
/// - Knowledge profile derivation for mesh peers
/// - Federated query protocol for cross-mesh search
/// - 3-tier enrichment dispatcher (mesh > free > paid)
/// - Mesh-level gap detection across all peers

pub mod asymmetric;
pub mod confidence;
pub mod coordinated;
pub mod enrichment;
pub mod error;
pub mod federated;
pub mod frontier;
pub mod llm_suggestions;
pub mod mesh_gaps;
pub mod profiles;
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
pub use profiles::{KnowledgeProfile, DomainCoverage, NodeCapability, ProfileConfig, derive_profile};
pub use federated::{FederatedQuery, FederatedResult, FederatedFact};
pub use enrichment::{EnrichmentConfig, EnrichmentMode, EnrichmentDispatcher, EnrichmentEvent};
pub use llm_suggestions::LlmSuggestionConfig;
pub use mesh_gaps::MeshGap;
