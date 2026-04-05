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
use std::sync::RwLock as StdRwLock;
use tokio::sync::Mutex;

/// Cached pool of public SearxNG instances from searx.space
struct SearxPool {
    instances: Vec<String>,  // URLs sorted by quality
    fetched_at: std::time::Instant,
    unhealthy: std::collections::HashMap<String, std::time::Instant>,  // url -> unhealthy_since
}

static SEARX_POOL: LazyLock<StdRwLock<SearxPool>> = LazyLock::new(|| {
    StdRwLock::new(SearxPool {
        instances: Vec::new(),
        fetched_at: std::time::Instant::now(),
        unhealthy: std::collections::HashMap::new(),
    })
});

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

    let t0 = std::time::Instant::now();
    dbg_debate!("[websearch] >> provider={} query=\"{}\"", provider, query);
    let result = match provider.as_str() {
        "searxng" => search_searxng(client, query, search_url.as_deref(), "").await,
        "brave"   => search_brave(&client, query, &api_key).await,
        "duckduckgo" => search_duckduckgo(&client, query).await,
        other => {
            let msg = format!("unknown web_search_provider '{}' -- supported: searxng, brave, duckduckgo", other);
            eprintln!("[web_search] {}", msg);
            Err(msg)
        }
    };
    let count = result.as_ref().map(|r| r.len()).unwrap_or(0);
    dbg_debate!("[websearch] << {} results in {:.1}s", count, t0.elapsed().as_secs_f32());
    result
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

    let t0 = std::time::Instant::now();
    dbg_debate!("[websearch] >> provider={} query=\"{}\" time_range={}", provider, query, time_range);
    let result = match provider.as_str() {
        "searxng" => search_searxng(client, query, search_url.as_deref(), time_range).await,
        "brave"   => search_brave(&client, query, &api_key).await,
        "duckduckgo" => search_duckduckgo(&client, query).await,
        other => {
            let msg = format!("unknown web_search_provider '{}' -- supported: searxng, brave, duckduckgo", other);
            eprintln!("[web_search] {}", msg);
            Err(msg)
        }
    };
    let count = result.as_ref().map(|r| r.len()).unwrap_or(0);
    dbg_debate!("[websearch] << {} results in {:.1}s", count, t0.elapsed().as_secs_f32());
    result
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

// ── SearxNG fallback pool (searx.space) ────────────────────────────────

/// Fetch public SearxNG instances from searx.space, filter for healthy ones.
async fn refresh_searx_pool(client: &reqwest::Client) {
    let url = "https://searx.space/data/instances.json";
    let resp = match client.get(url).timeout(std::time::Duration::from_secs(10)).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => { dbg_debate!("[websearch] failed to fetch searx.space instance list"); return; }
    };
    let data: serde_json::Value = match resp.json().await {
        Ok(d) => d,
        _ => return,
    };
    let instances = data.get("instances").and_then(|i| i.as_object());
    let Some(instances) = instances else { return };

    let mut good: Vec<(String, f64)> = Vec::new();
    for (url, info) in instances {
        let status = info.pointer("/http/status_code").and_then(|s| s.as_u64()).unwrap_or(0);
        let grade = info.pointer("/http/grade").and_then(|g| g.as_str()).unwrap_or("");
        let timing = info.pointer("/timing/search/mean").and_then(|t| t.as_f64()).unwrap_or(99.0);
        // Only grade A+, A, or B with status 200
        if status == 200 && matches!(grade, "A+" | "A" | "B") {
            let search_url = if url.ends_with('/') { format!("{}search", url) } else { format!("{}/search", url) };
            good.push((search_url, timing));
        }
    }
    // Sort by response time (fastest first)
    good.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let urls: Vec<String> = good.into_iter().map(|(u, _)| u).collect();
    dbg_debate!("[websearch] refreshed searx.space pool: {} healthy instances", urls.len());

    if let Ok(mut pool) = SEARX_POOL.write() {
        pool.instances = urls;
        pool.fetched_at = std::time::Instant::now();
    }
}

