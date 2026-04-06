/// Unified web search abstraction with tiered fallback.
///
/// All web-search callers (debate research, proxy endpoint, etc.) go through
/// this module so provider logic, error handling, and logging live in one place.
///
/// Providers are tried in the user-defined order from `web_search_providers` config.
/// Each provider cascades on error, timeout, or zero results.
///
/// Supported providers: searxng, serper, google_cx, brave, duckduckgo

use crate::state::{AppState, WebSearchProviderConfig};
use std::sync::LazyLock;
use tokio::sync::Mutex;

/// A single web search result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebSearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

/// Minimum delay between consecutive SearxNG searches (protects upstream engines).
const SEARXNG_MIN_INTERVAL_MS: u64 = 1500;

/// Rate limiter: tracks when the last SearxNG request was made.
static LAST_SEARCH: LazyLock<Mutex<std::time::Instant>> =
    LazyLock::new(|| Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(10)));

/// Execute a web search using the tiered provider chain.
///
/// Tries each enabled provider in order. Cascades on error, timeout, or zero results.
/// Returns results from the first provider that succeeds.
pub async fn search(state: &AppState, query: &str) -> Result<Vec<WebSearchResult>, String> {
    search_inner(state, query, "", "en").await
}

/// Like [`search`] but also accepts `time_range` (for the proxy endpoint).
pub async fn search_with_time_range(
    state: &AppState,
    query: &str,
    time_range: &str,
) -> Result<Vec<WebSearchResult>, String> {
    search_inner(state, query, time_range, "en").await
}

/// Like [`search`] but searches in a specific language (ISO 639-1 code).
pub async fn search_with_language(
    state: &AppState,
    query: &str,
    language: &str,
) -> Result<Vec<WebSearchResult>, String> {
    search_inner(state, query, "", language).await
}

async fn search_inner(state: &AppState, query: &str, time_range: &str, language: &str) -> Result<Vec<WebSearchResult>, String> {
    let providers = resolve_providers(state);

    if providers.is_empty() {
        let msg = "web search not configured: no providers in web_search_providers";
        eprintln!("[web_search] {}", msg);
        return Err(msg.to_string());
    }

    let client = &state.http_client;
    let t0 = std::time::Instant::now();

    for (i, provider) in providers.iter().enumerate() {
        if !provider.enabled { continue; }

        let tier = i + 1;
        dbg_debate!("[websearch] tier {}: {} ({})", tier, provider.name, provider.provider);

        let api_key = resolve_api_key(state, provider);

        let lang = if language.is_empty() { "en" } else { language };
        let result = match provider.provider.as_str() {
            "searxng" => {
                rate_limit_searxng().await;
                search_searxng(client, query, provider.url.as_deref(), time_range, lang).await
            }
            "serper" => search_serper(client, query, &api_key, lang).await,
            "google_cx" => search_google_cx(client, query, &api_key, provider.cx_id.as_deref().unwrap_or(""), lang).await,
            "brave" => search_brave(client, query, &api_key).await,
            "duckduckgo" => search_duckduckgo(client, query).await,
            other => {
                eprintln!("[web_search] unknown provider '{}', skipping", other);
                continue;
            }
        };

        match result {
            Ok(results) if !results.is_empty() => {
                dbg_debate!("[websearch] tier {} ({}) returned {} results in {:.1}s",
                    tier, provider.name, results.len(), t0.elapsed().as_secs_f32());
                return Ok(results);
            }
            Ok(_) => {
                dbg_debate!("[websearch] tier {} ({}) returned 0 results, trying next", tier, provider.name);
            }
            Err(e) => {
                dbg_debate!("[websearch] tier {} ({}) failed: {}, trying next", tier, provider.name, e);
            }
        }
    }

    let msg = format!("all {} search providers exhausted, no results for \"{}\"",
        providers.len(), &query[..query.len().min(60)]);
    eprintln!("[web_search] {}", msg);
    Err(msg)
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Resolve the provider list from config, with fallback to old single-provider fields.
fn resolve_providers(state: &AppState) -> Vec<WebSearchProviderConfig> {
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());

    // New-style: ordered provider array
    if let Some(ref providers) = cfg.web_search_providers {
        if !providers.is_empty() {
            return providers.clone();
        }
    }

    // Fallback: old single-provider field (for configs that haven't been migrated yet)
    if let Some(ref provider) = cfg.web_search_provider {
        if !provider.is_empty() {
            return vec![WebSearchProviderConfig {
                name: provider.clone(),
                provider: provider.clone(),
                url: cfg.web_search_url.clone(),
                cx_id: None,
                enabled: true,
                auth_secret_key: None,
            }];
        }
    }

    Vec::new()
}

