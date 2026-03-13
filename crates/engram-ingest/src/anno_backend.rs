/// Anno NER backend: GLiNER2 zero-shot NER via the `anno` crate (candle backend).
///
/// Uses the `anno` crate with pure-Rust candle ML backend -- no ONNX Runtime
/// dependency, no ort version conflict, no subprocess. Models are downloaded
/// from HuggingFace Hub on first use and cached locally.
///
/// Includes optional coreference resolution via MentionRankingCoref (rule-based)
/// to resolve pronouns and noun phrases to canonical entity names before RE.
///
/// This module is compiled only with `--features anno`.

#[cfg(feature = "anno")]
use crate::traits::Extractor;
#[cfg(feature = "anno")]
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// Configuration for the anno NER + coreference backend.
#[derive(Debug, Clone)]
pub struct AnnoConfig {
    /// HuggingFace model ID for GLiNER2 (e.g. "urchade/gliner_multi-v2.1").
    /// If empty, uses the default model.
    pub model_id: String,
    /// Entity types to extract (zero-shot labels).
    pub entity_types: Vec<String>,
    /// Minimum confidence threshold for extracted entities.
    pub min_confidence: f32,
    /// Enable coreference resolution after NER.
    pub coreference_enabled: bool,
}

impl Default for AnnoConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            entity_types: vec![
                "person".into(),
                "organization".into(),
                "location".into(),
                "date".into(),
                "event".into(),
                "product".into(),
            ],
            min_confidence: 0.3,
            coreference_enabled: true,
        }
    }
}

/// GLiNER2 NER backend via anno crate (candle, in-process).
///
/// Feature-gated: only compiled with `--features anno`.
#[cfg(feature = "anno")]
pub struct AnnoBackend {
    config: AnnoConfig,
    model: anno::backends::GLiNER2Candle,
    coref: Option<anno::backends::coref::mention_ranking::MentionRankingCoref>,
}

#[cfg(feature = "anno")]
impl AnnoBackend {
    /// Create a new anno NER backend.
    ///
    /// Downloads the model from HuggingFace on first use (cached via hf-hub).
    /// Pass an empty `model_id` to use the default GLiNER2 model.
    pub fn new(config: AnnoConfig) -> Result<Self, crate::IngestError> {
        let model_id = if config.model_id.is_empty() {
            "urchade/gliner_multi-v2.1"
        } else {
            &config.model_id
        };

        tracing::info!(model = %model_id, "loading GLiNER2 candle model");

        let model = anno::backends::GLiNER2Candle::from_pretrained(model_id)
            .map_err(|e| crate::IngestError::Config(format!(
                "failed to load GLiNER2 model '{}': {}", model_id, e
            )))?;

        let coref = if config.coreference_enabled {
            Some(anno::backends::coref::mention_ranking::MentionRankingCoref::new())
        } else {
            None
        };

        tracing::info!(
            model = %model_id,
            coref = config.coreference_enabled,
            "anno NER backend ready (candle)"
        );

        Ok(Self { config, model, coref })
    }

    /// Check if the backend is ready for inference.
    pub fn is_ready(&self) -> bool {
        true // Model is loaded in constructor; if we got here, it's ready
    }

    /// Run coreference resolution on text.
    /// Returns a mapping from mention text to canonical (longest) mention text.
    fn resolve_coreferences(&self, text: &str) -> std::collections::HashMap<String, String> {
        let mut mapping = std::collections::HashMap::new();

        if let Some(ref coref) = self.coref {
            match coref.resolve(text) {
                Ok(clusters) => {
                    for cluster in &clusters {
                        // Find the canonical mention (longest text = most informative)
                        let canonical = cluster.mentions
                            .iter()
                            .max_by_key(|m| m.text.len())
                            .map(|m| m.text.clone());

                        if let Some(ref canonical_text) = canonical {
                            for mention in &cluster.mentions {
                                if &mention.text != canonical_text {
                                    mapping.insert(mention.text.clone(), canonical_text.clone());
                                }
                            }
                        }
                    }
                    if !mapping.is_empty() {
                        tracing::debug!(
                            mappings = mapping.len(),
                            "coreference resolved {} mentions",
                            mapping.len()
                        );
                    }
                }
                Err(e) => {
                    tracing::debug!("coreference resolution skipped: {e}");
                }
            }
        }

        mapping
    }
}

