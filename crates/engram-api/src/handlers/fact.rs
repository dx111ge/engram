use super::*;

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