/// Resolve an API key from the secrets store, falling back to old web_search_api_key.
fn resolve_api_key(state: &AppState, provider: &WebSearchProviderConfig) -> String {
    // Try secrets store first
    if let Some(ref key_name) = provider.auth_secret_key {
        if let Ok(secrets) = state.secrets.read() {
            if let Some(ref store) = *secrets {
                if let Some(key) = store.get(key_name) {
                    return key.to_string();
                }
            }
        }
    }
    // Fallback: old config field (for brave with web_search_api_key)
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    cfg.web_search_api_key.clone().unwrap_or_default()
}

/// Enforce minimum delay between SearxNG requests.
async fn rate_limit_searxng() {
    let mut last = LAST_SEARCH.lock().await;
    let elapsed = last.elapsed();
    let min_interval = std::time::Duration::from_millis(SEARXNG_MIN_INTERVAL_MS);
    if elapsed < min_interval {
        let wait = min_interval - elapsed;
        tokio::time::sleep(wait).await;
    }
    *last = std::time::Instant::now();
}

// ── Provider implementations ────────────────────────────────────────────

async fn search_searxng(
    client: &reqwest::Client,
    query: &str,
    base_url: Option<&str>,
    time_range: &str,
    language: &str,
) -> Result<Vec<WebSearchResult>, String> {
    let base = match base_url {
        Some(url) if !url.is_empty() => url,
        _ => return Err("searxng: web_search_url not configured".into()),
    };

    let time_param = if !time_range.is_empty() {
        format!("&time_range={}", urlencoding::encode(time_range))
    } else {
        String::new()
    };

    let url = format!(
        "{}/search?q={}&format=json&language={}{}",
        base.trim_end_matches('/'),
        urlencoding::encode(query),
        language,
        time_param,
    );

    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("searxng request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("searxng HTTP {}: {}", status, &body[..body.len().min(200)]));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("searxng parse failed: {}", e))?;

    // Report unresponsive engines
    if let Some(unresponsive) = data.get("unresponsive_engines").and_then(|u| u.as_array()) {
        if !unresponsive.is_empty() {
            let engine_list: Vec<String> = unresponsive.iter()
                .filter_map(|e| {
                    let arr = e.as_array()?;
                    let name = arr.first()?.as_str()?;
                    let reason = arr.get(1).and_then(|r| r.as_str()).unwrap_or("unknown");
                    Some(format!("{}({})", name, reason))
                })
                .collect();
            eprintln!("[web_search] WARNING: unresponsive engines: {}", engine_list.join(", "));
        }
    }

    let mut results = Vec::new();
    if let Some(arr) = data.get("results").and_then(|r| r.as_array()) {
        for r in arr.iter().take(10) {
            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default().to_string();
            let snippet = r.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_string();
            let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            if !title.is_empty() {
                results.push(WebSearchResult { url, title, snippet });
            }
        }
    }

    eprintln!("[web_search] searxng: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    Ok(results)
}

async fn search_serper(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
    language: &str,
) -> Result<Vec<WebSearchResult>, String> {
    if api_key.is_empty() {
        return Err("serper: API key not configured (add to secrets store)".into());
    }

    let body = serde_json::json!({ "q": query, "gl": language, "hl": language });
    let resp = client.post("https://google.serper.dev/search")
        .timeout(std::time::Duration::from_secs(10))
        .header("X-API-KEY", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("serper request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("serper HTTP {}: {}", status, &body[..body.len().min(200)]));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("serper parse failed: {}", e))?;

    let mut results = Vec::new();
    if let Some(arr) = data.get("organic").and_then(|r| r.as_array()) {
        for r in arr.iter().take(10) {
            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default().to_string();
            let snippet = r.get("snippet").and_then(|s| s.as_str()).unwrap_or_default().to_string();
            let url = r.get("link").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            if !title.is_empty() {
                results.push(WebSearchResult { url, title, snippet });
            }
        }
    }

    eprintln!("[web_search] serper: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    Ok(results)
}

async fn search_google_cx(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
    cx_id: &str,
    language: &str,
) -> Result<Vec<WebSearchResult>, String> {
    if api_key.is_empty() {
        return Err("google_cx: API key not configured (add to secrets store)".into());
    }
    if cx_id.is_empty() {
        return Err("google_cx: cx_id not configured".into());
    }

    let url = format!(
        "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}&lr=lang_{}",
        urlencoding::encode(api_key),
        urlencoding::encode(cx_id),
        urlencoding::encode(query),
        language,
    );

    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("google_cx request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("google_cx HTTP {}: {}", status, &body[..body.len().min(200)]));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("google_cx parse failed: {}", e))?;

    let mut results = Vec::new();
    if let Some(arr) = data.get("items").and_then(|r| r.as_array()) {
        for r in arr.iter().take(10) {
            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default().to_string();
            let snippet = r.get("snippet").and_then(|s| s.as_str()).unwrap_or_default().to_string();
            let url = r.get("link").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            if !title.is_empty() {
                results.push(WebSearchResult { url, title, snippet });
            }
        }
    }

    eprintln!("[web_search] google_cx: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    Ok(results)
}

async fn search_brave(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
) -> Result<Vec<WebSearchResult>, String> {
    if api_key.is_empty() {
        return Err("brave: API key not configured".into());
    }

    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}",
        urlencoding::encode(query),
    );

    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|e| format!("brave request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("brave HTTP {}: {}", status, &body[..body.len().min(200)]));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("brave parse failed: {}", e))?;

    let mut results = Vec::new();
    if let Some(arr) = data.pointer("/web/results").and_then(|r| r.as_array()) {
        for r in arr.iter().take(10) {
            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default().to_string();
            let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string();
            let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            if !title.is_empty() {
                results.push(WebSearchResult { url, title, snippet });
            }
        }
    }

    eprintln!("[web_search] brave: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    Ok(results)
}

async fn search_duckduckgo(
    client: &reqwest::Client,
    query: &str,
) -> Result<Vec<WebSearchResult>, String> {
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        urlencoding::encode(query),
    );

    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .header("User-Agent", "engram/1.1")
        .send()
        .await
        .map_err(|e| format!("duckduckgo request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("duckduckgo HTTP {}: {}", status, &body[..body.len().min(200)]));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("duckduckgo parse failed: {}", e))?;

    let mut results = Vec::new();

    // Abstract (main result)
    if let (Some(heading), Some(abs)) = (
        data.get("Heading").and_then(|h| h.as_str()),
        data.get("Abstract").and_then(|a| a.as_str()),
    ) {
        if !abs.is_empty() {
            let abs_url = data.get("AbstractURL").and_then(|u| u.as_str()).unwrap_or_default();
            results.push(WebSearchResult {
                url: abs_url.to_string(),
                title: heading.to_string(),
                snippet: abs.to_string(),
            });
        }
    }

    // Related topics
    if let Some(topics) = data.get("RelatedTopics").and_then(|r| r.as_array()) {
        for t in topics.iter().take(8) {
            let text = t.get("Text").and_then(|x| x.as_str()).unwrap_or_default();
            let url = t.get("FirstURL").and_then(|u| u.as_str()).unwrap_or_default();
            if !text.is_empty() {
                let title = text.split(" - ").next().unwrap_or(text);
                results.push(WebSearchResult {
                    url: url.to_string(),
                    title: title.to_string(),
                    snippet: text.to_string(),
                });
            }
        }
    }

    eprintln!("[web_search] duckduckgo: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    Ok(results)
}
