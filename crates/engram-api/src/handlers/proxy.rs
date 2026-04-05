use super::*;

// ── GET /proxy/gdelt ── CORS proxy for GDELT API ──

/// Proxy GDELT requests to avoid CORS restrictions in browser.
/// Forwards query params to api.gdeltproject.org and returns the response.
pub async fn proxy_gdelt(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let mut url = "https://api.gdeltproject.org/api/v2/doc/doc?".to_string();
    let query_string: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect();
    url.push_str(&query_string.join("&"));

    let client = &state.http_client;

    let resp = client
        .get(&url)
        .header("User-Agent", "engram-intel/0.1")
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("GDELT request failed: {e}")))?;

    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let body = resp
        .bytes()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok((
        status,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

/// Proxy for Google News RSS -- fetches RSS XML and converts to JSON for the dashboard.
pub async fn proxy_news_rss(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "missing 'q' parameter".to_string()));
    }

    let rss_url = format!(
        "https://news.google.com/rss/search?q={}&hl=en&gl=US&ceid=US:en",
        urlencoding::encode(&query)
    );

    let client = &state.http_client;

    let resp = client
        .get(&rss_url)
        .header("User-Agent", "engram-intel/0.1")
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("RSS fetch failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, "RSS feed returned error".to_string()));
    }

    let xml = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    // Parse RSS XML into simple JSON array of {title, link, pubDate, domain}
    let mut items = Vec::new();
    for item_block in xml.split("<item>").skip(1) {
        let title = extract_xml_tag(item_block, "title").unwrap_or_default();
        let link = extract_xml_tag(item_block, "link").unwrap_or_default();
        let pub_date = extract_xml_tag(item_block, "pubDate").unwrap_or_default();
        let source = extract_xml_tag(item_block, "source").unwrap_or_default();

        if !title.is_empty() && !link.is_empty() {
            let domain = link.split('/').nth(2).unwrap_or("").to_string();
            items.push(serde_json::json!({
                "title": title,
                "link": link,
                "pubDate": pub_date,
                "source": source,
                "domain": domain,
            }));
        }
    }

    let body = serde_json::json!({ "items": items }).to_string();

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

// ── GET /proxy/search ── Web search via DuckDuckGo HTML ──

/// Proxy web search via Brave Search API or DuckDuckGo fallback.
/// Set ENGRAM_SEARCH_API_KEY for Brave Search, otherwise falls back to DuckDuckGo instant answers.
/// Returns JSON array of search results with title, url, snippet.
pub async fn proxy_web_search(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "missing 'q' parameter".to_string()));
    }

    let time_range = params.get("time_range").cloned().unwrap_or_default();

    let search_results = super::web_search::search_with_time_range(&state, &query, &time_range)
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e))?;

    let results: Vec<serde_json::Value> = search_results.iter()
        .map(|r| {
            let domain = r.url.split('/').nth(2).unwrap_or("");
            serde_json::json!({
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
                "domain": domain,
            })
        })
        .collect();

    let body = serde_json::json!({ "results": results }).to_string();

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

// ── POST /proxy/llm ── Forward chat completion requests to configured LLM ──

