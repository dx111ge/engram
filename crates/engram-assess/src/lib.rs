/// # engram-assess
///
/// Assessment and hypothesis tracking for engram. Lets users define hypotheses
/// (e.g. "NVIDIA stock > $200 by Q3 2026"), watch relevant entities, and
/// automatically re-evaluate probability when new information arrives.
///
/// Architecture: thin graph presence (assessment node + watch edges) + JSON sidecar
/// for score history time-series.

pub mod engine;
pub mod store;
pub mod types;

pub use engine::{add_evidence, evaluate, AssessmentEngine};
pub use store::AssessmentStore;
pub use types::*;
