/// Shared LLM helpers for the debate module.

use crate::state::AppState;

/// Call the configured LLM with a request body. Returns the raw JSON response.
pub async fn call_llm(state: &AppState, request_body: serde_json::Value) -> Result<serde_json::Value, String> {
    let (endpoint, api_key, default_model) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        let model = cfg.llm_model.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_MODEL").ok());
        (ep, key, model)
    };

    let endpoint = endpoint.ok_or("LLM not configured")?;
    let model = request_body.get("model")
        .and_then(|m| m.as_str())
        .map(String::from)
        .or(default_model)
        .unwrap_or_else(|| "llama3.2".into());

    let messages = request_body.get("messages").cloned().unwrap_or(serde_json::json!([]));
    let temperature = request_body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(0.7);
    let max_tokens = request_body.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(2048);

    let url = super::super::admin::normalize_llm_endpoint(&endpoint);

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.post(&url).header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req.json(&body).send().await.map_err(|e| format!("LLM request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("LLM returned {status}: {text}"));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON from LLM: {e}"))
}

/// Extract text content from an LLM chat completion response.
pub fn extract_content(response: &serde_json::Value) -> Option<String> {
    response.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(String::from)
}

/// Parse JSON from LLM content (handles markdown code fences).
pub fn parse_json_from_llm(content: &str) -> Result<serde_json::Value, String> {
    if let Ok(v) = serde_json::from_str(content) {
        return Ok(v);
    }
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            if start < end {
                if let Ok(v) = serde_json::from_str(&content[start..=end]) {
                    return Ok(v);
                }
            }
        }
    }
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if start < end {
                if let Ok(v) = serde_json::from_str(&content[start..=end]) {
                    return Ok(v);
                }
            }
        }
    }
    Err(format!("Could not parse JSON from LLM response: {}", &content[..content.len().min(200)]))
}

/// Helper to extract a string array from a JSON value.
pub fn extract_string_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}
