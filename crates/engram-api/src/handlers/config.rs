use super::*;

// ── GET /config ── Return current effective configuration

pub async fn get_config(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());

    // Build response with env var fallbacks for unset fields
    let effective_embed_endpoint = cfg.embed_endpoint.clone()
        .or_else(|| std::env::var("ENGRAM_EMBED_ENDPOINT").ok());
    let effective_embed_model = cfg.embed_model.clone()
        .or_else(|| std::env::var("ENGRAM_EMBED_MODEL").ok());
    let effective_llm_endpoint = cfg.llm_endpoint.clone()
        .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
    let effective_llm_model = cfg.llm_model.clone()
        .or_else(|| std::env::var("ENGRAM_LLM_MODEL").ok());

    // Mask API key: never return the actual value
    let has_llm_api_key = cfg.llm_api_key.is_some()
        || std::env::var("ENGRAM_LLM_API_KEY").is_ok();

    Json(serde_json::json!({
        "embed_endpoint": effective_embed_endpoint,
        "embed_model": effective_embed_model,
        "llm_endpoint": effective_llm_endpoint,
        "llm_model": effective_llm_model,
        "has_llm_api_key": has_llm_api_key,
        "llm_temperature": cfg.llm_temperature,
        "llm_thinking": cfg.llm_thinking.unwrap_or(false),
        "llm_context_window": cfg.llm_context_window,
        "pipeline_batch_size": cfg.pipeline_batch_size,
        "pipeline_workers": cfg.pipeline_workers,
        "pipeline_skip_stages": cfg.pipeline_skip_stages,
        "ner_provider": cfg.ner_provider,
        "ner_model": cfg.ner_model,
        "ner_endpoint": cfg.ner_endpoint,
        "rel_model": cfg.rel_model,
        "rel_threshold": cfg.rel_threshold.unwrap_or(0.9),
        "relation_templates": cfg.relation_templates,
        "coreference_enabled": cfg.coreference_enabled.unwrap_or(true),
        "mesh_enabled": cfg.mesh_enabled,
        "mesh_topology": cfg.mesh_topology,
        "quantization_enabled": cfg.quantization_enabled.unwrap_or(true),
        "web_search_provider": cfg.web_search_provider,
        "web_search_url": cfg.web_search_url,
        "has_web_search_api_key": cfg.web_search_api_key.is_some(),
        "web_search_providers": cfg.web_search_providers,
        "debate_debug": cfg.debate_debug.unwrap_or(false),
        "blocked_domains": cfg.blocked_domains.clone().unwrap_or_else(|| vec![
            "studylibid.com".into(), "studylib.net".into(), "doczz.net".into(),
        ]),
        "output_language": cfg.output_language,
        "ner_entity_labels": cfg.ner_entity_labels,
        "ner_auto_label_threshold": cfg.ner_auto_label_threshold.unwrap_or(3),
        "llm_system_prompt": cfg.llm_system_prompt,
        "domains": cfg.domains,
        "conflict_singular_properties": cfg.conflict_singular_properties.clone().unwrap_or_else(|| vec![
            "ceo".into(), "president".into(), "capital".into(), "population".into(), "founded".into(),
        ]),
    }))
}

// ── POST /config ── Update configuration (partial updates supported)

