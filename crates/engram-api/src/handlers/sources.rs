/// Source CRUD API handlers.
///
/// Manages configured ingestion sources (RSS, web, file, folder, API, SPARQL).
/// Sources are persisted in `.brain.sources` alongside the brain file.

use axum::extract::State;
use axum::Json;
use axum::http::StatusCode;
use crate::state::AppState;
use crate::handlers::{ApiResult, api_err, read_lock_err};

// ── Types ──

#[derive(serde::Deserialize)]
pub struct SourceCreateRequest {
    pub name: String,
    pub source_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub refresh_interval: Option<u64>,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_secret_key: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct SourceUpdateRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub refresh_interval: Option<u64>,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_secret_key: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Persisted source configuration.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SourceConfig {
    pub name: String,
    pub source_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub refresh_interval: Option<u64>,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_secret_key: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_run: Option<i64>,
    #[serde(default)]
    pub total_ingested: u64,
    #[serde(default)]
    pub error_count: u32,
}

fn default_active() -> String { "active".into() }

#[derive(serde::Serialize)]
pub struct SourceResponse {
    pub name: String,
    pub source_type: String,
    pub url: Option<String>,
    pub refresh_interval: Option<u64>,
    pub auth_type: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub last_run: Option<i64>,
    pub total_ingested: u64,
    pub error_count: u32,
}

impl From<&SourceConfig> for SourceResponse {
    fn from(s: &SourceConfig) -> Self {
        Self {
            name: s.name.clone(),
            source_type: s.source_type.clone(),
            url: s.url.clone(),
            refresh_interval: s.refresh_interval,
            auth_type: s.auth_type.clone(),
            status: s.status.clone(),
            created_at: s.created_at,
            last_run: s.last_run,
            total_ingested: s.total_ingested,
            error_count: s.error_count,
        }
    }
}

#[derive(serde::Serialize)]
pub struct TestResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_found: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_ms: Option<u64>,
}

// ── Persistence ──

fn sources_path(state: &AppState) -> std::path::PathBuf {
    let g = state.graph.read().unwrap();
    let mut p = g.path().to_path_buf();
    p.set_extension("brain.sources");
    p
}

fn load_sources(state: &AppState) -> Vec<SourceConfig> {
    let path = sources_path(state);
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_sources(state: &AppState, sources: &[SourceConfig]) -> Result<(), String> {
    let path = sources_path(state);
    let json = serde_json::to_string_pretty(sources)
        .map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

// ── Handlers ──

/// GET /sources -- list all sources (persisted + runtime registry).
#[cfg(feature = "ingest")]
pub async fn list_sources(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let persisted = load_sources(&state);
    let runtime = state.source_registry.list_info();

    // Merge: persisted sources first, then runtime-only sources
    let mut sources: Vec<serde_json::Value> = persisted.iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "source_type": s.source_type,
            "url": s.url,
            "status": s.status,
            "total_ingested": s.total_ingested,
            "last_run": s.last_run,
            "error_count": s.error_count,
            "refresh_interval": s.refresh_interval,
        })
    }).collect();

    // Add runtime sources not in persisted list
    let persisted_names: std::collections::HashSet<String> = persisted.iter().map(|s| s.name.clone()).collect();
    for info in &runtime {
        if !persisted_names.contains(&info.name) {
            sources.push(serde_json::json!({
                "name": info.name,
                "source_type": "runtime",
                "status": if info.healthy { "active" } else { "error" },
                "total_ingested": info.usage.items,
                "error_count": info.usage.errors,
            }));
        }
    }

    Ok(Json(serde_json::json!({ "sources": sources })))
}

#[cfg(not(feature = "ingest"))]
pub async fn list_sources() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /sources -- create a new source.
#[cfg(feature = "ingest")]
pub async fn create_source(
    State(state): State<AppState>,
    Json(req): Json<SourceCreateRequest>,
) -> ApiResult<SourceResponse> {
    if req.name.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "source name is required"));
    }

    let valid_types = ["rss", "web", "paste", "file", "folder", "api", "sparql"];
    if !valid_types.contains(&req.source_type.as_str()) {
        return Err(api_err(StatusCode::BAD_REQUEST,
            format!("invalid source_type '{}', must be one of: {}", req.source_type, valid_types.join(", "))));
    }

    let mut sources = load_sources(&state);
    if sources.iter().any(|s| s.name == req.name) {
        return Err(api_err(StatusCode::CONFLICT, format!("source '{}' already exists", req.name)));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let config = SourceConfig {
        name: req.name.clone(),
        source_type: req.source_type.clone(),
        url: req.url.clone(),
        refresh_interval: req.refresh_interval,
        auth_type: req.auth_type.clone(),
        auth_secret_key: req.auth_secret_key.clone(),
        status: "active".into(),
        created_at: now,
        last_run: None,
        total_ingested: 0,
        error_count: 0,
    };

    sources.push(config.clone());
    save_sources(&state, &sources)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    tracing::info!(name = %req.name, source_type = %req.source_type, "source created");

    Ok(Json(SourceResponse::from(&config)))
}

