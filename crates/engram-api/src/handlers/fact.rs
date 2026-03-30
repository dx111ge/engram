use super::*;

// ── Fact list types ──

#[derive(serde::Deserialize)]
pub struct FactListRequest {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search: Option<String>,
}

#[derive(serde::Serialize)]
pub struct FactItem {
    pub label: String,
    pub confidence: f32,
    pub status: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub claim: String,
    pub source_passage: String,
    pub chunk_index: String,
    pub extraction_method: String,
    pub event_date: String,
}

#[derive(serde::Serialize)]
pub struct FactListResponse {
    pub count: usize,
    pub total: usize,
    pub facts: Vec<FactItem>,
}

/// POST /facts -- list facts with server-side pagination.
/// Only builds FactItem for the requested page, counts total without allocating.
/// Source passages are truncated to 300 chars in list view.
pub async fn list_facts(
    State(state): State<AppState>,
    Json(req): Json<FactListRequest>,
) -> ApiResult<FactListResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(25);
    let offset = req.offset.unwrap_or(0);
    let filter_status = req.status.clone();
    let search_lower = req.search.as_ref().map(|s| s.to_lowercase());

    let all_nodes = g.all_nodes()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Two-pass: count matching + collect only the page we need
    let mut total = 0usize;
    let mut facts = Vec::with_capacity(limit);

    for node in &all_nodes {
        if node.node_type.as_deref() != Some("Fact") {
            continue;
        }
        let status = node.properties.get("status").map(|s| s.as_str()).unwrap_or("active");
        if let Some(ref filter) = filter_status {
            if status != filter.as_str() {
                continue;
            }
        }
        if let Some(ref query) = search_lower {
            let subject = node.properties.get("subject").map(|s| s.as_str()).unwrap_or("");
            let predicate = node.properties.get("predicate").map(|s| s.as_str()).unwrap_or("");
            let object = node.properties.get("object").map(|s| s.as_str()).unwrap_or("");
            let claim = node.properties.get("claim").map(|s| s.as_str()).unwrap_or("");
            let haystack = format!("{} {} {} {}", subject, predicate, object, claim).to_lowercase();
            if !haystack.contains(query.as_str()) {
                continue;
            }
        }

        // This node matches filters -- count it
        total += 1;

        // Only build FactItem if within the requested page
        if total > offset && facts.len() < limit {
            facts.push(FactItem {
                label: node.label.clone(),
                confidence: node.confidence,
                status: status.to_string(),
                subject: node.properties.get("subject").cloned().unwrap_or_default(),
                predicate: node.properties.get("predicate").cloned().unwrap_or_default(),
                object: node.properties.get("object").cloned().unwrap_or_default(),
                claim: node.properties.get("claim").cloned().unwrap_or_default(),
                source_passage: node.properties.get("source_passage")
                    .map(|s| if s.len() > 300 { format!("{}...", &s[..300]) } else { s.clone() })
                    .unwrap_or_default(),
                chunk_index: node.properties.get("chunk_index").cloned().unwrap_or_default(),
                extraction_method: node.properties.get("extraction_method").cloned().unwrap_or_default(),
                event_date: node.properties.get("event_date").cloned().unwrap_or_default(),
            });
        }
    }

    Ok(Json(FactListResponse {
        count: facts.len(),
        total,
        facts,
    }))
}

/// POST /facts/{label}/confirm -- confirm a fact, boost confidence + source trust
#[cfg(feature = "ingest")]
pub async fn fact_confirm(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let graph = state.graph.clone();
    let config = engram_ingest::LearnedTrustConfig::default();
    // Merge user overrides from config
    let trust_config = {
        let c = state.config.read().unwrap();
        let mut tc = config;
        if let Some(ref overrides) = c.source_trust_defaults {
            for (k, v) in overrides {
                tc.initial_trust_by_type.insert(k.clone(), *v);
            }
        }
        tc
    };
    let manager = engram_ingest::TrustManager::new(graph, trust_config);
    let (fact_conf, source_trust) = manager
        .confirm_fact(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state.mark_dirty();
    Ok(Json(serde_json::json!({
        "fact": label,
        "fact_confidence": fact_conf,
        "source_trust": source_trust,
        "status": "confirmed",
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn fact_confirm() -> impl axum::response::IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse {
            error: "ingest feature not enabled".into(),
        }),
    )
}

/// POST /facts/{label}/debunk -- debunk a fact, lower confidence + source trust
#[cfg(feature = "ingest")]
pub async fn fact_debunk(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let graph = state.graph.clone();
    let config = engram_ingest::LearnedTrustConfig::default();
    let trust_config = {
        let c = state.config.read().unwrap();
        let mut tc = config;
        if let Some(ref overrides) = c.source_trust_defaults {
            for (k, v) in overrides {
                tc.initial_trust_by_type.insert(k.clone(), *v);
            }
        }
        tc
    };
    let manager = engram_ingest::TrustManager::new(graph, trust_config);
    let (fact_conf, source_trust) = manager
        .debunk_fact(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state.mark_dirty();
    Ok(Json(serde_json::json!({
        "fact": label,
        "fact_confidence": fact_conf,
        "source_trust": source_trust,
        "status": "debunked",
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn fact_debunk() -> impl axum::response::IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse {
            error: "ingest feature not enabled".into(),
        }),
    )
}
