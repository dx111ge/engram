/// Ingest pipeline error types.

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("source error: {0}")]
    Source(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("NER error: {0}")]
    Ner(String),

    #[error("resolution error: {0}")]
    Resolution(String),

    #[error("graph error: {0}")]
    Graph(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("pipeline cancelled")]
    Cancelled,

    #[error("channel closed")]
    ChannelClosed,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
