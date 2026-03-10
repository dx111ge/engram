/// SpaCy HTTP sidecar NER backend.
///
/// Sends text to a SpaCy server (running as a sidecar process) via HTTP
/// and parses the entity response. Feature-gated behind `spacy`.
///
/// Expected SpaCy endpoint:
///   POST http://localhost:8081/ner
///   Body: {"text": "..."}
///   Response: {"entities": [{"text": "...", "label": "...", "start": N, "end": N}]}

use crate::traits::Extractor;
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// Configuration for the SpaCy sidecar.
#[derive(Debug, Clone)]
pub struct SpacyConfig {
    /// Base URL of the SpaCy HTTP server.
    pub url: String,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Minimum confidence to accept.
    pub min_confidence: f32,
}

impl Default for SpacyConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8081".into(),
            timeout_ms: 5000,
            min_confidence: 0.5,
        }
    }
}

/// SpaCy HTTP NER backend.
pub struct SpacyBackend {
    config: SpacyConfig,
}

impl SpacyBackend {
    pub fn new(config: SpacyConfig) -> Self {
        Self { config }
    }
}

impl Extractor for SpacyBackend {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        // Synchronous HTTP call to SpaCy sidecar.
        // Uses a blocking client since Extractor::extract is sync.
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(self.config.timeout_ms))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("spacy: failed to build HTTP client: {}", e);
                return Vec::new();
            }
        };

        let url = format!("{}/ner", self.config.url);
        let body = serde_json::json!({"text": text, "lang": &lang.code});

        let response = match client.post(&url).json(&body).send() {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("spacy: request failed (sidecar not running?): {}", e);
                return Vec::new();
            }
        };

        let json: serde_json::Value = match response.json() {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("spacy: invalid response: {}", e);
                return Vec::new();
            }
        };

        // Parse SpaCy response format
        let entities = json["entities"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        entities
            .iter()
            .filter_map(|ent| {
                let text_val = ent["text"].as_str()?;
                let label = ent["label"].as_str()?;
                let start = ent["start"].as_u64()? as usize;
                let end = ent["end"].as_u64()? as usize;
                let conf = ent["confidence"].as_f64().unwrap_or(0.75) as f32;

                if conf < self.config.min_confidence {
                    return None;
                }

                Some(ExtractedEntity {
                    text: text_val.to_string(),
                    entity_type: label.to_string(),
                    span: (start, end),
                    confidence: conf,
                    method: ExtractionMethod::StatisticalModel,
                    language: lang.code.clone(),
                    resolved_to: None,
                })
            })
            .collect()
    }

    fn name(&self) -> &str {
        "spacy-http"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // depends on SpaCy model loaded in sidecar
    }
}