pub async fn set_config(
    State(state): State<AppState>,
    Json(patch): Json<EngineConfig>,
) -> ApiResult<serde_json::Value> {
    // Detect if embedder settings changed before merging
    let embed_changed = patch.embed_endpoint.is_some() || patch.embed_model.is_some();

    // If embedder settings changed, create new embedder BEFORE acquiring locks
    let new_embedder = if embed_changed {
        let cfg = state.config.read().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
        })?;

        // Resolve effective values after merge
        let endpoint = patch.embed_endpoint.clone()
            .or_else(|| cfg.embed_endpoint.clone())
            .or_else(|| std::env::var("ENGRAM_EMBED_ENDPOINT").ok());
        let model = patch.embed_model.clone()
            .or_else(|| cfg.embed_model.clone())
            .or_else(|| std::env::var("ENGRAM_EMBED_MODEL").ok());
        drop(cfg);

        match (endpoint, model) {
            (Some(ep), Some(_m)) if ep.starts_with("onnx://") => {
                // ONNX uses local sidecar files, not an API endpoint.
                // The /config/onnx-download or /config/onnx-model handler
                // hot-loads the OnnxEmbedder once files are present.
                // Skip probe here -- save config only.
                None
            }
            (Some(ep), Some(m)) => {
                // Create new embedder with probe for dimension detection
                let embedder = engram_core::ApiEmbedder::new(
                    ep.clone(), m.clone(), 0, None,
                );
                let dim = embedder.probe_dimension().map_err(|e| {
                    api_err(StatusCode::BAD_REQUEST, format!("embedder probe failed: {e}"))
                })?;
                let embedder = engram_core::ApiEmbedder::new(ep, m, dim, None);
                Some(embedder)
            }
            _ => None,
        }
    } else {
        None
    };

    // Merge the patch into current config
    {
        let mut cfg = state.config.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
        })?;
        cfg.merge(&patch);
    }

    // Auto-detect LLM context window when model or endpoint changes
    let llm_changed = patch.llm_model.is_some() || patch.llm_endpoint.is_some();
    if llm_changed {
        let (endpoint, model, api_key, has_manual_ctx) = {
            let cfg = state.config.read().map_err(|_| {
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
            })?;
            (
                cfg.llm_endpoint.clone().unwrap_or_default(),
                cfg.llm_model.clone().unwrap_or_default(),
                cfg.llm_api_key.clone().unwrap_or_default(),
                // Only auto-detect if user didn't explicitly set it in this request
                patch.llm_context_window.is_some(),
            )
        };
        if !has_manual_ctx && !endpoint.is_empty() && !model.is_empty() {
            let ep = endpoint.clone();
            let m = model.clone();
            let ak = api_key.clone();
            let state_clone = state.clone();
            // Spawn async detection (don't block the config response)
            tokio::spawn(async move {
                // Detect context window
                if let Some(ctx) = super::debate::llm::detect_context_window(&ep, &m, &ak).await {
                    eprintln!("[config] Auto-detected LLM context window: {} tokens for model '{}'", ctx, m);
                    if let Ok(mut cfg) = state_clone.config.write() {
                        cfg.llm_context_window = Some(ctx);
                    }
                    let _ = state_clone.save_config();
                } else {
                    eprintln!("[config] WARNING: Could not auto-detect context window for '{}'. \
                               Set it manually via POST /config {{\"llm_context_window\": 32768}}", m);
                }

                // Auto-detect thinking model
                let is_thinking = super::debate::llm::detect_thinking_model(&ep, &m).await;
                if is_thinking {
                    eprintln!("[config] Auto-detected reasoning/thinking model: '{}'", m);
                    if let Ok(mut cfg) = state_clone.config.write() {
                        if cfg.llm_thinking.is_none() {
                            cfg.llm_thinking = Some(true);
                        }
                    }
                    let _ = state_clone.save_config();
                }
            });
        }
    }

    // Invalidate cached NER/REL backends if relevant config fields changed
    #[cfg(feature = "ingest")]
    {
        let ner_changed = patch.ner_model.is_some() || patch.ner_provider.is_some()
            || patch.ner_entity_labels.is_some() || patch.ner_auto_label_threshold.is_some();
        let rel_changed = patch.rel_model.is_some() || patch.relation_templates.is_some();
        if ner_changed {
            if let Ok(mut c) = state.cached_ner.write() {
                *c = None;
                tracing::info!("NER backend cache invalidated (config changed)");
            }
        }
        if rel_changed {
            if let Ok(mut c) = state.cached_rel.write() {
                *c = None;
                tracing::info!("REL backend cache invalidated (config changed)");
            }
        }
    }

    // Hot-reload embedder if settings changed
    if let Some(embedder) = new_embedder {
        let model = embedder.model_id().to_string();
        let dim = embedder.dim();
        let endpoint = {
            let cfg = state.config.read().map_err(|_| {
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
            })?;
            cfg.embed_endpoint.clone().unwrap_or_default()
        };

        // Acquire graph write lock and install new embedder
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        g.set_embedder(Box::new(embedder));
        drop(g);

        // Update compute info (no lock needed, but we need interior mutability workaround)
        // ComputeInfo is on the cloned AppState, so we log it. The /compute endpoint
        // will reflect the config values via GET /config instead.
        tracing::info!("hot-reloaded embedder: {} ({}D) via {}", model, dim, endpoint);
    }

    // Persist to sidecar file
    state.save_config().map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("config save failed: {e}"))
    })?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "configuration updated"
    })))
}

