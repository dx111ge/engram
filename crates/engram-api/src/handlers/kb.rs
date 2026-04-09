use super::*;

// ── GET /config/relation-templates/export -- Export configured + learned relation templates ──

pub async fn export_relation_templates(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let cfg = state.config.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
    })?;

    // Start with configured templates (or defaults)
    let configured: std::collections::HashMap<String, String> = cfg.relation_templates.clone()
        .unwrap_or_else(|| std::collections::HashMap::from([
                ("works_at".to_string(), "{head} works at {tail}".to_string()),
                ("headquartered_in".to_string(), "{head} is headquartered in {tail}".to_string()),
                ("located_in".to_string(), "{head} is located in {tail}".to_string()),
                ("founded".to_string(), "{head} founded {tail}".to_string()),
                ("leads".to_string(), "{head} leads {tail}".to_string()),
                ("supports".to_string(), "{head} supports {tail}".to_string()),
            ]));
    let threshold = cfg.rel_threshold.unwrap_or(0.9);
    drop(cfg);

    // Collect learned relation types from the graph's relation gazetteer sidecar
    let mut learned_types: Vec<String> = Vec::new();
    #[cfg(feature = "ingest")]
    if let Some(ref config_path) = state.config_path {
        // Derive brain path from config path (config is .brain.config, brain is .brain)
        let brain_path = config_path.with_extension("");
        let relgaz_path = brain_path.with_extension("relgaz");
        if relgaz_path.exists() {
            if let Ok(gaz) = engram_ingest::RelationGazetteer::load(&brain_path) {
                for rt in gaz.known_relation_types() {
                    if !configured.contains_key::<str>(&rt) {
                        learned_types.push(rt.to_string());
                    }
                }
                learned_types.sort();
            }
        }
    }

    Ok(Json(serde_json::json!({
        "templates": configured,
        "threshold": threshold,
        "learned_relation_types": learned_types,
    })))
}

// ── POST /config/relation-templates/import -- Import relation templates ──

pub async fn import_relation_templates(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let templates = body.get("templates")
        .and_then(|v| serde_json::from_value::<std::collections::HashMap<String, String>>(v.clone()).ok())
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "\"templates\" field required: {\"rel_type\": \"{head} verb {tail}\", ...}"))?;

    // Validate templates contain {head} and {tail}
    for (rel_type, template) in &templates {
        if !template.contains("{head}") || !template.contains("{tail}") {
            return Err(api_err(StatusCode::BAD_REQUEST,
                format!("template '{}' must contain {{head}} and {{tail}} placeholders", rel_type)));
        }
    }

    let threshold = body.get("threshold")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);

    // Merge into config
    {
        let mut cfg = state.config.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
        })?;
        // Merge: existing templates + imported (imported wins on conflict)
        let mut merged = cfg.relation_templates.clone()
            .unwrap_or_else(|| std::collections::HashMap::from([
                ("works_at".to_string(), "{head} works at {tail}".to_string()),
                ("headquartered_in".to_string(), "{head} is headquartered in {tail}".to_string()),
                ("located_in".to_string(), "{head} is located in {tail}".to_string()),
                ("founded".to_string(), "{head} founded {tail}".to_string()),
                ("leads".to_string(), "{head} leads {tail}".to_string()),
                ("supports".to_string(), "{head} supports {tail}".to_string()),
            ]));
        merged.extend(templates.clone());
        cfg.relation_templates = Some(merged.clone());
        if let Some(t) = threshold {
            cfg.rel_threshold = Some(t);
        }
    }
    state.save_config().ok();

    // Invalidate cached rel backend so next ingest picks up new templates
    #[cfg(feature = "ingest")]
    {
        if let Ok(mut cached) = state.cached_rel.write() {
            *cached = None;
        }
    }

    let cfg = state.config.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
    })?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "templates_count": cfg.relation_templates.as_ref().map(|t| t.len()).unwrap_or(0),
        "threshold": cfg.rel_threshold.unwrap_or(0.9),
        "imported": templates.len(),
    })))
}