#[cfg(not(feature = "ingest"))]
pub async fn create_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// GET /sources/{name} -- get a single source.
#[cfg(feature = "ingest")]
pub async fn get_source(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<SourceResponse> {
    let sources = load_sources(&state);
    let src = sources.iter().find(|s| s.name == name)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("source '{}' not found", name)))?;
    Ok(Json(SourceResponse::from(src)))
}

#[cfg(not(feature = "ingest"))]
pub async fn get_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// PUT /sources/{name} -- update a source.
#[cfg(feature = "ingest")]
pub async fn update_source(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<SourceUpdateRequest>,
) -> ApiResult<SourceResponse> {
    let mut sources = load_sources(&state);
    let src = sources.iter_mut().find(|s| s.name == name)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("source '{}' not found", name)))?;

    if let Some(url) = req.url { src.url = Some(url); }
    if let Some(interval) = req.refresh_interval { src.refresh_interval = Some(interval); }
    if let Some(auth_type) = req.auth_type { src.auth_type = Some(auth_type); }
    if let Some(auth_key) = req.auth_secret_key { src.auth_secret_key = Some(auth_key); }
    if let Some(status) = req.status {
        if !["active", "paused", "error"].contains(&status.as_str()) {
            return Err(api_err(StatusCode::BAD_REQUEST, "status must be active, paused, or error"));
        }
        src.status = status;
    }

    let response = SourceResponse::from(&*src);
    save_sources(&state, &sources)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(response))
}

#[cfg(not(feature = "ingest"))]
pub async fn update_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// DELETE /sources/{name} -- delete a source.
#[cfg(feature = "ingest")]
pub async fn delete_source(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sources = load_sources(&state);
    let before = sources.len();
    sources.retain(|s| s.name != name);
    if sources.len() == before {
        return Err(api_err(StatusCode::NOT_FOUND, format!("source '{}' not found", name)));
    }

    save_sources(&state, &sources)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Unregister from runtime registry too
    state.source_registry.unregister(&name);

    tracing::info!(name = %name, "source deleted");
    Ok(Json(serde_json::json!({ "deleted": name })))
}

#[cfg(not(feature = "ingest"))]
pub async fn delete_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /sources/{name}/run -- trigger immediate fetch/ingest for a source.
#[cfg(feature = "ingest")]
pub async fn run_source(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sources = load_sources(&state);
    let src = sources.iter_mut().find(|s| s.name == name)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("source '{}' not found", name)))?
        .clone();

    let result = run_source_impl(&state, &src).await;

    // Update stats in persisted config
    let mut sources = load_sources(&state);
    if let Some(s) = sources.iter_mut().find(|s| s.name == name) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        s.last_run = Some(now);
        match &result {
            Ok(stats) => {
                s.total_ingested += stats["facts_stored"].as_u64().unwrap_or(0);
                s.status = "active".into();
            }
            Err(_) => {
                s.error_count += 1;
                s.status = "error".into();
            }
        }
        let _ = save_sources(&state, &sources);
    }

    match result {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[cfg(not(feature = "ingest"))]
pub async fn run_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /sources/{name}/test -- test source connectivity without ingesting.
#[cfg(feature = "ingest")]
pub async fn test_source(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<TestResult> {
    let sources = load_sources(&state);
    let src = sources.iter().find(|s| s.name == name)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("source '{}' not found", name)))?;

    let result = test_source_impl(src).await;
    Ok(Json(result))
}

