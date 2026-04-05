/// Unified web search abstraction.
///
/// All web-search callers (debate research, proxy endpoint, etc.) go through
/// this module so provider logic, error handling, and logging live in one place.
///
/// Features:
/// - Explicit provider matching (searxng, brave, duckduckgo)
/// - Rate limiting (min 1.5s between SearxNG calls to protect upstream engines)
/// - Unresponsive engine reporting (SearxNG tells us which engines are blocked)
/// - Clear error messages on misconfiguration

use crate::state::AppState;
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

/// Execute a structured web search using the configured provider.
///
/// Returns `Ok(results)` on success (possibly empty if the query matched nothing),
/// or `Err(message)` when the provider is misconfigured or unreachable.
pub async fn search(state: &AppState, query: &str) -> Result<Vec<WebSearchResult>, String> {
    let (provider, api_key, search_url) = read_config(state);

    if provider.is_empty() {
        let msg = "web search not configured: web_search_provider is not set";
        eprintln!("[web_search] {}", msg);
        return Err(msg.to_string());
    }

    let client = &state.http_client;

    match provider.as_str() {
        "searxng" => search_searxng(client, query, search_url.as_deref(), "").await,
        "brave"   => search_brave(&client, query, &api_key).await,
        "duckduckgo" => search_duckduckgo(&client, query).await,
        other => {
            let msg = format!("unknown web_search_provider '{}' -- supported: searxng, brave, duckduckgo", other);
            eprintln!("[web_search] {}", msg);
            Err(msg)
        }
    }
}

/// Like [`search`] but also accepts `time_range` (for the proxy endpoint).
pub async fn search_with_time_range(
    state: &AppState,
    query: &str,
    time_range: &str,
) -> Result<Vec<WebSearchResult>, String> {
    let (provider, api_key, search_url) = read_config(state);

    if provider.is_empty() {
        let msg = "web search not configured: web_search_provider is not set";
        eprintln!("[web_search] {}", msg);
        return Err(msg.to_string());
    }

    let client = &state.http_client;

    match provider.as_str() {
        "searxng" => search_searxng(client, query, search_url.as_deref(), time_range).await,
        "brave"   => search_brave(&client, query, &api_key).await,
        "duckduckgo" => search_duckduckgo(&client, query).await,
        other => {
            let msg = format!("unknown web_search_provider '{}' -- supported: searxng, brave, duckduckgo", other);
            eprintln!("[web_search] {}", msg);
            Err(msg)
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_config(state: &AppState) -> (String, String, Option<String>) {
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    (
        cfg.web_search_provider.clone().unwrap_or_default(),
        cfg.web_search_api_key.clone().unwrap_or_default(),
        cfg.web_search_url.clone(),
    )
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
) -> Result<Vec<WebSearchResult>, String> {
    let base = match base_url {
        Some(url) if !url.is_empty() => url,
        _ => {
            let msg = "searxng configured but web_search_url is not set";
            eprintln!("[web_search] {}", msg);
            return Err(msg.to_string());
        }
    };

    // Rate limit to protect upstream engines
    rate_limit_searxng().await;

    let time_param = if !time_range.is_empty() {
        format!("&time_range={}", urlencoding::encode(time_range))
    } else {
        String::new()
    };

    let url = format!(
        "{}/search?q={}&format=json&language=en{}",
        base.trim_end_matches('/'),
        urlencoding::encode(query),
        time_param,
    );

    let resp = client.get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            let msg = format!("searxng request to {} failed: {}", base, e);
            eprintln!("[web_search] {}", msg);
            msg
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format!("searxng returned HTTP {}: {}", status, &body[..body.len().min(200)]);
        eprintln!("[web_search] {}", msg);
        return Err(msg);
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        let msg = format!("searxng response parse failed: {}", e);
        eprintln!("[web_search] {}", msg);
        msg
    })?;

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

    if results.is_empty() {
        // Log which engines responded (helps diagnose rate limiting)
        let num_results = data.get("number_of_results").and_then(|n| n.as_u64()).unwrap_or(0);
        eprintln!("[web_search] searxng: 0 results (number_of_results={}) for \"{}\"",
            num_results, &query[..query.len().min(60)]);
    } else {
        eprintln!("[web_search] searxng: {} results for \"{}\"", results.len(), &query[..query.len().min(60)]);
    }

    Ok(results)
}

async fn search_brave(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
) -> Result<Vec<WebSearchResult>, String> {
    if api_key.is_empty() {
        let msg = "brave configured but web_search_api_key is not set";
        eprintln!("[web_search] {}", msg);
        return Err(msg.to_string());
    }

    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}",
        urlencoding::encode(query),
    );

    let resp = client.get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|e| {
            let msg = format!("brave search request failed: {}", e);
            eprintln!("[web_search] {}", msg);
            msg
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format!("brave returned HTTP {}: {}", status, &body[..body.len().min(200)]);
        eprintln!("[web_search] {}", msg);
        return Err(msg);
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        let msg = format!("brave response parse failed: {}", e);
        eprintln!("[web_search] {}", msg);
        msg
    })?;

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
        .header("User-Agent", "engram/1.1")
        .send()
        .await
        .map_err(|e| {
            let msg = format!("duckduckgo request failed: {}", e);
            eprintln!("[web_search] {}", msg);
            msg
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format!("duckduckgo returned HTTP {}: {}", status, &body[..body.len().min(200)]);
        eprintln!("[web_search] {}", msg);
        return Err(msg);
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        let msg = format!("duckduckgo response parse failed: {}", e);
        eprintln!("[web_search] {}", msg);
        msg
    })?;

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