// ── GET /config/kb ──

pub async fn list_kb_endpoints(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let cfg = state.config.read().map_err(|_| read_lock_err())?;
    let endpoints = cfg.kb_endpoints.clone().unwrap_or_default();
    Ok(Json(serde_json::json!({ "endpoints": endpoints })))
}

// ── POST /config/kb ──

pub async fn add_kb_endpoint(
    State(state): State<AppState>,
    Json(body): Json<KbEndpointRequest>,
) -> ApiResult<serde_json::Value> {
    use crate::state::KbEndpointConfig;

    let kb = KbEndpointConfig {
        name: body.name.clone(),
        url: body.url,
        auth_type: body.auth_type.unwrap_or_else(|| "none".to_string()),
        auth_secret_key: body.auth_secret_key,
        enabled: true,
        entity_link_template: body.entity_link_template,
        relation_query_template: body.relation_query_template,
        max_lookups_per_call: body.max_lookups_per_call,
    };

    {
        let mut cfg = state.config.write().map_err(|_| write_lock_err())?;
        let endpoints = cfg.kb_endpoints.get_or_insert_with(Vec::new);
        // Remove existing with same name
        endpoints.retain(|e| e.name != kb.name);
        endpoints.push(kb);
    }

    state.save_config().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "status": "added", "name": body.name })))
}

// ── DELETE /config/kb/{name} ──

pub async fn delete_kb_endpoint(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut found = false;
    {
        let mut cfg = state.config.write().map_err(|_| write_lock_err())?;
        if let Some(ref mut endpoints) = cfg.kb_endpoints {
            let before = endpoints.len();
            endpoints.retain(|e| e.name != name);
            found = endpoints.len() < before;
        }
    }

    if !found {
        return Err(api_err(StatusCode::NOT_FOUND, format!("kb endpoint '{}' not found", name)));
    }

    state.save_config().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "status": "deleted", "name": name })))
}

// ── POST /config/kb/{name}/test ──

pub async fn test_kb_endpoint(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<KbTestResponse> {
    let url = {
        let cfg = state.config.read().map_err(|_| read_lock_err())?;
        let endpoints = cfg.kb_endpoints.as_ref().ok_or_else(|| {
            api_err(StatusCode::NOT_FOUND, "no kb endpoints configured")
        })?;
        let ep = endpoints.iter().find(|e| e.name == name).ok_or_else(|| {
            api_err(StatusCode::NOT_FOUND, format!("kb endpoint '{}' not found", name))
        })?;
        ep.url.clone()
    };

    let start = std::time::Instant::now();
    let client = &state.http_client;

    match client.get(&url).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            if resp.status().is_success() || resp.status().as_u16() == 400 {
                // SPARQL endpoints may return 400 for empty query but are reachable
                Ok(Json(KbTestResponse {
                    success: true,
                    latency_ms: Some(latency),
                    error: None,
                }))
            } else {
                Ok(Json(KbTestResponse {
                    success: false,
                    latency_ms: Some(latency),
                    error: Some(format!("HTTP {}", resp.status())),
                }))
            }
        }
        Err(e) => {
            Ok(Json(KbTestResponse {
                success: false,
                latency_ms: None,
                error: Some(e.to_string()),
            }))
        }
    }
}

// ── Action engine endpoints ──────────────────────────────────────────

#[cfg(feature = "actions")]
pub async fn load_action_rules(
    State(state): State<AppState>,
    body: String,
) -> ApiResult<serde_json::Value> {
    let rules = engram_action::parse_rules(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })))?;
    let count = rules.len();
    let mut engine = state.action_engine.write().map_err(|_| write_lock_err())?;
    engine.load_rules(rules);
    drop(engine);
    state.save_action_rules();
    Ok(Json(serde_json::json!({ "loaded": count })))
}

