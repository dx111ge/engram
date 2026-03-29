use super::*;

// ── Fact list types ──

#[derive(serde::Deserialize)]
pub struct FactListRequest {
    pub status: Option<String>,
    pub limit: Option<usize>,
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
    pub facts: Vec<FactItem>,
}

/// POST /facts -- list facts, optionally filtered by status.
pub async fn list_facts(
    State(state): State<AppState>,
    Json(req): Json<FactListRequest>,
) -> ApiResult<FactListResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(50);
    let filter_status = req.status.clone();

    let all_nodes = g.all_nodes()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut facts = Vec::new();
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
        facts.push(FactItem {
            label: node.label.clone(),
            confidence: node.confidence,
            status: status.to_string(),
            subject: node.properties.get("subject").cloned().unwrap_or_default(),
            predicate: node.properties.get("predicate").cloned().unwrap_or_default(),
            object: node.properties.get("object").cloned().unwrap_or_default(),
            claim: node.properties.get("claim").cloned().unwrap_or_default(),
            source_passage: node.properties.get("source_passage").cloned().unwrap_or_default(),
            chunk_index: node.properties.get("chunk_index").cloned().unwrap_or_default(),
            extraction_method: node.properties.get("extraction_method").cloned().unwrap_or_default(),
            event_date: node.properties.get("event_date").cloned().unwrap_or_default(),
        });
        if facts.len() >= limit {
            break;
        }
    }

    Ok(Json(FactListResponse {
        count: facts.len(),
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
