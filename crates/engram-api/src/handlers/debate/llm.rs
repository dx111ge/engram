/// Shared LLM helpers for the debate module.
/// Handles both standard models and reasoning/thinking models (DeepSeek R1, QwQ, Qwen3, etc.)
///
/// Reasoning model quirks this module handles:
/// - Thinking in separate `reasoning` or `reasoning_content` field, `content` may be empty
/// - `<think>...</think>` tags embedded in `content` itself
/// - `finish_reason: "length"` when token budget exhausted during thinking phase
/// - Higher token requirements (thinking consumes tokens before answer)
///
/// Context window handling:
/// - Reads `llm_context_window` from EngineConfig (auto-detected or user-set)
/// - For Ollama: sends `options.num_ctx` to actually use the full window
/// - Caps `max_tokens` to never exceed context window
/// - Warns loudly if context window is unknown

use crate::state::AppState;

/// Conservative default when context window is unknown. Low enough to work everywhere.
const DEFAULT_CONTEXT_WINDOW: u32 = 8192;

/// Map ISO 639-1 codes to human-readable language names.
pub fn language_name(code: &str) -> &str {
    match code {
        "de" => "German", "fr" => "French", "es" => "Spanish", "it" => "Italian",
        "pt" => "Portuguese", "nl" => "Dutch", "ru" => "Russian", "uk" => "Ukrainian",
        "ar" => "Arabic", "zh" => "Chinese", "ja" => "Japanese", "ko" => "Korean",
        "pl" => "Polish", "tr" => "Turkish", "sv" => "Swedish", "da" => "Danish",
        "no" => "Norwegian", "fi" => "Finnish", "cs" => "Czech", "ro" => "Romanian",
        "hu" => "Hungarian", "el" => "Greek", "he" => "Hebrew", "hi" => "Hindi",
        "th" => "Thai", "vi" => "Vietnamese", "id" => "Indonesian",
        _ => code,
    }
}

/// Append output language instruction to a user-facing system prompt.
/// If `output_language` is configured and not "en", appends "Respond in {language}."
/// Use this for all LLM calls that produce user-visible text (synthesis, chat, assessments).
/// Do NOT use for internal calls (NER, gap detection, search query generation, agent reasoning).
pub fn user_facing_system_prompt(state: &AppState, base_prompt: &str) -> String {
    let lang = state.config.read().ok()
        .and_then(|c| c.output_language.clone())
        .unwrap_or_default();
    if lang.is_empty() || lang == "en" {
        return base_prompt.to_string();
    }
    let name = language_name(&lang);
    format!("{}\n\nRespond in {}.", base_prompt, name)
}

/// Multiplier for reasoning models: thinking uses ~3-5x the output tokens.
/// We need output_budget * REASONING_MULTIPLIER tokens available.
const REASONING_MULTIPLIER: u64 = 4;

/// Derive a sensible max_tokens for short structured output (JSON extraction, gap reading, etc.)
/// based on the configured context window. Returns min(context_window / 8, 2048).
pub fn short_output_budget(state: &AppState) -> u64 {
    let ctx = state.config.read()
        .ok()
        .and_then(|c| c.llm_context_window)
        .unwrap_or(DEFAULT_CONTEXT_WINDOW) as u64;
    (ctx / 8).min(2048).max(256)
}

/// Derive max_tokens for medium output (summaries, conclusions).
/// Returns min(context_window / 4, 4096).
pub fn medium_output_budget(state: &AppState) -> u64 {
    let ctx = state.config.read()
        .ok()
        .and_then(|c| c.llm_context_window)
        .unwrap_or(DEFAULT_CONTEXT_WINDOW) as u64;
    (ctx / 4).min(4096).max(512)
}

/// Known reasoning model name patterns. Lowercase match.
const REASONING_MODEL_PATTERNS: &[&str] = &[
    "deepseek-r1", "deepseek-reasoner", "qwq", "qwen3",
    "gemma4", "thinking", "reason", "r1-",
];

/// Check if a model name suggests it's a reasoning/thinking model.
fn is_likely_reasoning_model(model: &str) -> bool {
    let lower = model.to_lowercase();
    REASONING_MODEL_PATTERNS.iter().any(|p| lower.contains(p))
}