/// Get a list of healthy fallback SearxNG URLs to try.
fn get_fallback_urls() -> Vec<String> {
    let pool = match SEARX_POOL.read() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let now = std::time::Instant::now();
    pool.instances.iter()
        .filter(|url| {
            // Skip if marked unhealthy within last 5 minutes
            pool.unhealthy.get(*url)
                .map(|since| now.duration_since(*since) > std::time::Duration::from_secs(300))
                .unwrap_or(true)
        })
        .take(5)  // Try at most 5 fallbacks
        .cloned()
        .collect()
}

/// Mark an instance as unhealthy (skip for 5 minutes).
fn mark_unhealthy(url: &str) {
    if let Ok(mut pool) = SEARX_POOL.write() {
        pool.unhealthy.insert(url.to_string(), std::time::Instant::now());
    }
}

/// Check if the pool needs refreshing (older than 1 hour).
fn pool_needs_refresh() -> bool {
    match SEARX_POOL.read() {
        Ok(pool) => pool.fetched_at.elapsed() > std::time::Duration::from_secs(3600) || pool.instances.is_empty(),
        Err(_) => true,
    }
}

// ── Provider implementations ────────────────────────────────────────────

async fn search_searxng(
    client: &reqwest::Client,
    query: &str,
    base_url: Option<&str>,
    time_range: &str,
) -> Result<Vec<WebSearchResult>, String> {
    let has_local = matches!(base_url, Some(url) if !url.is_empty());

    // Try local/configured instance first (with rate limiting)
    if has_local {
        let base = base_url.unwrap();
        rate_limit_searxng().await;
        match searxng_single_request(client, base, query, time_range).await {
            Ok(results) if !results.is_empty() => return Ok(results),
            Ok(_) => {
                dbg_debate!("[websearch] local searxng returned 0 results, trying fallbacks");
            }
            Err(e) => {
                dbg_debate!("[websearch] local searxng failed: {}, trying fallbacks", e);
            }
        }
    } else {
        dbg_debate!("[websearch] no local searxng configured, trying fallbacks from searx.space");
    }

    // If primary failed or returned nothing, try fallbacks from searx.space pool
    if pool_needs_refresh() {
        refresh_searx_pool(client).await;
    }
    let fallbacks = get_fallback_urls();
    if fallbacks.is_empty() {
        let msg = if has_local {
            "searxng: local instance returned no results and no fallbacks available".to_string()
        } else {
            "searxng configured but web_search_url is not set and no fallbacks available".to_string()
        };
        eprintln!("[web_search] {}", msg);
        return Err(msg);
    }

    for fallback_url in &fallbacks {
        dbg_debate!("[websearch] trying fallback: {}", fallback_url);
        // No rate limiting for public instances -- they have their own limits
        match searxng_single_request(client, fallback_url, query, time_range).await {
            Ok(results) if !results.is_empty() => {
                eprintln!("[web_search] searxng fallback: {} results from {}", results.len(), fallback_url);
                return Ok(results);
            }
            Ok(_) => {
                dbg_debate!("[websearch] fallback {} returned 0 results", fallback_url);
            }
            Err(e) => {
                dbg_debate!("[websearch] fallback {} failed: {}", fallback_url, e);
                mark_unhealthy(fallback_url);
            }
        }
    }

    let msg = format!("searxng: all {} fallbacks exhausted, no results", fallbacks.len());
    eprintln!("[web_search] {}", msg);
    Err(msg)
}

/// Execute a single SearxNG search request against one instance.
async fn searxng_single_request(
    client: &reqwest::Client,
    base: &str,
    query: &str,
    time_range: &str,
) -> Result<Vec<WebSearchResult>, String> {
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
        .timeout(std::time::Duration::from_secs(10))
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
        .timeout(std::time::Duration::from_secs(10))
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
        .timeout(std::time::Duration::from_secs(10))
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