#[cfg(not(feature = "actions"))]
pub async fn load_action_rules() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "actions feature not enabled".into() }))
}

#[cfg(feature = "actions")]
pub async fn list_action_rules(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let engine = state.action_engine.read().map_err(|_| read_lock_err())?;
    let rules = engine.list_rules_full();
    Ok(Json(serde_json::to_value(rules).unwrap_or_default()))
}

#[cfg(not(feature = "actions"))]
pub async fn list_action_rules() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "actions feature not enabled".into() }))
}

#[cfg(feature = "actions")]
pub async fn get_action_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = state.action_engine.read().map_err(|_| read_lock_err())?;
    match engine.get_rule(&id) {
        Some(rule) => Ok(Json(serde_json::json!(rule))),
        None => Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("rule '{}' not found", id) }))),
    }
}

#[cfg(not(feature = "actions"))]
pub async fn get_action_rule() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "actions feature not enabled".into() }))
}

#[cfg(feature = "actions")]
pub async fn delete_action_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut engine = state.action_engine.write().map_err(|_| write_lock_err())?;
    let removed = engine.remove_rule(&id);
    drop(engine);
    if removed {
        state.save_action_rules();
        Ok(Json(serde_json::json!({ "removed": id })))
    } else {
        Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("rule '{}' not found", id) })))
    }
}

#[cfg(not(feature = "actions"))]
pub async fn delete_action_rule() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "actions feature not enabled".into() }))
}

#[cfg(feature = "actions")]
pub async fn dry_run_action(
    State(state): State<AppState>,
    Json(event_json): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    // Build a synthetic FactStored event from the JSON
    let label = event_json.get("label").and_then(|v| v.as_str()).unwrap_or("unknown");
    let confidence = event_json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
    let entity_type = event_json.get("entity_type").and_then(|v| v.as_str());

    let event = engram_core::events::GraphEvent::FactStored {
        node_id: 0,
        label: std::sync::Arc::from(label),
        confidence,
        source: std::sync::Arc::from("dry-run"),
        entity_type: entity_type.map(|s| std::sync::Arc::from(s)),
    };

    let engine = state.action_engine.read().map_err(|_| read_lock_err())?;
    let results = engine.dry_run(&event);
    Ok(Json(serde_json::json!({ "results": results })))
}

#[cfg(not(feature = "actions"))]
pub async fn dry_run_action() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "actions feature not enabled".into() }))
}

// ── Reason / gap detection endpoints ─────────────────────────────────