/// Check if an LLM response came from a reasoning model (has reasoning field or think tags).
fn response_has_reasoning(response: &serde_json::Value) -> bool {
    response.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .map(|m| {
            m.get("reasoning").and_then(|r| r.as_str()).is_some_and(|s| !s.is_empty())
            || m.get("reasoning_content").and_then(|r| r.as_str()).is_some_and(|s| !s.is_empty())
        })
        .unwrap_or(false)
}

/// Detect if the endpoint is Ollama-style (port 11434 or /api/ in path).
fn is_ollama_endpoint(endpoint: &str) -> bool {
    endpoint.contains(":11434") || endpoint.contains("/api/")
}

/// Call the configured LLM with a request body. Returns the raw JSON response.
///
/// Token budgeting:
/// - Reads `llm_context_window` and `llm_thinking` from config
/// - For thinking models: multiplies requested max_tokens by REASONING_MULTIPLIER
/// - Caps max_tokens to context_window (never send more than the model supports)
/// - For Ollama: injects `options.num_ctx` so Ollama actually uses the full context
pub async fn call_llm(state: &AppState, request_body: serde_json::Value) -> Result<serde_json::Value, String> {
    let (endpoint, api_key, default_model, is_thinking, context_window) = {
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
        let thinking = cfg.llm_thinking.unwrap_or(false);
        let ctx = cfg.llm_context_window;
        (ep, key, model, thinking, ctx)
    };

    let endpoint = endpoint.ok_or("LLM not configured")?;
    let model = request_body.get("model")
        .and_then(|m| m.as_str())
        .map(String::from)
        .or(default_model)
        .unwrap_or_else(|| "llama3.2".into());

    let messages = request_body.get("messages").cloned().unwrap_or(serde_json::json!([]));
    let temperature = request_body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(0.7);
    let requested_tokens = request_body.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(2048);

    // ── Thinking toggle ──
    // Per-request "think" field overrides the global config.
    // true  = enable thinking (deep analysis, agent turns)
    // false = disable thinking (structured JSON output, speed)
    // null  = use global config default
    let think_override = request_body.get("think").and_then(|t| t.as_bool());
    let is_thinking_model = is_thinking || is_likely_reasoning_model(&model);
    let use_thinking = think_override.unwrap_or(is_thinking_model);

    // ── Token budgeting ──
    let ctx_window = context_window.unwrap_or_else(|| {
        eprintln!("[llm] WARNING: llm_context_window not set. Using conservative default ({DEFAULT_CONTEXT_WINDOW}). \
                   Set it via POST /config {{\"llm_context_window\": 32768}} or it will be auto-detected on model change.");
        DEFAULT_CONTEXT_WINDOW
    }) as u64;

    let max_tokens = if use_thinking {
        // Thinking models need budget for thinking + answer.
        let budget = requested_tokens * REASONING_MULTIPLIER;
        budget.min(ctx_window)
    } else {
        // No thinking -- requested tokens are sufficient
        requested_tokens.min(ctx_window)
    };

    let url = super::super::admin::normalize_llm_endpoint(&endpoint);

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });

    // For Ollama: inject options.num_ctx and thinking toggle
    if is_ollama_endpoint(&endpoint) {
        body.as_object_mut().unwrap().insert(
            "options".to_string(),
            serde_json::json!({"num_ctx": ctx_window}),
        );
        // Toggle thinking on/off per request (Ollama "think" field).
        // Always send when explicitly requested (think_override), or when model is a known thinker.
        // This ensures think=false actually suppresses thinking on models like gemma4 that think by default.
        if is_thinking_model || think_override.is_some() {
            body.as_object_mut().unwrap().insert(
                "think".to_string(),
                serde_json::json!(use_thinking),
            );
        }
    }

    let client = &state.http_client;

    let prompt_len: usize = messages.as_array().map(|a| a.iter().map(|m| m.get("content").and_then(|c| c.as_str()).map(|s| s.len()).unwrap_or(0)).sum()).unwrap_or(0);
    let t_llm = std::time::Instant::now();
    dbg_debate!("[llm] >> CALL model={} think={} max_tokens={} prompt_chars={} url={}",
        model, use_thinking, max_tokens, prompt_len, &url[..url.len().min(60)]);

    // Log full prompt when debug is on
    if crate::handlers::debate::DEBATE_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(msgs) = messages.as_array() {
            for (i, m) in msgs.iter().enumerate() {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("?");
                let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                dbg_debate!("[llm] prompt[{}] role={} chars={}\n{}", i, role, content.len(), content);
            }
        }
    }

    let mut req = client.post(&url).header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req.json(&body).send().await.map_err(|e| {
        dbg_debate!("[llm] << ERROR model={} send_failed took={:.1}s err={}",
            model, t_llm.elapsed().as_secs_f32(), e);
        format!("LLM request failed: {e}")
    })?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        dbg_debate!("[llm] << ERROR model={} http_status={} took={:.1}s",
            model, status, t_llm.elapsed().as_secs_f32());
        return Err(format!("LLM returned {status}: {text}"));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let response: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("invalid JSON from LLM: {e}"))?;

    let content_len = extract_content(&response).map(|c| c.len()).unwrap_or(0);
    let finish = response.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("finish_reason")).and_then(|f| f.as_str()).unwrap_or("?");
    dbg_debate!("[llm] << DONE model={} finish={} content_chars={} took={:.1}s",
        model, finish, content_len, t_llm.elapsed().as_secs_f32());

    // Warn if reasoning model exhausted tokens (content will likely be empty/truncated)
    if let Some(choice) = response.get("choices").and_then(|c| c.get(0)) {
        let finish = choice.get("finish_reason").and_then(|f| f.as_str()).unwrap_or("");
        if finish == "length" {
            if response_has_reasoning(&response) {
                eprintln!("[llm] WARNING: reasoning model hit token limit (max_tokens={max_tokens}, context_window={ctx_window}). \
                           Answer likely incomplete. Increase llm_context_window via POST /config.");
            } else {
                eprintln!("[llm] WARNING: model hit token limit (max_tokens={max_tokens}). Response may be truncated.");
            }
        }
    }

    Ok(response)
}

