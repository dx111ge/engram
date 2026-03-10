/// Anno NER backend: feature-gated ONNX model integration.
///
/// Uses GLiNER2 or similar zero-shot NER models via ONNX runtime.
/// The model is user-installed (`engram model install ner <id>`).
///
/// This module is compiled only with `--features anno`.

#[cfg(feature = "anno")]
use crate::traits::Extractor;
#[cfg(feature = "anno")]
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// Configuration for the ONNX NER backend.
#[derive(Debug, Clone)]
pub struct AnnoConfig {
    /// Path to the ONNX model file.
    pub model_path: std::path::PathBuf,
    /// Path to the tokenizer JSON.
    pub tokenizer_path: std::path::PathBuf,
    /// Entity types to extract (for zero-shot models).
    pub entity_types: Vec<String>,
    /// Minimum confidence threshold for extracted entities.
    pub min_confidence: f32,
    /// Maximum sequence length for the model.
    pub max_seq_length: usize,
}

impl Default for AnnoConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::from("model.onnx"),
            tokenizer_path: std::path::PathBuf::from("tokenizer.json"),
            entity_types: vec![
                "PERSON".into(),
                "ORG".into(),
                "LOC".into(),
                "DATE".into(),
                "EVENT".into(),
            ],
            min_confidence: 0.5,
            max_seq_length: 512,
        }
    }
}

/// ONNX-based NER backend.
///
/// Loads an ONNX NER model (e.g. GLiNER2) and runs inference.
/// Feature-gated: only compiled with `--features anno`.
#[cfg(feature = "anno")]
pub struct AnnoBackend {
    config: AnnoConfig,
    // TODO(8.7): ONNX runtime session will be held here.
    // Requires `ort` crate (ONNX Runtime for Rust) as a dependency
    // when the anno feature is fully implemented.
}

#[cfg(feature = "anno")]
impl AnnoBackend {
    /// Create a new ONNX NER backend.
    ///
    /// Returns an error if the model files don't exist or can't be loaded.
    pub fn new(config: AnnoConfig) -> Result<Self, crate::IngestError> {
        if !config.model_path.exists() {
            return Err(crate::IngestError::Config(format!(
                "NER model not found: {}. Run: engram model install ner <model-id>",
                config.model_path.display()
            )));
        }
        if !config.tokenizer_path.exists() {
            return Err(crate::IngestError::Config(format!(
                "tokenizer not found: {}",
                config.tokenizer_path.display()
            )));
        }

        Ok(Self { config })
    }

    /// Check if the model is ready for inference.
    pub fn is_ready(&self) -> bool {
        self.config.model_path.exists() && self.config.tokenizer_path.exists()
    }
}

#[cfg(feature = "anno")]
impl Extractor for AnnoBackend {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        // TODO(8.7): Actual ONNX inference implementation.
        // This will:
        // 1. Tokenize `text` using the loaded tokenizer
        // 2. Run inference through the ONNX session
        // 3. Decode token-level predictions into entity spans
        // 4. Filter by min_confidence
        // 5. Return ExtractedEntity list
        //
        // For now, return empty — the model loading infrastructure
        // needs the `ort` crate which is added when anno feature is
        // fully wired.

        tracing::warn!(
            model = %self.config.model_path.display(),
            "anno backend: ONNX inference not yet implemented"
        );

        let _ = lang; // will be used for language-specific tokenization
        Vec::new()
    }

    fn name(&self) -> &str {
        "anno-onnx"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        // Depends on the installed model — GLiNER supports 20+ languages
        vec![]
    }
}

/// Check if an NER model is installed at the standard location.
pub fn find_ner_model(model_name: &str) -> Option<AnnoConfig> {
    // Standard model location: ~/.engram/models/ner/<model-name>/
    let home = dirs_next().unwrap_or_default();
    let model_dir = home.join(".engram").join("models").join("ner").join(model_name);

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if model_path.exists() && tokenizer_path.exists() {
        Some(AnnoConfig {
            model_path,
            tokenizer_path,
            ..Default::default()
        })
    } else {
        None
    }
}

/// Get the user's home directory.
fn dirs_next() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// List installed NER models.
pub fn list_installed_models() -> Vec<String> {
    let home = match dirs_next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let models_dir = home.join(".engram").join("models").join("ner");
    if !models_dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(&models_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.path().join("model.onnx").exists() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect()
}