#[cfg(feature = "reason")]
pub async fn reason_gaps(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let min_severity: f32 = query.get("min_severity")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let graph = state.graph.read().map_err(|_| read_lock_err())?;
    let config = engram_reason::DetectionConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let (gaps, report) = engram_reason::scan(&graph, &config, now)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })))?;

    let filtered: Vec<_> = gaps.into_iter()
        .filter(|g| g.severity >= min_severity)
        .collect();

    Ok(Json(serde_json::json!({
        "gaps": filtered,
        "report": report,
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn reason_gaps() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

#[cfg(feature = "reason")]
pub async fn reason_scan(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let graph = state.graph.read().map_err(|_| read_lock_err())?;
    let config = engram_reason::DetectionConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let (gaps, report) = engram_reason::scan(&graph, &config, now)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })))?;

    Ok(Json(serde_json::json!({
        "gaps": gaps,
        "report": report,
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn reason_scan() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

#[cfg(feature = "reason")]
pub async fn reason_frontier(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let graph = state.graph.read().map_err(|_| read_lock_err())?;
    let nodes = graph.all_nodes()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })))?;
    let config = engram_reason::DetectionConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    let mut gaps = engram_reason::frontier::detect_frontier_nodes(&nodes, &config, now);
    gaps.extend(engram_reason::frontier::detect_isolated_nodes(&nodes, now));
    engram_reason::scoring::rank_gaps(&mut gaps);

    Ok(Json(serde_json::json!({ "frontier": gaps })))
}

#[cfg(not(feature = "reason"))]
pub async fn reason_frontier() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

// ── POST /reason/enrich/plan -- Generate search queries for gap enrichment ──

pub async fn enrich_plan(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use super::debate::llm::{call_llm, extract_content, research_context};

    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let entities = req.get("entities").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>())
        .unwrap_or_default();

    if query.is_empty() && entities.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "provide query or entities"));
    }

    // Gather graph context about the entities
    let graph_context = {
        let g = state.graph.read().unwrap();
        let mut ctx = String::new();
        for ent in entities.iter().take(5) {
            if let Ok(edges) = g.edges_from(ent) {
                for e in edges.iter().take(5) {
                    ctx.push_str(&format!("  {} --[{}]--> {} ({:.2})\n", ent, e.relationship, e.to, e.confidence));
                }
            }
        }
        ctx
    };

    let sys_prompt = research_context(&state);
    let topic = if !query.is_empty() { query.clone() } else { entities.join(", ") };

    let prompt = format!(
        "{}Generate 3-5 precise web search queries to fill knowledge gaps about: \"{}\"\n\n\
         {}\
         Requirements:\n\
         - Queries must work on Google/search engines (short, specific, no LLM prose)\n\
         - Focus on what's MISSING, not what we already know\n\
         - Include quantitative/factual queries (numbers, dates, volumes)\n\
         - Each query on its own line, no numbering\n\n\
         Return ONLY the search queries, one per line.",
        if sys_prompt.is_empty() { String::new() } else { format!("Research context: {}\n\n", sys_prompt) },
        topic,
        if graph_context.is_empty() { String::new() } else { format!("Known facts:\n{}\n\n", graph_context) },
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a research analyst generating search queries to fill intelligence gaps."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 512,
        "think": false
    });

    match call_llm(&state, request).await {
        Ok(resp) => {
            let content = extract_content(&resp).unwrap_or_default();
            let queries: Vec<String> = content.lines()
                .map(|l| l.trim().trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '*').trim().to_string())
                .filter(|l| !l.is_empty() && l.len() > 5 && l.len() < 200)
                .take(5)
                .collect();

            Ok(Json(serde_json::json!({ "queries": queries })))
        }
        Err(e) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("LLM error: {}", e))),
    }
}

// ── POST /reason/enrich/run -- Execute background enrichment with given queries ──