// ── POST /reindex -- Re-embed all nodes ──────────────────────────────

/// POST /reindex -- Re-embed all active nodes using the current embedder.
/// Call after changing the embedding model or endpoint.
pub async fn reindex(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let count = g.reindex()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    drop(g);
    state.mark_dirty();
    Ok(Json(serde_json::json!({
        "status": "ok",
        "reindexed": count,
    })))
}

/// POST /kge/train -- Trigger KGE (RotatE) training on current graph.
#[cfg(feature = "ingest")]
pub async fn kge_train(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let epochs = body.get("epochs").and_then(|v| v.as_u64()).unwrap_or(100) as u32;

    let graph = state.graph.clone();
    let brain_path = {
        let g = graph.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph lock poisoned"))?;
        g.path().to_path_buf()
    };

    let result = tokio::task::spawn_blocking(move || {
        let mut model = engram_ingest::KgeModel::load(&brain_path, engram_ingest::KgeConfig::default())
            .unwrap_or_else(|_| engram_ingest::KgeModel::new(&brain_path, engram_ingest::KgeConfig::default()));

        let g = graph.read().map_err(|_| "graph lock poisoned".to_string())?;
        let stats = model.train_full(&g, epochs).map_err(|e| e.to_string())?;
        drop(g);

        model.save().map_err(|e| e.to_string())?;

        Ok::<_, String>(stats)
    })
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "epochs_completed": result.epochs_completed,
        "final_loss": result.final_loss,
        "entity_count": result.entity_count,
        "relation_type_count": result.relation_type_count,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn kge_train() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── GET /config/status ──

pub async fn config_status(
    State(state): State<AppState>,
) -> ApiResult<ConfigStatusResponse> {
    let cfg = state.config.read().map_err(|_| read_lock_err())?;
    let mut configured = Vec::new();
    let mut missing = Vec::new();
    let mut warnings = Vec::new();

    // Check embedder
    if cfg.embed_endpoint.is_some() {
        configured.push("embed_endpoint".to_string());
    } else {
        missing.push("embed_endpoint".to_string());
    }

    // Check LLM
    if cfg.llm_endpoint.is_some() {
        configured.push("llm_endpoint".to_string());
    } else {
        missing.push("llm_endpoint".to_string());
    }

    // Check NER
    if cfg.ner_provider.is_some() {
        configured.push("ner_provider".to_string());
    } else {
        warnings.push("ner_provider not set -- NER will use fallback chain only".to_string());
    }

    // Check KB endpoints
    if let Some(ref kbs) = cfg.kb_endpoints {
        if !kbs.is_empty() {
            configured.push(format!("kb_endpoints ({})", kbs.len()));
        }
    }

    let (node_count, edge_count) = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        g.stats()
    };

    let ready = cfg.embed_endpoint.is_some();

    let wizard_dismissed = cfg.wizard_dismissed.unwrap_or(false);

    Ok(Json(ConfigStatusResponse {
        configured,
        missing,
        warnings,
        ready,
        node_count,
        edge_count,
        is_empty_graph: node_count == 0 && edge_count == 0,
        wizard_dismissed,
    }))
}

// ── POST /config/wizard-complete ──

pub async fn wizard_complete(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    {
        let mut cfg = state.config.write().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "config lock poisoned".into() }))
        })?;
        cfg.wizard_dismissed = Some(true);
    }
    state.save_config().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("failed to save config: {}", e) }))
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── GET /config/entity-labels ── Effective NER entity label set with breakdown