#[cfg(feature = "anno")]
impl Extractor for AnnoBackend {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let _ = lang; // GLiNER handles multilingual natively

        // Run zero-shot NER with custom entity types via ZeroShotNER trait
        use anno::ZeroShotNER;
        let labels: Vec<&str> = self.config.entity_types.iter().map(|s| s.as_str()).collect();

        let entities = match self.model.extract_with_types(text, &labels, self.config.min_confidence) {
            Ok(ents) => ents,
            Err(e) => {
                tracing::warn!(error = %e, "GLiNER2 candle inference failed");
                return Vec::new();
            }
        };

        // Run coreference resolution to get pronoun -> canonical mappings
        let coref_map = self.resolve_coreferences(text);

        // Convert anno entities to engram ExtractedEntity
        entities
            .into_iter()
            .map(|e| {
                let entity_text = e.text.clone();
                // If coreference resolved this mention, use the canonical name
                let resolved_text = coref_map.get(&entity_text).cloned();

                ExtractedEntity {
                    text: resolved_text.as_deref().unwrap_or(&entity_text).to_string(),
                    entity_type: e.entity_type.as_label().to_string(),
                    span: (e.start, e.end),
                    confidence: e.confidence.value() as f32,
                    method: ExtractionMethod::StatisticalModel,
                    language: String::new(),
                    resolved_to: None,
                }
            })
            .collect()
    }

    fn name(&self) -> &str {
        "gliner2-candle"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        // GLiNER Multi supports 100+ languages
        vec![]
    }
}

/// Get the user's home directory.
fn dirs_next() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// Check if a NER model is available (HuggingFace model ID or local path).
///
/// For candle backend, models are downloaded from HuggingFace on first use.
/// This function checks if a model directory exists locally (legacy ONNX models)
/// or returns a config with the model ID for HuggingFace download.
pub fn find_ner_model(model_name: &str) -> Option<AnnoConfig> {
    let home = dirs_next()?;
    let model_dir = home.join(".engram").join("models").join("ner").join(model_name);

    // Check for local model (legacy ONNX or downloaded safetensors)
    let has_onnx = model_dir.join("model.onnx").exists();
    let has_safetensors = model_dir.join("model.safetensors").exists()
        || model_dir.join("gliner_model.safetensors").exists();

    if has_onnx || has_safetensors {
        return Some(AnnoConfig {
            model_id: model_name.to_string(),
            ..Default::default()
        });
    }

    // For HuggingFace models (e.g. "urchade/gliner_multi-v2.1"),
    // always return a config -- anno handles download + caching via hf-hub
    if model_name.contains('/') {
        return Some(AnnoConfig {
            model_id: model_name.to_string(),
            ..Default::default()
        });
    }

    None
}

/// List installed NER models (local models only).
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
            let path = entry.path();
            let has_model = path.join("model.onnx").exists()
                || path.join("model.safetensors").exists()
                || path.join("gliner_model.safetensors").exists();
            if has_model {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anno_config_defaults() {
        let config = AnnoConfig::default();
        assert_eq!(config.entity_types.len(), 6);
        assert!(config.entity_types.contains(&"person".to_string()));
        assert!(config.entity_types.contains(&"organization".to_string()));
        assert!(config.entity_types.contains(&"location".to_string()));
        assert!((config.min_confidence - 0.3).abs() < f32::EPSILON);
        assert!(config.coreference_enabled);
        assert!(config.model_id.is_empty());
    }

    #[test]
    fn test_find_ner_model_hf_id() {
        // HuggingFace model IDs with "/" always return Some
        let result = find_ner_model("urchade/gliner_multi-v2.1");
        assert!(result.is_some());
        let cfg = result.unwrap();
        assert_eq!(cfg.model_id, "urchade/gliner_multi-v2.1");
    }

    #[test]
    fn test_find_ner_model_nonexistent_local() {
        // Non-existent local model without "/" returns None
        let result = find_ner_model("nonexistent_model_xyz_123");
        assert!(result.is_none());
    }
}
