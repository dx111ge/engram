/// LLM fallback NER backend.
///
/// Uses an LLM endpoint (Ollama, OpenAI, vLLM) as a last-resort NER.
/// Always produces the lowest confidence results and is clearly marked
/// as LLM-generated. Feature-gated behind `llm-ner`.
///
/// Restrictions (from design doc):
/// - Always lowest priority in NER chain
/// - Confidence capped at 0.10 (LLM_CAP)
/// - Every extraction tagged with method: LlmFallback
/// - Results require human/corroboration confirmation to rise above cap

use crate::traits::Extractor;
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// LLM confidence cap — LLM-extracted entities never exceed this.
const LLM_CONFIDENCE_CAP: f32 = 0.10;

/// Configuration for the LLM NER backend.
#[derive(Debug, Clone)]
pub struct LlmNerConfig {
    /// LLM API endpoint URL.
    pub endpoint: String,
    /// Model name (e.g. "llama3", "gpt-4").
    pub model: String,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Entity types to ask the LLM to extract.
    pub entity_types: Vec<String>,
}

impl Default for LlmNerConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434/api/generate".into(),
            model: "llama3".into(),
            timeout_ms: 30000,
            entity_types: vec![
                "PERSON".into(),
                "ORG".into(),
                "LOC".into(),
                "DATE".into(),
                "EVENT".into(),
            ],
        }
    }
}

/// LLM-based NER backend (last resort).
pub struct LlmNerBackend {
    config: LlmNerConfig,
}

impl LlmNerBackend {
    pub fn new(config: LlmNerConfig) -> Self {
        Self { config }
    }

    /// Build the NER prompt for the LLM.
    fn build_prompt(&self, text: &str) -> String {
        let types = self.config.entity_types.join(", ");
        format!(
            "Extract named entities from the following text. \
             Return ONLY a JSON array of objects with fields: \
             \"text\" (entity surface form), \"type\" (one of: {}), \
             \"start\" (character offset), \"end\" (character offset). \
             No explanation, no markdown, just the JSON array.\n\nText: {}",
            types, text
        )
    }
}

impl Extractor for LlmNerBackend {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(self.config.timeout_ms))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("llm-ner: failed to build HTTP client: {}", e);
                return Vec::new();
            }
        };

        let prompt = self.build_prompt(text);
        let body = serde_json::json!({
            "model": &self.config.model,
            "prompt": prompt,
            "stream": false,
        });

        let response = match client.post(&self.config.endpoint).json(&body).send() {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("llm-ner: request failed: {}", e);
                return Vec::new();
            }
        };

        let json: serde_json::Value = match response.json() {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("llm-ner: invalid response: {}", e);
                return Vec::new();
            }
        };

        // Parse LLM response — expect JSON array in "response" field (Ollama format)
        let response_text = json["response"].as_str().unwrap_or("");
        let entities_json: Vec<serde_json::Value> = serde_json::from_str(response_text)
            .unwrap_or_default();

        entities_json
            .iter()
            .filter_map(|ent| {
                let entity_text = ent["text"].as_str()?;
                let entity_type = ent["type"].as_str()?;
                let start = ent["start"].as_u64().unwrap_or(0) as usize;
                let end = ent["end"].as_u64().unwrap_or(entity_text.len() as u64) as usize;

                Some(ExtractedEntity {
                    text: entity_text.to_string(),
                    entity_type: entity_type.to_string(),
                    span: (start, end),
                    confidence: LLM_CONFIDENCE_CAP, // always capped
                    method: ExtractionMethod::LlmFallback,
                    language: lang.code.clone(),
                    resolved_to: None,
                })
            })
            .collect()
    }

    fn name(&self) -> &str {
        "llm-fallback"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::LlmFallback
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // LLMs are typically multilingual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_confidence_cap_is_low() {
        assert!(LLM_CONFIDENCE_CAP <= 0.10);
    }

    #[test]
    fn prompt_includes_entity_types() {
        let backend = LlmNerBackend::new(LlmNerConfig::default());
        let prompt = backend.build_prompt("Apple released a new iPhone");
        assert!(prompt.contains("PERSON"));
        assert!(prompt.contains("ORG"));
        assert!(prompt.contains("Apple released a new iPhone"));
    }
}
