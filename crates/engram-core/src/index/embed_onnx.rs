/// Local ONNX embedder — runs embedding models directly in Rust via ONNX Runtime.
///
/// For users who want zero-dependency semantic search without running Ollama or
/// an external API. Loads an ONNX model and tokenizer from sidecar files next to
/// the .brain file.
///
/// Expected sidecar files:
///   {brain_file}.model.onnx     — the ONNX embedding model
///   {brain_file}.tokenizer.json — HuggingFace tokenizer configuration
///
/// Recommended model: multilingual-e5-small (384 shape, ~120 MB, 100+ languages)
///
/// To export a model for engram:
///   pip install optimum[onnxruntime]
///   optimum-cli export onnx --model intfloat/multilingual-e5-small ./e5-small-onnx/
///   cp ./e5-small-onnx/model.onnx ./knowledge.brain.model.onnx
///   cp ./e5-small-onnx/tokenizer.json ./knowledge.brain.tokenizer.json

use super::embedding::{EmbedError, Embedder};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct OnnxEmbedder {
    session: Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
    dim: usize,
    model_path: PathBuf,
}

impl OnnxEmbedder {
    /// Load an ONNX embedding model from sidecar files next to the brain file.
    /// Returns None if the sidecar files don't exist.
    pub fn from_brain_path(brain_path: &Path) -> Option<Self> {
        let model_path = brain_path.with_extension("brain.model.onnx");
        let tokenizer_path = brain_path.with_extension("brain.tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            return None;
        }

        Self::load(&model_path, &tokenizer_path).ok()
    }

    /// Load from explicit paths.
    pub fn load(model_path: &Path, tokenizer_path: &Path) -> Result<Self, EmbedError> {
        let session = ort::session::Session::builder()
            .map_err(|e| EmbedError::RuntimeError(format!("ONNX session builder: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| EmbedError::RuntimeError(format!("ONNX threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| EmbedError::RuntimeError(format!("ONNX load model: {e}")))?;

        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| EmbedError::RuntimeError(format!("tokenizer: {e}")))?;

        // Probe dimension by running a test embedding
        let mut embedder = OnnxEmbedder {
            session: Mutex::new(session),
            tokenizer,
            dim: 0,
            model_path: model_path.to_path_buf(),
        };

        let probe = embedder.embed("dimension probe")?;
        embedder.dim = probe.len();

        tracing::info!(
            "ONNX embedder loaded: {} ({}D)",
            model_path.display(),
            embedder.dim
        );

        Ok(embedder)
    }

    /// Run inference on tokenized input.
    fn infer(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let encoding = self.tokenizer
            .encode(text, true)
            .map_err(|e| EmbedError::RuntimeError(format!("tokenize: {e}")))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();
        let seq_len = input_ids.len();

        // Build input tensors using ort's Tensor type
        let input_ids_tensor = ort::value::Tensor::from_array(
            ndarray::Array2::from_shape_vec((1, seq_len), input_ids)
                .map_err(|e| EmbedError::RuntimeError(format!("shape: {e}")))?
        ).map_err(|e| EmbedError::RuntimeError(format!("tensor: {e}")))?;

        let attention_mask_tensor = ort::value::Tensor::from_array(
            ndarray::Array2::from_shape_vec((1, seq_len), attention_mask)
                .map_err(|e| EmbedError::RuntimeError(format!("shape: {e}")))?
        ).map_err(|e| EmbedError::RuntimeError(format!("tensor: {e}")))?;

        let token_type_ids_tensor = ort::value::Tensor::from_array(
            ndarray::Array2::from_shape_vec((1, seq_len), token_type_ids)
                .map_err(|e| EmbedError::RuntimeError(format!("shape: {e}")))?
        ).map_err(|e| EmbedError::RuntimeError(format!("tensor: {e}")))?;

        let inputs = ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ];

        let mut session = self.session.lock()
            .map_err(|e| EmbedError::RuntimeError(format!("session lock: {e}")))?;

        let output_names: Vec<String> = session.outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        let outputs = session
            .run(inputs)
            .map_err(|e| EmbedError::RuntimeError(format!("inference: {e}")))?;
        let output_name = output_names.first()
            .ok_or_else(|| EmbedError::RuntimeError("model has no outputs".into()))?;
        let output_value = &outputs[output_name.as_str()];

        let (shape, data) = output_value.try_extract_tensor::<f32>()
            .map_err(|e| EmbedError::RuntimeError(format!("extract: {e}")))?;

        let embedding = if shape.len() == 3 {
            // (batch, seq_len, hidden_dim) — mean pool over seq_len
            let seq_len = shape[1] as usize;
            let hidden_dim = shape[2] as usize;
            let mut pooled = vec![0.0f32; hidden_dim];
            for s in 0..seq_len {
                for d in 0..hidden_dim {
                    pooled[d] += data[s * hidden_dim + d];
                }
            }
            for v in &mut pooled {
                *v /= seq_len as f32;
            }
            pooled
        } else if shape.len() == 2 {
            // (batch, hidden_dim) — already pooled
            let hidden_dim = shape[1] as usize;
            data[..hidden_dim].to_vec()
        } else {
            return Err(EmbedError::RuntimeError(
                format!("unexpected output shape: {shape:?}")
            ));
        };

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            Ok(embedding.into_iter().map(|v| v / norm).collect())
        } else {
            Ok(embedding)
        }
    }
}

impl Embedder for OnnxEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        self.infer(text)
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn model_id(&self) -> &str {
        self.model_path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("onnx-local")
    }
}