#[cfg(feature = "gliner2")]
pub async fn get_entity_labels(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    use super::ingest::{resolve_entity_labels, CORE_ENTITY_LABELS, MAX_ENTITY_LABELS};

    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    let user_labels = cfg.ner_entity_labels.clone().unwrap_or_default();
    let threshold = cfg.ner_auto_label_threshold.unwrap_or(3);
    drop(cfg);

    let g = state.graph.read().unwrap();
    let effective = resolve_entity_labels(&g, &user_labels, threshold);

    // Build auto-discovered breakdown
    let core: Vec<&str> = CORE_ENTITY_LABELS.to_vec();
    let core_set: std::collections::HashSet<String> = core.iter().map(|s| s.to_string()).collect();
    let user_set: std::collections::HashSet<String> = user_labels.iter().map(|s| s.to_lowercase().replace(' ', "_")).collect();

    let auto_discovered: Vec<serde_json::Value> = {
        let all_types = g.all_node_types();
        all_types.iter()
            .filter_map(|t| {
                let lower = t.to_lowercase();
                if lower == "document" || lower == "source" || lower == "fact"
                    || core_set.contains(&lower) || user_set.contains(&lower)
                {
                    return None;
                }
                let count = g.count_nodes_of_type(t);
                if count >= threshold as usize {
                    Some(serde_json::json!({"label": lower, "count": count}))
                } else {
                    None
                }
            })
            .collect()
    };

    Json(serde_json::json!({
        "core": core,
        "user_defined": user_labels,
        "auto_discovered": auto_discovered,
        "effective": effective,
        "max_labels": MAX_ENTITY_LABELS,
        "auto_threshold": threshold,
    }))
}

// ── POST /config/entity-labels ── Set user-defined labels and/or threshold

#[cfg(feature = "gliner2")]
pub async fn set_entity_labels(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let labels = req.get("labels").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
    });
    let threshold = req.get("auto_threshold").and_then(|v| v.as_u64()).map(|n| n as u32);

    {
        let mut cfg = state.config.write().unwrap();
        if let Some(labels) = labels {
            cfg.ner_entity_labels = Some(labels);
        }
        if let Some(t) = threshold {
            cfg.ner_auto_label_threshold = Some(t);
        }
    }

    // Save config + invalidate NER cache
    state.save_config().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("failed to save config: {}", e) }))
    })?;
    #[cfg(feature = "ingest")]
    {
        if let Ok(mut c) = state.cached_ner.write() {
            *c = None;
            eprintln!("[ner] entity labels updated, NER cache invalidated");
        }
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── GET /config/domains ── List user-defined domains

pub async fn get_domains(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    let domains = cfg.domains.clone().unwrap_or_default();
    Json(serde_json::json!({
        "domains": domains,
    }))
}

// ── POST /config/domains ── Set user-defined domains

pub async fn set_domains(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let domains = req.get("domains").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
    });

    if let Some(domains) = domains {
        let mut cfg = state.config.write().unwrap();
        cfg.domains = Some(domains);
    }

    state.save_config().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("failed to save config: {}", e) }))
    })?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── GET /config/domains/suggest ── LLM suggests domains from graph content

pub async fn suggest_domains(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    use super::debate::llm::{call_llm, extract_content, research_context};

    // Gather graph context: top entity types + high-degree nodes
    let context = {
        let g = state.graph.read().unwrap();
        let types = g.all_node_types();
        let type_counts: Vec<String> = types.iter()
            .filter(|t| !["Document", "Source", "Fact", ""].contains(&t.as_str()))
            .map(|t| format!("{}: {} entities", t, g.count_nodes_of_type(t)))
            .collect();

        let all = g.all_nodes().unwrap_or_default();
        let mut by_edges: Vec<(&str, usize)> = all.iter()
            .filter(|n| n.node_type.as_deref().unwrap_or("") != "Document"
                     && n.node_type.as_deref().unwrap_or("") != "Source")
            .map(|n| {
                let ec = g.edges_from(&n.label).map(|e| e.len()).unwrap_or(0)
                    + g.edges_to(&n.label).map(|e| e.len()).unwrap_or(0);
                (n.label.as_str(), ec)
            })
            .collect();
        by_edges.sort_by(|a, b| b.1.cmp(&a.1));
        let top_entities: Vec<String> = by_edges.iter().take(20)
            .map(|(l, c)| format!("{} ({} connections)", l, c))
            .collect();

        format!("Entity types:\n{}\n\nTop entities:\n{}", type_counts.join("\n"), top_entities.join("\n"))
    };

    let sys_prompt = research_context(&state);
    let prompt = format!(
        "{}Based on this knowledge graph content, suggest 3-8 research domains that would group these entities meaningfully.\n\n\
         {}\n\n\
         Return ONLY a JSON array of domain names, e.g. [\"Russia-EU Energy\", \"Semiconductor Supply Chain\"]. \
         Each domain should be 2-5 words, specific enough to be useful, broad enough to group multiple entities.",
        if sys_prompt.is_empty() { String::new() } else { format!("Research context: {}\n\n", sys_prompt) },
        context
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a knowledge graph analyst. Suggest meaningful research domains."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 256,
        "think": false
    });

    match call_llm(&state, request).await {
        Ok(resp) => {
            let content = extract_content(&resp).unwrap_or_default();
            let suggestions: Vec<String> = if let Ok(arr) = serde_json::from_str::<Vec<String>>(&content) {
                arr
            } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                val.as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default()
            } else {
                content.lines()
                    .map(|l| l.trim().trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '*').trim())
                    .filter(|l| !l.is_empty() && l.len() > 3 && l.len() < 60)
                    .map(|l| l.trim_matches('"').to_string())
                    .collect()
            };

            Ok(Json(serde_json::json!({ "suggestions": suggestions })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("LLM error: {}", e) }))),
    }
}

