use super::*;

// ── Webhook receiver ─────────────────────────────────────────────────

/// Webhook receiver: accepts JSON payload and processes through ingest pipeline.
#[cfg(feature = "ingest")]
pub async fn webhook_receive(
    State(state): State<AppState>,
    Path(pipeline_id): Path<String>,
    body: String,
) -> ApiResult<serde_json::Value> {
    use engram_ingest::Pipeline;
    use engram_ingest::types::{PipelineConfig, RawItem, Content};

    let config = PipelineConfig::default();
    let graph = state.graph.clone();
    let doc_store = state.doc_store.clone();
    let mut pipeline = Pipeline::new(graph, config);
    pipeline.set_doc_store(doc_store);
    {
        let c = state.config.read().unwrap();
        if let (Some(ep), Some(m)) = (c.llm_endpoint.as_ref(), c.llm_model.as_ref()) {
            pipeline.set_llm(ep.clone(), m.clone());
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let items = vec![RawItem {
        content: Content::Text(body),
        source_url: None,
        source_name: pipeline_id.clone(),
        fetched_at: now,
        metadata: Default::default(),
    }];

    let results = pipeline.execute(items)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "pipeline": pipeline_id,
        "facts_stored": results.facts_stored,
        "relations_created": results.relations_created,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn webhook_receive() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── LLM suggestions for gaps ─────────────────────────────────────────

/// Generate LLM-powered investigation suggestions for a knowledge gap.
#[cfg(feature = "reason")]
pub async fn reason_suggest(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use engram_reason::llm_suggestions::{LlmSuggestionConfig, build_request, parse_suggestions, extract_content};

    let config = LlmSuggestionConfig::from_env();

    // Build a BlackArea from the request body
    let kind_str = body.get("kind").and_then(|v| v.as_str()).unwrap_or("frontier_node");
    let entities: Vec<String> = body.get("entities")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let severity = body.get("severity").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
    let domain = body.get("domain").and_then(|v| v.as_str()).map(String::from);

    let gap = engram_reason::BlackArea {
        kind: match kind_str {
            "structural_hole" => engram_reason::BlackAreaKind::StructuralHole,
            "asymmetric_cluster" => engram_reason::BlackAreaKind::AsymmetricCluster,
            "temporal_gap" => engram_reason::BlackAreaKind::TemporalGap,
            "confidence_desert" => engram_reason::BlackAreaKind::ConfidenceDesert,
            "coordinated_cluster" => engram_reason::BlackAreaKind::CoordinatedCluster,
            _ => engram_reason::BlackAreaKind::FrontierNode,
        },
        entities,
        severity,
        suggested_queries: vec![],
        domain,
        detected_at: 0,
    };

    let llm_body = build_request(&config, &gap);

    // Call the LLM endpoint
    let client = reqwest::Client::new();
    let mut req = client.post(&config.endpoint).json(&llm_body);
    if let Some(key) = &config.api_key {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let resp = req.send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("LLM request failed: {e}")))?;

    let resp_json: serde_json::Value = resp.json().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("LLM response parse failed: {e}")))?;

    let content = extract_content(&resp_json)
        .unwrap_or_default();
    let suggestions = parse_suggestions(&content, config.max_suggestions);

    let _ = state; // state not needed but keeps signature consistent

    Ok(Json(serde_json::json!({
        "suggestions": suggestions,
        "model": config.model,
        "gap_kind": kind_str,
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn reason_suggest() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

// ── Mesh discovery endpoints ─────────────────────────────────────────

/// GET /mesh/profiles -- list all known peer knowledge profiles.
#[cfg(feature = "reason")]
pub async fn mesh_profiles(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let config = engram_reason::ProfileConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let local_profile = engram_reason::derive_profile(&g, "local", &config, vec![], now);

    Ok(Json(serde_json::json!({
        "profiles": [local_profile],
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn mesh_profiles() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

/// GET /mesh/discover?topic=X -- find peers covering a topic.
#[cfg(feature = "reason")]
pub async fn mesh_discover(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let topic = query.get("topic").or_else(|| query.get("query"))
        .cloned()
        .unwrap_or_default();

    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let config = engram_reason::ProfileConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let local_profile = engram_reason::derive_profile(&g, "local", &config, vec![], now);
    let profiles = vec![local_profile];

    let matches = engram_reason::profiles::discover_by_topic(&profiles, &topic);
    let results: Vec<serde_json::Value> = matches.iter().map(|(idx, domain)| {
        serde_json::json!({
            "peer": profiles[*idx].name,
            "topic": domain.topic,
            "fact_count": domain.fact_count,
            "avg_confidence": domain.avg_confidence,
            "depth": domain.depth,
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "query": topic,
        "matches": results,
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn mesh_discover() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

/// POST /mesh/query -- execute a federated query.
#[cfg(feature = "reason")]
pub async fn mesh_federated_query(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let query_text = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let max_results = body.get("max_results").and_then(|v| v.as_u64()).unwrap_or(50) as u32;
    let min_confidence = body.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let clearance = body.get("sensitivity_clearance").and_then(|v| v.as_str()).unwrap_or("public");

    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let fq = engram_reason::FederatedQuery {
        query: query_text.to_string(),
        query_type: "hybrid".to_string(),
        max_results,
        min_confidence,
        requesting_node: "self".to_string(),
        sensitivity_clearance: clearance.to_string(),
    };

    let mut result = engram_reason::federated::execute_local(&g, &fq);
    result.peer_id = "local".to_string();

    Ok(Json(serde_json::json!(result)))
}

#[cfg(not(feature = "reason"))]
pub async fn mesh_federated_query() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

// ── Batch job streaming (SSE) ────────────────────────────────────────

/// GET /batch/jobs/{id}/stream -- SSE progress stream for a batch ingest job.
pub async fn batch_job_stream(
    Path(_job_id): Path<String>,
) -> impl axum::response::IntoResponse {
    // Batch job tracking is not yet implemented -- return a stub SSE stream
    use axum::response::sse::{Event, Sse};
    use tokio_stream::StreamExt;

    let stream = tokio_stream::once(
        Event::default()
            .event("complete")
            .data(r#"{"status":"no active job"}"#)
    );

    Sse::new(stream.map(Ok::<_, std::convert::Infallible>))
}

// ── WebSocket ingest endpoint ────────────────────────────────────────

/// WebSocket ingest: accepts streaming NDJSON over WebSocket for real-time ingestion.
#[cfg(feature = "ingest")]
pub async fn ws_ingest(
    State(state): State<AppState>,
    Path(pipeline_id): Path<String>,
    ws: axum::extract::WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_ingest(socket, state, pipeline_id))
}

#[cfg(feature = "ingest")]
async fn handle_ws_ingest(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    pipeline_id: String,
) {
    use axum::extract::ws::Message;
    use engram_core::graph::Provenance;

    let mut count = 0u64;
    let mut errors = 0u64;

    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                // Each message is a JSON entity to ingest
                let result = match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(val) => {
                        let entity = val.get("entity")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&text);
                        let source = val.get("source")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&pipeline_id);
                        let prov = Provenance::user(source);
                        let mut g = state.graph.write().unwrap();
                        match g.store(entity, &prov) {
                            Ok(slot) => {
                                count += 1;
                                state.mark_dirty();
                                serde_json::json!({"ok": true, "slot": slot, "count": count})
                            }
                            Err(e) => {
                                errors += 1;
                                serde_json::json!({"ok": false, "error": e.to_string()})
                            }
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        serde_json::json!({"ok": false, "error": e.to_string()})
                    }
                };
                let _ = socket.send(Message::Text(serde_json::to_string(&result).unwrap_or_default().into())).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Send final summary before closing
    let summary = serde_json::json!({
        "pipeline": pipeline_id,
        "ingested": count,
        "errors": errors,
        "status": "closed"
    });
    let _ = socket.send(Message::Text(serde_json::to_string(&summary).unwrap_or_default().into())).await;
}

#[cfg(not(feature = "ingest"))]
pub async fn ws_ingest(
    _ws: axum::extract::WebSocketUpgrade,
    _path: Path<String>,
) -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── SSE enrichment streaming ─────────────────────────────────────────

/// SSE streaming for enrichment: streams enrichment events as they happen.
/// Use `?enrich=await` on query endpoints to get streaming enrichment results.
#[cfg(feature = "reason")]
pub async fn enrich_stream(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::Event;
    use tokio_stream::StreamExt;

    let search_query = query.get("q").cloned().unwrap_or_default();
    let g = state.graph.read().unwrap();

    // Step 1: local search
    let local_results = g.search(&search_query, 20).unwrap_or_default();
    let local_facts: Vec<serde_json::Value> = local_results.iter().map(|r| {
        serde_json::json!({
            "label": r.label,
            "confidence": r.confidence,
            "score": r.score,
            "source": "local"
        })
    }).collect();
    drop(g);

    // Build SSE stream with enrichment phases
    let events = vec![
        Event::default()
            .event("enrichment_start")
            .data(serde_json::json!({"query": search_query, "phase": "local"}).to_string()),
        Event::default()
            .event("local_results")
            .data(serde_json::to_string(&local_facts).unwrap_or_default()),
        Event::default()
            .event("enrichment_phase")
            .data(serde_json::json!({"phase": "mesh", "status": "checking"}).to_string()),
        Event::default()
            .event("enrichment_phase")
            .data(serde_json::json!({"phase": "external", "status": "skipped"}).to_string()),
        Event::default()
            .event("enrichment_complete")
            .data(serde_json::json!({"total_results": local_facts.len(), "enriched": false}).to_string()),
    ];

    let stream = tokio_stream::iter(events).map(Ok::<_, std::convert::Infallible>);
    axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}

#[cfg(not(feature = "reason"))]
pub async fn enrich_stream(
    _state: State<AppState>,
    _query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse { error: "reason feature not enabled".into() }))
}
