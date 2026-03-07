/// Embedding interface — trait for generating embedding vectors from text.
///
/// Implementations can use ONNX Runtime, remote APIs, or any other backend.
/// Engram itself is model-agnostic: bring your own ONNX model.

/// Trait for embedding generation backends.
pub trait Embedder: Send + Sync {
    /// Generate an embedding vector from text.
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>;

    /// Generate embeddings for multiple texts (batch).
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Embedding dimensions.
    fn dim(&self) -> usize;

    /// Model identifier (for tracking which model produced embeddings).
    fn model_id(&self) -> &str;
}

#[derive(Debug)]
pub enum EmbedError {
    ModelNotLoaded,
    DimensionMismatch { expected: usize, got: usize },
    RuntimeError(String),
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedError::ModelNotLoaded => write!(f, "embedding model not loaded"),
            EmbedError::DimensionMismatch { expected, got } => {
                write!(f, "dimension mismatch: expected {expected}, got {got}")
            }
            EmbedError::RuntimeError(msg) => write!(f, "embedding runtime error: {msg}"),
        }
    }
}

impl std::error::Error for EmbedError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dummy embedder for testing — produces deterministic vectors from text.
    pub struct DummyEmbedder {
        dim: usize,
    }

    impl DummyEmbedder {
        pub fn new(dim: usize) -> Self {
            DummyEmbedder { dim }
        }
    }

    impl Embedder for DummyEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
            // Simple hash-based deterministic embedding
            let mut vec = vec![0.0f32; self.dim];
            let bytes = text.as_bytes();
            for (i, v) in vec.iter_mut().enumerate() {
                let mut hash: u32 = 0x811c9dc5;
                for &b in bytes {
                    hash ^= b as u32;
                    hash = hash.wrapping_mul(0x01000193);
                }
                hash ^= i as u32;
                hash = hash.wrapping_mul(0x01000193);
                *v = (hash as f32) / (u32::MAX as f32) * 2.0 - 1.0;
            }
            // Normalize
            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > f32::EPSILON {
                for v in &mut vec {
                    *v /= norm;
                }
            }
            Ok(vec)
        }

        fn dim(&self) -> usize {
            self.dim
        }

        fn model_id(&self) -> &str {
            "dummy-test-embedder"
        }
    }

    #[test]
    fn dummy_embedder_deterministic() {
        let emb = DummyEmbedder::new(4);
        let v1 = emb.embed("hello").unwrap();
        let v2 = emb.embed("hello").unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn dummy_embedder_normalized() {
        let emb = DummyEmbedder::new(8);
        let v = emb.embed("test").unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    fn dummy_embedder_different_texts() {
        let emb = DummyEmbedder::new(4);
        let v1 = emb.embed("cat").unwrap();
        let v2 = emb.embed("dog").unwrap();
        assert_ne!(v1, v2);
    }
}