/// Proxy LLM chat completion requests to an OpenAI-compatible endpoint.
/// Uses ENGRAM_EMBED_ENDPOINT (same as embeddings) or ENGRAM_LLM_ENDPOINT if set.
/// Model defaults to ENGRAM_LLM_MODEL env var, then request body, then "llama3.2".
pub async fn proxy_llm(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    // Read config from AppState, then secrets store, then env vars.
    let (endpoint, api_key, default_model) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        // API key: secrets store first, then config (legacy), then env
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        let model = cfg.llm_model.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_MODEL").ok());
        (ep, key, model)
    };

    let endpoint = endpoint.ok_or_else(|| {
        api_err(StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured. Set endpoint via POST /config or ENGRAM_LLM_ENDPOINT env var.")
    })?;
    let default_model = default_model.unwrap_or_else(|| "llama3.2".to_string());

    let messages = body.get("messages").cloned().unwrap_or(serde_json::json!([]));
    let model = body.get("model")
        .and_then(|m| m.as_str())
        .unwrap_or(&default_model);
    let temperature = body.get("temperature")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.7);
    let max_tokens = body.get("max_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(1024);
    let tools = body.get("tools").cloned();

    let url = super::admin::normalize_llm_endpoint(&endpoint);

    let mut request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });
    // Pass through tools for function calling
    if let Some(tools_val) = tools {
        request_body["tools"] = tools_val;
    }

    let client = &state.http_client;

    let mut req = client.post(&url)
        .header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req
        .json(&request_body)
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("LLM request failed: {e}")))?;

    if !resp.status().is_success() {
        let status_code = resp.status().as_u16();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(api_err(
            StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("LLM returned {status_code}: {body_text}"),
        ));
    }

    let body_text = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    let json_value: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("invalid JSON from LLM: {e}")))?;

    Ok(Json(json_value))
}

/// GET /proxy/models -- fetch available models from the configured LLM endpoint (avoids CORS).
pub async fn proxy_llm_models(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let (endpoint, api_key) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        (ep, key)
    };

    let endpoint = endpoint.ok_or_else(|| {
        api_err(StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured. Set endpoint via POST /config or ENGRAM_LLM_ENDPOINT env var.")
    })?;

    // Build /v1/models URL from the raw endpoint
    let base = endpoint.trim().trim_end_matches('/');
    let models_url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        // Strip any path after host (e.g. /v1/chat/completions) and use /v1/models
        let after_scheme = base.strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
            .unwrap_or(base);
        let scheme_end = base.len() - after_scheme.len();
        let host_end = after_scheme.find('/').map(|i| scheme_end + i).unwrap_or(base.len());
        format!("{}/v1/models", &base[..host_end])
    };

    let client = &state.http_client;

    let mut req = client.get(&models_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req.send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Failed to reach LLM: {e}")))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_err(
            StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("LLM returned {code}: {body}"),
        ));
    }

    let body = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Invalid JSON from LLM: {e}")))?;

    Ok(Json(json))
}

/// POST /proxy/fetch-models -- fetch available models from ANY endpoint (for wizard/setup).
/// Body: { "endpoint": "http://localhost:11434/api/embed" }
/// Returns the raw JSON from {base}/api/tags (Ollama) or {base}/v1/models (OpenAI-compatible).
pub async fn proxy_fetch_models(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let endpoint = body.get("endpoint").and_then(|v| v.as_str()).unwrap_or("");
    if endpoint.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "Missing 'endpoint' field"));
    }

    // Derive base URL by stripping known paths
    let base = endpoint.trim().trim_end_matches('/')
        .replace("/v1/chat/completions", "")
        .replace("/v1/embeddings", "")
        .replace("/api/embed", "")
        .replace("/api/generate", "");

    let client = &state.http_client;

    // Try Ollama-style /api/tags first
    let ollama_url = format!("{base}/api/tags");
    if let Ok(resp) = client.get(&ollama_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("models").and_then(|m| m.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
                        return Ok(Json(json));
                    }
                }
            }
        }
    }

    // Try OpenAI-style /v1/models
    let openai_url = format!("{base}/v1/models");
    let api_key = body.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let mut req = client.get(&openai_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let resp = req.send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Failed to reach endpoint: {e}")))?;

    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("Endpoint returned {}", resp.status().as_u16())));
    }

    let text = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Invalid JSON: {e}")))?;

    Ok(Json(json))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after_open = &xml[start..];
    let content_start = after_open.find('>')? + 1;
    let content = &after_open[content_start..];
    let end = content.find(&close)?;
    let val = &content[..end];
    // Handle CDATA
    let val = val.trim();
    let val = if val.starts_with("<![CDATA[") && val.ends_with("]]>") {
        &val[9..val.len()-3]
    } else {
        val
    };
    Some(val.to_string())
}
