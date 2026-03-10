/// LLM-powered investigation query suggestions for knowledge gaps.
///
/// Uses an external LLM endpoint (Ollama, OpenAI, etc.) to generate
/// context-aware investigation queries for detected black areas.
/// Suggestions are NEVER auto-executed — they are displayed in the
/// frontend for human review and approval.

use crate::types::{BlackArea, BlackAreaKind};

/// Configuration for LLM suggestion generation.
#[derive(Debug, Clone)]
pub struct LlmSuggestionConfig {
    /// LLM API endpoint (OpenAI-compatible).
    pub endpoint: String,
    /// API key (if required).
    pub api_key: Option<String>,
    /// Model name.
    pub model: String,
    /// Maximum suggestions per gap.
    pub max_suggestions: usize,
}

impl Default for LlmSuggestionConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434/v1/chat/completions".into(),
            api_key: None,
            model: "llama3.2".into(),
            max_suggestions: 10,
        }
    }
}

impl LlmSuggestionConfig {
    /// Load config from environment variables.
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("ENGRAM_LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434/v1".into())
                + "/chat/completions",
            api_key: std::env::var("ENGRAM_LLM_API_KEY").ok(),
            model: std::env::var("ENGRAM_LLM_MODEL")
                .unwrap_or_else(|_| "llama3.2".into()),
            max_suggestions: 10,
        }
    }
}

/// Build an LLM prompt for a black area.
pub fn build_prompt(gap: &BlackArea) -> String {
    let kind_desc = match &gap.kind {
        BlackAreaKind::FrontierNode => "frontier nodes (entities with very few connections)",
        BlackAreaKind::StructuralHole => "a structural hole (expected transitive link missing)",
        BlackAreaKind::AsymmetricCluster => "asymmetric coverage (related topics with vastly different fact counts)",
        BlackAreaKind::TemporalGap => "temporal gaps (entities not updated recently)",
        BlackAreaKind::ConfidenceDesert => "a confidence desert (cluster of low-confidence facts)",
        BlackAreaKind::CoordinatedCluster => "a coordinated cluster (dense internal edges, suspicious pattern)",
    };

    let entities = gap.entities.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
    let domain_hint = gap.domain.as_deref().unwrap_or("unknown");
    let existing = if gap.suggested_queries.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nExisting mechanical queries (for context, do NOT repeat these):\n{}",
            gap.suggested_queries.iter().take(5).map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")
        )
    };

    format!(
        "You are an intelligence analyst. A knowledge graph has detected {kind_desc}.\n\
         \n\
         Domain: {domain_hint}\n\
         Entities involved: {entities}\n\
         Severity: {:.2}\n\
         {existing}\n\
         \n\
         Generate up to 10 specific, actionable search queries that would help fill this knowledge gap. \
         Each query should be a concrete search term or question, NOT a general instruction. \
         Output one query per line, no numbering, no bullets.",
        gap.severity,
    )
}

/// Build the JSON request body for the LLM API.
pub fn build_request(config: &LlmSuggestionConfig, gap: &BlackArea) -> serde_json::Value {
    let prompt = build_prompt(gap);
    serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": "You are a knowledge gap analyst. Output only search queries, one per line."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.7,
        "max_tokens": 500
    })
}

/// Parse LLM response text into individual query suggestions.
pub fn parse_suggestions(response_text: &str, max: usize) -> Vec<String> {
    response_text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| line.len() > 5) // skip very short lines
        .filter(|line| !line.starts_with('#') && !line.starts_with("```"))
        // Strip leading numbering like "1. " or "- "
        .map(|line| {
            let stripped = line
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '*')
                .trim();
            stripped.to_string()
        })
        .filter(|s| !s.is_empty())
        .take(max)
        .collect()
}

/// Extract the text content from an OpenAI-compatible chat completion response.
pub fn extract_content(response: &serde_json::Value) -> Option<String> {
    response
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_frontier() {
        let gap = BlackArea {
            kind: BlackAreaKind::FrontierNode,
            entities: vec!["Iran".into(), "sanctions".into()],
            severity: 0.7,
            suggested_queries: vec!["Iran related entities".into()],
            domain: Some("geopolitics".into()),
            detected_at: 0,
        };

        let prompt = build_prompt(&gap);
        assert!(prompt.contains("frontier nodes"));
        assert!(prompt.contains("Iran"));
        assert!(prompt.contains("geopolitics"));
        assert!(prompt.contains("Iran related entities")); // existing queries included
    }

    #[test]
    fn parse_suggestions_strips_numbering() {
        let text = "1. Iran sanctions timeline\n2. EU sanctions on Iran oil\n3. US JCPOA withdrawal impact\n";
        let queries = parse_suggestions(text, 10);
        assert_eq!(queries.len(), 3);
        assert_eq!(queries[0], "Iran sanctions timeline");
        assert_eq!(queries[1], "EU sanctions on Iran oil");
    }

    #[test]
    fn parse_suggestions_respects_max() {
        let text = "query1\nquery2\nquery3\nquery4\nquery5\n";
        let queries = parse_suggestions(text, 3);
        assert_eq!(queries.len(), 3);
    }

    #[test]
    fn extract_content_openai_format() {
        let resp = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "Iran nuclear program\nIran oil exports"
                }
            }]
        });
        let content = extract_content(&resp).unwrap();
        assert!(content.contains("Iran nuclear program"));
    }

    #[test]
    fn build_request_structure() {
        let config = LlmSuggestionConfig::default();
        let gap = BlackArea {
            kind: BlackAreaKind::TemporalGap,
            entities: vec!["Russia".into()],
            severity: 0.5,
            suggested_queries: vec![],
            domain: None,
            detected_at: 0,
        };
        let req = build_request(&config, &gap);
        assert!(req.get("model").is_some());
        assert!(req.get("messages").is_some());
    }
}