pub async fn enrich_run(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let queries: Vec<String> = req.get("queries").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    if queries.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "no queries provided"));
    }

    let max_sources = req.get("max_sources").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let query_count = queries.len();

    // Run enrichment in background
    let bg_state = state.clone();
    tokio::spawn(async move {
        use engram_core::events::GraphEvent;

        let _ = bg_state.event_bus.publish(GraphEvent::IngestProgress {
            operation: "enrichment".into(),
            document: "".into(),
            processed: 0,
            total: queries.len() as u32,
            stage: "starting web search".into(),
        });

        let mut total_docs = 0u32;
        let blocked = bg_state.config.read().ok()
            .and_then(|c| c.blocked_domains.clone())
            .unwrap_or_default();

        for (qi, query) in queries.iter().enumerate() {
            eprintln!("[enrich] searching: {}", query);

            // Web search
            let results = match crate::handlers::web_search::search(&bg_state, query).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[enrich] search error for \"{}\": {}", query, e);
                    continue;
                }
            };

            let _ = bg_state.event_bus.publish(GraphEvent::IngestProgress {
                operation: "enrichment".into(),
                document: query.clone().into(),
                processed: qi as u32 + 1,
                total: queries.len() as u32,
                stage: format!("found {} results", results.len()).into(),
            });

            // Fetch and store top articles as pending documents
            for result in results.iter().take(max_sources) {
                // Skip blocked domains
                if blocked.iter().any(|d| result.url.contains(d)) { continue; }

                let client = &bg_state.http_client;
                let fetched = match super::debate::research::fetch_article_content(
                    client, &result.url, &blocked
                ).await {
                    Some(f) => f,
                    None => continue,
                };

                // Store in doc_store
                let content_hash = engram_core::storage::doc_store::DocStore::hash_content(fetched.text.as_bytes());
                let content_hash_hex = engram_core::storage::doc_store::DocStore::hash_hex(&content_hash);
                let mime = engram_core::storage::doc_store::MimeType::from_mime_str(&fetched.mime_type);

                {
                    if let Ok(mut store) = bg_state.doc_store.write() {
                        let _ = store.store(fetched.text.as_bytes(), mime);
                    }
                }

                // Create pending document node
                super::debate::research::create_pending_document_node(
                    &bg_state.graph,
                    &content_hash_hex,
                    Some(&result.url),
                    Some(&result.title),
                    &fetched.mime_type,
                    fetched.text.len(),
                    None,
                );

                total_docs += 1;
                eprintln!("[enrich] stored doc: {} ({} chars)", result.title, fetched.text.len());
            }
        }

        let _ = bg_state.event_bus.publish(GraphEvent::IngestProgress {
            operation: "enrichment".into(),
            document: "".into(),
            processed: queries.len() as u32,
            total: queries.len() as u32,
            stage: format!("complete: {} documents stored as pending", total_docs).into(),
        });

        // Trigger reprocess on pending docs if any were created
        if total_docs > 0 {
            eprintln!("[enrich] triggering reprocess on {} new pending docs", total_docs);
            // The reprocess will be picked up by the user clicking "Process Pending" or we could auto-trigger
            // For now, we just mark dirty so the UI updates
            bg_state.mark_dirty();
        }

        eprintln!("[enrich] enrichment complete: {} queries, {} docs", queries.len(), total_docs);
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "queries": query_count,
    })))
}

#[cfg(not(feature = "reason"))]
pub async fn enrich_plan() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

#[cfg(not(feature = "reason"))]
pub async fn enrich_run() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "reason feature not enabled".into() }))
}

// ── GET /conflicts -- List detected conflicts (conflicts_with edges) ──

pub async fn get_conflicts(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let g = state.graph.read().unwrap();
    let all_nodes = g.all_nodes().unwrap_or_default();

    let mut conflicts = Vec::new();
    for node in &all_nodes {
        if let Ok(edges) = g.edges_from(&node.label) {
            for edge in &edges {
                if edge.relationship == "conflicts_with" {
                    let props = g.get_edge_properties(edge.edge_slot)
                        .unwrap_or_default();
                    let resolution = props.get("resolution").cloned().unwrap_or_default();
                    conflicts.push(serde_json::json!({
                        "entity": node.label,
                        "target": edge.to,
                        "description": props.get("description").cloned().unwrap_or_default(),
                        "severity": props.get("severity").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.5),
                        "detected_at": props.get("detected_at").cloned().unwrap_or_default(),
                        "resolution": resolution,
                        "edge_slot": edge.edge_slot,
                    }));
                }
            }
        }
    }

    Json(serde_json::json!({
        "conflicts": conflicts,
        "total": conflicts.len(),
    }))
}

// ── POST /conflicts/resolve -- Resolve a conflict ──

pub async fn resolve_conflict(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let edge_slot = req.get("edge_slot").and_then(|v| v.as_u64())
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "edge_slot required"))?;
    let resolution = req.get("resolution").and_then(|v| v.as_str())
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "resolution required (accepted_new, kept_old, disputed)"))?;

    let valid = ["accepted_new", "kept_old", "disputed", "unresolved"];
    if !valid.contains(&resolution) {
        return Err(api_err(StatusCode::BAD_REQUEST, format!("invalid resolution, use one of: {:?}", valid)));
    }

    {
        let mut g = state.graph.write().unwrap();
        g.set_edge_property(edge_slot, "resolution", resolution);
    }
    state.mark_dirty();

    Ok(Json(serde_json::json!({ "ok": true, "resolution": resolution })))
}