#[cfg(not(feature = "ingest"))]
pub async fn test_source() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(super::ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── Implementation helpers ──

#[cfg(feature = "ingest")]
async fn run_source_impl(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    match src.source_type.as_str() {
        "folder" => run_folder_source(state, src).await,
        "file" => run_file_source(state, src).await,
        "web" => run_web_source(state, src).await,
        "rss" => run_rss_source(state, src).await,
        "api" => run_api_source(state, src).await,
        "sparql" => Ok(serde_json::json!({ "message": "SPARQL sources are managed via /kb/add" })),
        "paste" => Ok(serde_json::json!({ "message": "paste sources have no URL to fetch" })),
        other => Err(format!("unsupported source type: {other}")),
    }
}

#[cfg(feature = "ingest")]
async fn run_folder_source(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    let folder = src.url.as_deref()
        .ok_or_else(|| "folder source requires a URL (folder path)".to_string())?;

    let folder_path = std::path::PathBuf::from(folder);
    if !folder_path.is_dir() {
        return Err(format!("folder not found: {folder}"));
    }

    let source_name = src.name.clone();
    let file_source = engram_ingest::FileSource::new(engram_ingest::FileSourceConfig {
        root: folder_path,
        recursive: true,
        name: source_name.clone(),
        ..Default::default()
    });

    let items = file_source.scan()
        .map_err(|e| format!("folder scan failed: {e}"))?;

    if items.is_empty() {
        return Ok(serde_json::json!({
            "message": "no files found",
            "facts_stored": 0,
            "relations_created": 0,
        }));
    }

    let item_count = items.len();
    run_ingest_items(state, items, &source_name).await
        .map(|mut v| {
            v["files_scanned"] = serde_json::json!(item_count);
            v
        })
}

#[cfg(feature = "ingest")]
async fn run_file_source(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    let file_path = src.url.as_deref()
        .ok_or_else(|| "file source requires a URL (file path)".to_string())?;

    let path = std::path::Path::new(file_path);
    if !path.is_file() {
        return Err(format!("file not found: {file_path}"));
    }

    let file_source = engram_ingest::FileSource::new(engram_ingest::FileSourceConfig {
        root: path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf(),
        extensions: vec![path.extension().and_then(|e| e.to_str()).unwrap_or("txt").to_string()],
        recursive: false,
        name: src.name.clone(),
    });

    let items = file_source.scan()
        .map_err(|e| format!("file read failed: {e}"))?;

    run_ingest_items(state, items, &src.name).await
}

#[cfg(feature = "ingest")]
async fn run_web_source(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    let url = src.url.as_deref()
        .ok_or_else(|| "web source requires a URL".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("engram/1.1")
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let resp = client.get(url).send().await
        .map_err(|e| format!("fetch failed: {e}"))?;

    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp.text().await
        .map_err(|e| format!("body read: {e}"))?;

    // Extract article text from HTML
    let text = if content_type.contains("html") {
        dom_smoothie::Readability::new(body.clone(), None, None)
            .ok()
            .and_then(|mut r| r.parse().ok())
            .map(|a| a.text_content.to_string())
            .unwrap_or(body)
    } else {
        body
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let items = vec![engram_ingest::RawItem {
        content: engram_ingest::Content::Text(text),
        source_url: Some(url.to_string()),
        source_name: src.name.clone(),
        fetched_at: now,
        metadata: Default::default(),
    }];

    run_ingest_items(state, items, &src.name).await
}

#[cfg(feature = "ingest")]
async fn run_rss_source(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    let url = src.url.as_deref()
        .ok_or_else(|| "RSS source requires a URL".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("engram/1.1")
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let body = client.get(url).send().await
        .map_err(|e| format!("fetch failed: {e}"))?
        .text().await
        .map_err(|e| format!("body read: {e}"))?;

    // Simple RSS item extraction (title + description)
    let mut items = Vec::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for item_block in body.split("<item>").skip(1) {
        let end = item_block.find("</item>").unwrap_or(item_block.len());
        let block = &item_block[..end];

        let title = extract_xml_tag(block, "title").unwrap_or_default();
        let description = extract_xml_tag(block, "description").unwrap_or_default();
        let link = extract_xml_tag(block, "link");

        let text = if description.is_empty() {
            title.clone()
        } else {
            format!("{title}. {description}")
        };

        if text.len() > 20 {
            items.push(engram_ingest::RawItem {
                content: engram_ingest::Content::Text(text),
                source_url: link,
                source_name: src.name.clone(),
                fetched_at: now,
                metadata: std::collections::HashMap::from([
                    ("title".into(), title),
                ]),
            });
        }
    }

    if items.is_empty() {
        return Ok(serde_json::json!({ "message": "no RSS items found", "facts_stored": 0 }));
    }

    let item_count = items.len();
    run_ingest_items(state, items, &src.name).await
        .map(|mut v| {
            v["rss_items"] = serde_json::json!(item_count);
            v
        })
}

#[cfg(feature = "ingest")]
async fn run_api_source(
    state: &AppState,
    src: &SourceConfig,
) -> Result<serde_json::Value, String> {
    let url = src.url.as_deref()
        .ok_or_else(|| "API source requires a URL".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let mut req = client.get(url);

    // Apply auth
    if let Some(ref auth_type) = src.auth_type {
        if let Some(ref key_name) = src.auth_secret_key {
            if let Ok(secrets) = state.secrets.read() {
                if let Some(ref store) = *secrets {
                    if let Some(secret_val) = store.get(key_name) {
                        match auth_type.as_str() {
                            "bearer" => { req = req.bearer_auth(secret_val); }
                            "api_key" => { req = req.header("X-API-Key", secret_val); }
                            "basic" => {
                                let parts: Vec<&str> = secret_val.splitn(2, ':').collect();
                                if parts.len() == 2 {
                                    req = req.basic_auth(parts[0], Some(parts[1]));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let body = req.send().await
        .map_err(|e| format!("fetch failed: {e}"))?
        .text().await
        .map_err(|e| format!("body read: {e}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let items = vec![engram_ingest::RawItem {
        content: engram_ingest::Content::Text(body),
        source_url: Some(url.to_string()),
        source_name: src.name.clone(),
        fetched_at: now,
        metadata: Default::default(),
    }];

    run_ingest_items(state, items, &src.name).await
}

/// Run ingest pipeline on a batch of items.
#[cfg(feature = "ingest")]
async fn run_ingest_items(
    state: &AppState,
    items: Vec<engram_ingest::RawItem>,
    source_name: &str,
) -> Result<serde_json::Value, String> {
    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold,
         coreference_enabled, llm_endpoint, llm_model) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled,
         c.llm_endpoint.clone(), c.llm_model.clone())
    };

    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();
    let _source_name = source_name.to_string();

    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = super::ingest::build_pipeline(
            graph, engram_ingest::PipelineConfig {
                name: _source_name,
                ..Default::default()
            },
            kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled,
            ner_cache, rel_cache,
        );
        pipeline.set_doc_store(doc_store);
        if let (Some(ep), Some(m)) = (llm_endpoint.as_ref(), llm_model.as_ref()) {
            pipeline.set_llm(ep.clone(), m.clone());
        }
        pipeline.execute(items)
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
    .map_err(|e| format!("pipeline: {e}"))?;

    state.mark_dirty();

    Ok(serde_json::json!({
        "facts_stored": result.facts_stored,
        "relations_created": result.relations_created,
        "duration_ms": result.duration_ms,
        "errors": result.errors,
    }))
}

/// Test source connectivity.
#[cfg(feature = "ingest")]
async fn test_source_impl(src: &SourceConfig) -> TestResult {
    let t0 = std::time::Instant::now();

    match src.source_type.as_str() {
        "folder" => {
            let path = match src.url.as_deref() {
                Some(p) => p,
                None => return TestResult {
                    ok: false, message: Some("no folder path configured".into()),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            let dir = std::path::Path::new(path);
            if !dir.is_dir() {
                return TestResult {
                    ok: false, message: Some(format!("directory not found: {path}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                };
            }
            let file_source = engram_ingest::FileSource::new(engram_ingest::FileSourceConfig {
                root: dir.to_path_buf(),
                recursive: true,
                name: src.name.clone(),
                ..Default::default()
            });
            let count = file_source.scan().map(|v| v.len()).unwrap_or(0);
            TestResult {
                ok: true,
                message: Some(format!("found {} ingestible files", count)),
                files_found: Some(count),
                content_type: None, size_bytes: None,
                response_ms: Some(t0.elapsed().as_millis() as u64),
            }
        }
        "file" => {
            let path = match src.url.as_deref() {
                Some(p) => p,
                None => return TestResult {
                    ok: false, message: Some("no file path configured".into()),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            let fp = std::path::Path::new(path);
            if !fp.is_file() {
                return TestResult {
                    ok: false, message: Some(format!("file not found: {path}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                };
            }
            let meta = std::fs::metadata(fp).ok();
            let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("?");
            TestResult {
                ok: true,
                message: Some(format!("file readable, extension: .{ext}")),
                files_found: None,
                content_type: Some(format!(".{ext}")),
                size_bytes: meta.map(|m| m.len()),
                response_ms: Some(t0.elapsed().as_millis() as u64),
            }
        }
        "web" | "rss" | "api" => {
            let url = match src.url.as_deref() {
                Some(u) => u,
                None => return TestResult {
                    ok: false, message: Some("no URL configured".into()),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("engram/1.1")
                .build();
            let client = match client {
                Ok(c) => c,
                Err(e) => return TestResult {
                    ok: false, message: Some(format!("HTTP client: {e}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            match client.head(url).send().await {
                Ok(resp) => {
                    let ct = resp.headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let cl = resp.headers()
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok());
                    TestResult {
                        ok: resp.status().is_success() || resp.status().as_u16() == 405,
                        message: Some(format!("HTTP {}", resp.status())),
                        files_found: None,
                        content_type: ct,
                        size_bytes: cl,
                        response_ms: Some(t0.elapsed().as_millis() as u64),
                    }
                }
                Err(e) => TestResult {
                    ok: false, message: Some(format!("connection failed: {e}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            }
        }
        "sparql" => {
            let url = match src.url.as_deref() {
                Some(u) => u,
                None => return TestResult {
                    ok: false, message: Some("no SPARQL endpoint configured".into()),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build();
            let client = match client {
                Ok(c) => c,
                Err(e) => return TestResult {
                    ok: false, message: Some(format!("HTTP client: {e}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            };
            let probe = format!("{}?query={}", url, urlencoding::encode("ASK { ?s ?p ?o }"));
            match client.get(&probe)
                .header("Accept", "application/sparql-results+json")
                .send().await
            {
                Ok(resp) => TestResult {
                    ok: resp.status().is_success(),
                    message: Some(format!("SPARQL HTTP {}", resp.status())),
                    files_found: None, content_type: None, size_bytes: None,
                    response_ms: Some(t0.elapsed().as_millis() as u64),
                },
                Err(e) => TestResult {
                    ok: false, message: Some(format!("SPARQL probe failed: {e}")),
                    files_found: None, content_type: None, size_bytes: None, response_ms: None,
                },
            }
        }
        _ => TestResult {
            ok: false, message: Some(format!("unsupported type: {}", src.source_type)),
            files_found: None, content_type: None, size_bytes: None, response_ms: None,
        },
    }
}

/// Extract text content from an XML tag. Simple non-recursive.
#[cfg(feature = "ingest")]
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let content_start = xml[start..].find('>')? + start + 1;
    let end = xml[content_start..].find(&close)? + content_start;
    let text = &xml[content_start..end];
    // Strip CDATA wrapper if present
    let text = text.trim();
    let text = text.strip_prefix("<![CDATA[").unwrap_or(text);
    let text = text.strip_suffix("]]>").unwrap_or(text);
    // Decode basic HTML entities
    let text = text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    Some(text.trim().to_string())
}

/// Background source polling task.
/// Spawns a tokio task that checks sources on their configured intervals.
#[cfg(feature = "ingest")]
pub fn spawn_source_poller(state: AppState) {
    tokio::spawn(async move {
        let mut scheduler = engram_ingest::AdaptiveScheduler::new(
            engram_ingest::SchedulerConfig::default(),
        );

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            let sources = load_sources(&state);
            for src in &sources {
                if src.status == "paused" {
                    continue;
                }

                // Register if new
                scheduler.register(&src.name);

                // Override interval from source config
                if let Some(interval) = src.refresh_interval {
                    scheduler.set_interval(&src.name, interval);
                }

                if !scheduler.is_due(&src.name) {
                    continue;
                }

                tracing::info!(source = %src.name, source_type = %src.source_type, "polling source");

                let result = run_source_impl(&state, src).await;
                let yield_count = match &result {
                    Ok(v) => v["facts_stored"].as_u64().unwrap_or(0) as u32,
                    Err(_) => 0,
                };
                scheduler.report_yield(&src.name, yield_count);

                // Update stats
                let mut all = load_sources(&state);
                if let Some(s) = all.iter_mut().find(|s| s.name == src.name) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    s.last_run = Some(now);
                    match result {
                        Ok(v) => {
                            s.total_ingested += v["facts_stored"].as_u64().unwrap_or(0);
                        }
                        Err(e) => {
                            tracing::warn!(source = %src.name, error = %e, "source poll failed");
                            s.error_count += 1;
                        }
                    }
                    let _ = save_sources(&state, &all);
                }
            }
        }
    });
}