// ── POST /config/domains/classify ── Classify undomained entities into user-defined domains

pub async fn classify_domains(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    use super::debate::llm::{call_llm, extract_content, research_context};

    let domains = state.config.read().ok()
        .and_then(|c| c.domains.clone())
        .unwrap_or_default();

    if domains.is_empty() {
        return Err((StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "No domains defined. Set domains first via POST /config/domains.".into() })));
    }

    // Find entities without domain property (batch of up to 50)
    let undomained: Vec<(String, String)> = {
        let g = state.graph.read().unwrap();
        let all = g.all_nodes().unwrap_or_default();
        all.iter()
            .filter(|n| {
                let nt = n.node_type.as_deref().unwrap_or("");
                nt != "Document" && nt != "Source" && nt != "Fact" && !nt.is_empty()
            })
            .filter(|n| !n.properties.contains_key("domain"))
            .take(50)
            .map(|n| (n.label.clone(), n.node_type.clone().unwrap_or_default()))
            .collect()
    };

    if undomained.is_empty() {
        return Ok(Json(serde_json::json!({ "classified": 0, "message": "All entities already have domains." })));
    }

    let entity_list: String = undomained.iter()
        .map(|(label, nt)| format!("- {} ({})", label, nt))
        .collect::<Vec<_>>()
        .join("\n");

    let sys_prompt = research_context(&state);
    let prompt = format!(
        "{}Classify each entity into ONE of these research domains: {:?}\n\n\
         If an entity doesn't clearly fit any domain, use \"uncategorized\".\n\n\
         Entities:\n{}\n\n\
         Return ONLY a JSON object mapping entity labels to domain names, e.g.:\n\
         {{\"Germany\": \"Russia-EU Energy\", \"TSMC\": \"Semiconductor Supply Chain\"}}",
        if sys_prompt.is_empty() { String::new() } else { format!("Research context: {}\n\n", sys_prompt) },
        domains, entity_list
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a knowledge graph analyst. Classify entities into research domains."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": 2048,
        "think": false
    });

    match call_llm(&state, request).await {
        Ok(resp) => {
            let content = extract_content(&resp).unwrap_or_default();
            let mut classified = 0u32;

            if let Ok(mapping) = serde_json::from_str::<std::collections::HashMap<String, String>>(&content) {
                let mut g = state.graph.write().unwrap();
                for (entity, domain) in &mapping {
                    if domain != "uncategorized" && domains.contains(domain) {
                        let _ = g.set_property(entity, "domain", domain);
                        classified += 1;
                    }
                }
            } else if let Ok(val) = super::debate::llm::parse_json_from_llm(&content) {
                if let Some(obj) = val.as_object() {
                    let mut g = state.graph.write().unwrap();
                    for (entity, domain_val) in obj {
                        if let Some(domain) = domain_val.as_str() {
                            if domain != "uncategorized" && domains.contains(&domain.to_string()) {
                                let _ = g.set_property(entity, "domain", domain);
                                classified += 1;
                            }
                        }
                    }
                }
            }

            if classified > 0 {
                state.mark_dirty();
            }

            Ok(Json(serde_json::json!({
                "classified": classified,
                "total_undomained": undomained.len(),
            })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("LLM error: {}", e) }))),
    }
}