// ── Auto-detection of context window ───────────────────────────────────

/// Query the LLM provider for model metadata and return the context window size.
/// Supports Ollama (`/api/show`) and OpenAI-compatible (`/v1/models/{model}`).
pub async fn detect_context_window(endpoint: &str, model: &str, api_key: &str) -> Option<u32> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build().ok()?;

    let base = endpoint.trim().trim_end_matches('/')
        .replace("/v1/chat/completions", "")
        .replace("/v1/completions", "")
        .replace("/api/chat", "")
        .replace("/api/generate", "");

    // Try Ollama /api/show first
    if let Some(ctx) = detect_ollama_context(&client, &base, model).await {
        return Some(ctx);
    }

    // Try OpenAI /v1/models/{model}
    if let Some(ctx) = detect_openai_context(&client, &base, model, api_key).await {
        return Some(ctx);
    }

    None
}

async fn detect_ollama_context(client: &reqwest::Client, base: &str, model: &str) -> Option<u32> {
    let url = format!("{base}/api/show");
    let body = serde_json::json!({"name": model});
    let resp = client.post(&url).json(&body).send().await.ok()?;
    if !resp.status().is_success() { return None; }
    let json: serde_json::Value = resp.json().await.ok()?;

    // Ollama returns model_info with context_length, or parameters with num_ctx
    if let Some(ctx) = json.get("model_info")
        .and_then(|mi| {
            // Keys vary: "general.context_length", "llama.context_length", etc.
            mi.as_object()?.iter()
                .find(|(k, _)| k.contains("context_length"))
                .and_then(|(_, v)| v.as_u64())
        }) {
        return Some(ctx as u32);
    }

    // Fallback: parse parameters string for num_ctx
    if let Some(params) = json.get("parameters").and_then(|p| p.as_str()) {
        for line in params.lines() {
            let line = line.trim();
            if line.starts_with("num_ctx") {
                if let Some(val) = line.split_whitespace().last().and_then(|v| v.parse::<u32>().ok()) {
                    return Some(val);
                }
            }
        }
    }

    None
}

async fn detect_openai_context(client: &reqwest::Client, base: &str, model: &str, api_key: &str) -> Option<u32> {
    let url = format!("{base}/v1/models/{model}");
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() { return None; }
    let json: serde_json::Value = resp.json().await.ok()?;

    // OpenAI returns "context_window" or "max_model_len" depending on provider
    json.get("context_window").and_then(|v| v.as_u64())
        .or_else(|| json.get("max_model_len").and_then(|v| v.as_u64()))
        .or_else(|| json.get("context_length").and_then(|v| v.as_u64()))
        .map(|v| v as u32)
}

/// Try to detect if a model is a thinking/reasoning model from its metadata.
pub async fn detect_thinking_model(endpoint: &str, model: &str) -> bool {
    // Check by name pattern first
    if is_likely_reasoning_model(model) {
        return true;
    }

    // Could also check API metadata, but name patterns cover the common cases
    let _ = endpoint;
    false
}

