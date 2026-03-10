/// Error types for the reason crate.

#[derive(Debug, thiserror::Error)]
pub enum ReasonError {
    #[error("graph error: {0}")]
    Graph(String),

    #[error("detection error: {0}")]
    Detection(String),

    #[error("enrichment error: {0}")]
    Enrichment(String),
}
