use super::*;

// ── GET /proxy/gdelt ── CORS proxy for GDELT API ──

/// Proxy GDELT requests to avoid CORS restrictions in browser.
/// Forwards query params to api.gdeltproject.org and returns the response.
pub async fn proxy_gdelt(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let mut url = "https://api.gdeltproject.org/api/v2/doc/doc?".to_string();
    let query_string: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect();
    url.push_str(&query_string.join("&"));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut results = Vec::new();

    // Read web search config from AppState
    let (provider, api_key, search_base_url) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.web_search_provider.clone().unwrap_or_else(|| "searxng".to_string()),
            cfg.web_search_api_key.clone().unwrap_or_default(),
            cfg.web_search_url.clone(),
        )
    };

    // Optional time_range: "day", "week", "month", "year"
    let time_range = params.get("time_range").cloned().unwrap_or_default();

    match provider.as_str() {
        "brave" => {
            // Brave Search API
            let brave_url = format!(
                "https://api.search.brave.com/res/v1/web/search?q={}",
                urlencoding::encode(&query),
            );
            let resp = client
                .get(&brave_url)
                .header("Accept", "application/json")
                .header("X-Subscription-Token", &api_key)
                .send()
                .await;

            if let Ok(resp) = resp {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(web_results) = data.pointer("/web/results").and_then(|r| r.as_array()) {
                            for r in web_results.iter().take(50) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or_default();
                                let domain = url.split('/').nth(2).unwrap_or("");
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(serde_json::json!({
                                        "title": title,
                                        "url": url,
                                        "snippet": snippet,
                                        "domain": domain,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
        "duckduckgo" => {
            // DuckDuckGo only -- skip SearXNG entirely
        }
        _ => {
            // Default: SearXNG (self-hosted meta search, no API key needed)
            let searxng_base = search_base_url.clone()
                .or_else(|| std::env::var("ENGRAM_SEARXNG_URL").ok())
                .unwrap_or_else(|| "http://localhost:8090".to_string());
            let time_param = if !time_range.is_empty() {
                format!("&time_range={}", urlencoding::encode(&time_range))
            } else {
                String::new()
            };
            let search_url = format!(
                "{}/search?q={}&format=json&categories=general&engines=google,bing,duckduckgo{}",
                searxng_base,
                urlencoding::encode(&query),
                time_param
            );
            let resp = client
                .get(&search_url)
                .header("Accept", "application/json")
                .send()
                .await;

            if let Ok(resp) = resp {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(web_results) = data.get("results").and_then(|r| r.as_array()) {
                            for r in web_results.iter().take(50) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                let snippet = r.get("content").and_then(|d| d.as_str()).unwrap_or_default();
                                let domain = url.split('/').nth(2).unwrap_or("");
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(serde_json::json!({
                                        "title": title,
                                        "url": url,
                                        "snippet": snippet,
                                        "domain": domain,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: DuckDuckGo Instant Answer API (limited but no auth needed)
    if results.is_empty() {
        let ddg_url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(&query)
        );
        let resp = client
            .get(&ddg_url)
            .header("User-Agent", "engram-intel/1.0")
            .send()
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    // Abstract
                    if let (Some(heading), Some(abs)) = (
                        data.get("Heading").and_then(|h| h.as_str()),
                        data.get("Abstract").and_then(|a| a.as_str()),
                    ) {
                        if !abs.is_empty() {
                            let url = data.get("AbstractURL").and_then(|u| u.as_str()).unwrap_or_default();
                            let domain = url.split('/').nth(2).unwrap_or("");
                            results.push(serde_json::json!({
                                "title": heading,
                                "url": url,
                                "snippet": abs,
                                "domain": domain,
                            }));
                        }
                    }
                    // Related topics
                    if let Some(topics) = data.get("RelatedTopics").and_then(|r| r.as_array()) {
                        for t in topics.iter().take(8) {
                            let text = t.get("Text").and_then(|x| x.as_str()).unwrap_or_default();
                            let url = t.get("FirstURL").and_then(|u| u.as_str()).unwrap_or_default();
                            if !text.is_empty() {
                                let title = text.split(" - ").next().unwrap_or(text);
                                let domain = url.split('/').nth(2).unwrap_or("");
                                results.push(serde_json::json!({
                                    "title": title,
                                    "url": url,
                                    "snippet": text,
                                    "domain": domain,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