// ── Content extraction ─────────────────────────────────────────────────

/// Extract the usable text content from an LLM chat completion response.
///
/// Handles three model families:
/// 1. **Standard models**: answer in `content`
/// 2. **Reasoning models (separate field)**: thinking in `reasoning`/`reasoning_content`, answer in `content`
/// 3. **Reasoning models (inline)**: `<think>...</think>` tags in `content`, answer after tags
///
/// When `content` is empty but reasoning exists (token exhaustion), attempts to
/// extract structured data (JSON) from the reasoning field as a fallback.
pub fn extract_content(response: &serde_json::Value) -> Option<String> {
    let message = response.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))?;

    // ── Step 1: Try "content" field ──
    let raw_content = message.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let content = strip_think_tags(raw_content);

    if !content.is_empty() {
        return Some(content);
    }

    // ── Step 2: Content empty -- try reasoning fields (token exhaustion fallback) ──
    let reasoning = message.get("reasoning").and_then(|r| r.as_str())
        .or_else(|| message.get("reasoning_content").and_then(|r| r.as_str()))
        .unwrap_or("");

    if !reasoning.is_empty() {
        // Try to salvage structured JSON from reasoning output
        if let Some(json) = extract_last_json(reasoning) {
            eprintln!("[llm] Recovered JSON from reasoning field (content was empty)");
            return Some(json);
        }

        // Last resort: return reasoning text itself
        eprintln!("[llm] Using reasoning text as content fallback ({} chars)", reasoning.len());
        return Some(reasoning.to_string());
    }

    // Content field exists but is truly empty, no reasoning available
    if message.get("content").is_some() {
        return Some(String::new());
    }

    None
}

/// Strip `<think>...</think>` blocks from content.
fn strip_think_tags(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.contains("<think>") {
        return trimmed.to_string();
    }

    let mut result = String::with_capacity(trimmed.len());
    let mut remaining = trimmed;

    while let Some(start) = remaining.find("<think>") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find("</think>") {
            remaining = &remaining[start + end + "</think>".len()..];
        } else {
            break;
        }
    }
    result.push_str(remaining);
    result.trim().to_string()
}

/// Try to extract the last valid JSON array or object from text.
fn extract_last_json(text: &str) -> Option<String> {
    // Try array first (most common for gap/moderator responses)
    if let Some(end) = text.rfind(']') {
        let search_region = &text[..=end];
        let mut depth = 0i32;
        for (i, ch) in search_region.char_indices().rev() {
            match ch {
                ']' => depth += 1,
                '[' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &text[i..=end];
                        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                            return Some(candidate.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Try object
    if let Some(end) = text.rfind('}') {
        let search_region = &text[..=end];
        let mut depth = 0i32;
        for (i, ch) in search_region.char_indices().rev() {
            match ch {
                '}' => depth += 1,
                '{' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &text[i..=end];
                        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                            return Some(candidate.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    None
}

/// Parse JSON from LLM content (handles markdown code fences, thinking tags, preamble, etc.).
pub fn parse_json_from_llm(content: &str) -> Result<serde_json::Value, String> {
    let cleaned = strip_think_tags(content);
    let text = cleaned.trim();

    // Direct parse
    if let Ok(v) = serde_json::from_str(text) {
        return Ok(v);
    }

    // Strip markdown code fences
    let unfenced = if text.contains("```") {
        text.lines()
            .skip_while(|l| !l.starts_with("```"))
            .skip(1)
            .take_while(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text.to_string()
    };

    if !unfenced.is_empty() {
        if let Ok(v) = serde_json::from_str(&unfenced) {
            return Ok(v);
        }
    }

    // Extract JSON using bracket matching
    if let Some(json) = extract_last_json(text) {
        if let Ok(v) = serde_json::from_str(&json) {
            return Ok(v);
        }
    }

    // Legacy fallback: first [ to last ], first { to last }
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if start < end {
                if let Ok(v) = serde_json::from_str(&text[start..=end]) {
                    return Ok(v);
                }
            }
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if start < end {
                if let Ok(v) = serde_json::from_str(&text[start..=end]) {
                    return Ok(v);
                }
            }
        }
    }

    Err(format!("Could not parse JSON from LLM response: {}", &text[..text.len().min(200)]))
}

/// Helper to extract a string array from a JSON value.
pub fn extract_string_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}
