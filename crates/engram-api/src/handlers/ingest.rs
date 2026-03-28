use super::*;

// ── POST /ingest -- ingest pipeline ──

/// Build a pipeline with all available NER + relation extraction backends wired up.
/// Public for MCP tool access.
#[cfg(feature = "ingest")]
pub fn build_pipeline_mcp(
    graph: std::sync::Arc<std::sync::RwLock<engram_core::graph::Graph>>,
    config: engram_ingest::PipelineConfig,
    kb_endpoints: Option<Vec<crate::state::KbEndpointConfig>>,
    ner_model: Option<String>,
    rel_model: Option<String>,
) -> engram_ingest::Pipeline {
    // MCP / A2A callers don't have access to AppState caches -- pass empty caches.
    let no_ner_cache = std::sync::Arc::new(std::sync::RwLock::new(None));
    let no_rel_cache = std::sync::Arc::new(std::sync::RwLock::new(None));
    build_pipeline(graph, config, kb_endpoints, ner_model, rel_model, None, None, None,
        no_ner_cache, no_rel_cache)
}

/// Build a pipeline with all available NER + relation extraction backends wired up.
/// `ner_model` / `rel_model` come from EngineConfig -- the user's chosen models.
/// `ner_cache` / `rel_cache` are shared caches -- if populated, the backend is reused
/// instead of reloading the model from disk/HF. If empty, a new backend is created and cached.
#[cfg(feature = "ingest")]
pub(crate) fn build_pipeline(
    graph: std::sync::Arc<std::sync::RwLock<engram_core::graph::Graph>>,
    config: engram_ingest::PipelineConfig,
    kb_endpoints: Option<Vec<crate::state::KbEndpointConfig>>,
    _ner_model: Option<String>,
    _rel_model: Option<String>,
    relation_templates: Option<std::collections::HashMap<String, String>>,
    rel_threshold: Option<f32>,
    _coreference_enabled: Option<bool>,
    _ner_cache: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn engram_ingest::Extractor>>>>,
    _rel_cache: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn engram_ingest::RelationExtractor>>>>,
) -> engram_ingest::Pipeline {
    use engram_ingest::{NerChain, ChainStrategy};

    let mut pipeline = engram_ingest::Pipeline::new(graph.clone(), config);

    // Build a MergeAll NER chain: run ALL backends, merge + dedup results.
    // This ensures gazetteer resolved_to IDs are preserved AND GLiNER finds
    // new entities the gazetteer doesn't know about yet.
    let mut chain = NerChain::new(ChainStrategy::MergeAll);

    // 1. Rule-based NER (emails, IPs, dates -- always available, fast)
    chain.add_backend(Box::new(engram_ingest::RuleBasedNer::default()));

    // 2. Graph-derived gazetteer (known entities with resolved node IDs)
    {
        let brain_path = {
            let g = graph.read().unwrap();
            g.path().to_path_buf()
        };
        let mut gaz = engram_ingest::GraphGazetteer::new(&brain_path, 0.3);
        {
            let g = graph.read().unwrap();
            gaz.build_from_graph(&g);
        }
        let gaz = std::sync::Arc::new(tokio::sync::RwLock::new(gaz));
        chain.add_backend(Box::new(engram_ingest::GazetteerExtractor::new(gaz)));
    }

    // 3. GLiNER2 in-process backend (unified NER + RE, no sidecar)
    #[cfg(feature = "gliner2")]
    let gliner2_arc: Option<std::sync::Arc<engram_ingest::gliner2_backend::Gliner2PipelineBackend>> = {
        use engram_ingest::gliner2_backend::{Gliner2Backend, find_gliner2_model};

        if let Some(cfg) = find_gliner2_model() {
            let variant = "fp16"; // default to FP16 hybrid (half size, full precision)
            match Gliner2Backend::load(&cfg.model_dir, variant) {
                Ok(backend) => {
                    // Default entity labels and relation types
                    let entity_labels = vec![
                        "person".into(), "organization".into(), "location".into(),
                        "date".into(), "event".into(), "product".into(),
                    ];
                    let relation_types: Vec<String> = relation_templates
                        .as_ref()
                        .map(|t| t.keys().cloned().collect())
                        .unwrap_or_else(|| vec![
                            "works_at".into(), "headquartered_in".into(),
                            "located_in".into(), "founded".into(),
                            "leads".into(), "supports".into(),
                        ]);
                    let threshold = rel_threshold.unwrap_or(0.85);

                    let pb = engram_ingest::gliner2_backend::Gliner2PipelineBackend::new(
                        backend,
                        entity_labels,
                        relation_types,
                        0.5,       // NER threshold
                        threshold, // RE threshold (0.85 default -- facts only, no noise)
                    );
                    eprintln!("[build_pipeline] gliner2: loaded OK (variant={variant})");
                    tracing::info!(variant, "GLiNER2 unified NER+RE backend loaded");
                    Some(std::sync::Arc::new(pb))
                }
                Err(e) => {
                    eprintln!("[build_pipeline] gliner2: FAILED: {e}");
                    tracing::warn!("GLiNER2 backend failed: {e}");
                    None
                }
            }
        } else {
            None
        }
    };

    #[cfg(feature = "gliner2")]
    if let Some(ref arc) = gliner2_arc {
        let ner_arc: std::sync::Arc<dyn engram_ingest::Extractor> = arc.clone();
        chain.add_backend(Box::new(engram_ingest::ArcExtractor(ner_arc)));
    }

    eprintln!("[build_pipeline] NER chain: {} backends", chain.backend_count());
    tracing::info!("NER chain: MergeAll with {} backends", chain.backend_count());
    pipeline.add_extractor(Box::new(chain));

    // Add conservative entity resolver
    pipeline.add_resolver(Box::new(engram_ingest::ConservativeResolver::default()));

    // Add language detector
    pipeline.set_language_detector(Box::new(engram_ingest::DefaultLanguageDetector::default()));

    // ── Relation extraction chain (MergeAll: KB + gazetteer + KGE + optional GLiREL) ──
    let mut rel_chain = engram_ingest::RelationChain::new(0.15);

    // 0. Knowledge Base (SPARQL) -- bootstraps empty graphs
    //    Wire GLiNER2 backend into KB extractor for classifying unresolved co-occurrence pairs.
    if let Some(ref endpoints) = kb_endpoints {
        let enabled: Vec<engram_ingest::KbEndpoint> = endpoints
            .iter()
            .filter(|e| e.enabled)
            .map(|e| engram_ingest::KbEndpoint {
                name: e.name.clone(),
                url: e.url.clone(),
                auth_type: e.auth_type.clone(),
                auth_header: e.auth_secret_key.clone(),
                entity_link_template: e.entity_link_template.clone(),
                relation_query_template: e.relation_query_template.clone(),
                max_lookups: e.max_lookups_per_call.unwrap_or(50),
            })
            .collect();

        if !enabled.is_empty() {
            tracing::info!(count = enabled.len(), "KB relation extractor enabled");
            let mut kb_ext = engram_ingest::KbRelationExtractor::new(enabled, graph.clone());
            // Wire GLiNER2 for co-occurrence pair classification (discovery -> typed)
            #[cfg(feature = "gliner2")]
            if let Some(ref arc) = gliner2_arc {
                let re_arc: std::sync::Arc<dyn engram_ingest::RelationExtractor> = arc.clone();
                kb_ext.set_gliner2_backend(re_arc);
                tracing::info!("KB extractor: GLiNER2 wired for co-occurrence classification");
            }
            rel_chain.add_backend(Box::new(kb_ext));
        }
    }

    // 1. Relation gazetteer (known graph edges)
    {
        let brain_path = {
            let g = graph.read().unwrap();
            g.path().to_path_buf()
        };
        let mut rel_gaz = engram_ingest::RelationGazetteer::new(&brain_path);
        {
            let g = graph.read().unwrap();
            rel_gaz.build_from_graph(&g);
        }
        let rel_gaz = std::sync::Arc::new(tokio::sync::RwLock::new(rel_gaz));
        rel_chain.add_backend(Box::new(engram_ingest::RelationGazetteerExtractor::new(rel_gaz)));
    }

    // 2. KGE (RotatE) link prediction
    {
        let brain_path = {
            let g = graph.read().unwrap();
            g.path().to_path_buf()
        };
        let kge = engram_ingest::KgeModel::load(&brain_path, engram_ingest::KgeConfig::default())
            .unwrap_or_else(|_| engram_ingest::KgeModel::new(&brain_path, engram_ingest::KgeConfig::default()));
        let kge = std::sync::Arc::new(std::sync::RwLock::new(kge));
        rel_chain.add_backend(Box::new(engram_ingest::KgeRelationExtractor::new(kge)));
    }

    // 3. GLiNER2 relation extraction (same backend instance as NER, shared Arc)
    #[cfg(feature = "gliner2")]
    if let Some(ref arc) = gliner2_arc {
        let re_arc: std::sync::Arc<dyn engram_ingest::RelationExtractor> = arc.clone();
        rel_chain.add_backend(Box::new(engram_ingest::ArcRelationExtractor(re_arc)));
    }

    tracing::info!("Relation chain: MergeAll with {} backends", rel_chain.backend_count());
    pipeline.add_relation_extractor(Box::new(rel_chain));

    pipeline
}

#[cfg(feature = "ingest")]
pub async fn ingest(
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> ApiResult<IngestResponse> {
    use engram_ingest::{PipelineConfig, types::StageConfig};

    let source = req.source.unwrap_or_else(|| "api-ingest".into());

    // Build pipeline config with skip stages
    let mut stages = StageConfig::default();
    let mut skipped_names = Vec::new();
    if let Some(ref skip) = req.skip {
        let unknown = stages.apply_skip(skip);
        if !unknown.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                format!("unknown stages to skip: {}", unknown.join(", ")),
            ));
        }
        skipped_names = stages.skipped_stages().iter().map(|s| s.to_string()).collect();
    }

    let config = PipelineConfig {
        name: "api-ingest".into(),
        stages,
        ..Default::default()
    };

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled)
    };
    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();

    // Convert IngestItems to RawItems
    let items: Vec<engram_ingest::types::RawItem> = req.items
        .into_iter()
        .map(|item| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            match item {
                IngestItem::WithUrl { content, source_url } => engram_ingest::types::RawItem {
                    content: engram_ingest::types::Content::Text(content),
                    source_url,
                    source_name: source.clone(),
                    fetched_at: now,
                    metadata: Default::default(),
                },
                IngestItem::Text(text) => engram_ingest::types::RawItem {
                    content: engram_ingest::types::Content::Text(text),
                    source_url: None,
                    source_name: source.clone(),
                    fetched_at: now,
                    metadata: Default::default(),
                },
                IngestItem::Structured(map) => engram_ingest::types::RawItem {
                    content: engram_ingest::types::Content::Structured(map),
                    source_url: None,
                    source_name: source.clone(),
                    fetched_at: now,
                    metadata: Default::default(),
                },
            }
        })
        .collect();

    let review_mode = req.review.unwrap_or(false);

    if review_mode {
        // Review mode: run analyze (NER + RE) but don't commit to graph.
        // Store results in IngestSession for later review + selective commit.
        let result = tokio::task::spawn_blocking(move || {
            let mut pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
                relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
            pipeline.set_doc_store(doc_store.clone());
            pipeline.analyze(items)
        })
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let session_id = format!("ingest-{:016x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64);

        let entities: Vec<crate::state::IngestPreviewEntity> = result.entities.iter().map(|e| {
            crate::state::IngestPreviewEntity {
                label: e.text.clone(),
                entity_type: e.entity_type.clone(),
                confidence: e.confidence,
            }
        }).collect();

        let relations: Vec<crate::state::IngestPreviewRelation> = result.relations.iter().map(|r| {
            let is_sparql = r.confidence >= 0.70 && matches!(r.method, engram_ingest::ExtractionMethod::KnowledgeBase);
            crate::state::IngestPreviewRelation {
                from: r.from.clone(),
                to: r.to.clone(),
                rel_type: r.rel_type.clone(),
                confidence: r.confidence,
                method: format!("{:?}", r.method),
                tier: crate::state::ConnectionTier::from_confidence(r.confidence, is_sparql),
            }
        }).collect();

        let session = crate::state::IngestSession {
            session_id: session_id.clone(),
            entities,
            relations,
            created_at: std::time::Instant::now(),
        };

        state.ingest_sessions.write().unwrap().insert(session_id.clone(), session);

        // Return a response with session_id. facts_stored=0 since nothing committed yet.
        return Ok(Json(IngestResponse {
            facts_stored: 0,
            relations_created: 0,
            relations_deduplicated: 0,
            facts_resolved: 0,
            facts_deduped: 0,
            conflicts_detected: 0,
            errors: Vec::new(),
            duration_ms: result.duration_ms,
            stages_skipped: skipped_names,
            warnings: vec![format!("review_session:{}", session_id)],
            kb_stats: None,
        }));
    }

    // Normal mode: run pipeline and commit to graph.
    let parallel = req.parallel.unwrap_or(false);
    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
        pipeline.set_doc_store(doc_store.clone());
        if parallel {
            pipeline.execute_parallel(items)
        } else {
            pipeline.execute(items)
        }
    })
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.mark_dirty();

    Ok(Json(IngestResponse {
        facts_stored: result.facts_stored,
        relations_created: result.relations_created,
        relations_deduplicated: result.relations_deduplicated,
        facts_resolved: result.facts_resolved,
        facts_deduped: result.facts_deduped,
        conflicts_detected: result.conflicts_detected,
        errors: result.errors,
        duration_ms: result.duration_ms,
        stages_skipped: skipped_names,
        warnings: result.warnings,
        kb_stats: result.kb_stats.map(|s| KbStatsResponse {
            endpoint: s.endpoint,
            entities_linked: s.entities_linked,
            entities_not_found: s.entities_not_found,
            relations_found: s.relations_found,
            errors: s.errors,
            lookup_ms: s.lookup_ms,
        }),
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled -- rebuild with --features ingest".into() }))
}

/// POST /ingest/analyze -- run NER on text without storing, for preview
#[cfg(feature = "ingest")]
pub async fn ingest_analyze(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeRequest>,
) -> ApiResult<AnalyzeResponse> {
    use engram_ingest::PipelineConfig;

    let config = PipelineConfig::default();
    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled)
    };
    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();
    let text = req.text;

    // Run build_pipeline + analyze in spawn_blocking to avoid tokio runtime panic
    // from reqwest::blocking (KbRelationExtractor)
    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
        pipeline.set_doc_store(doc_store.clone());
        let items = vec![engram_ingest::types::RawItem {
            content: engram_ingest::types::Content::Text(text),
            source_url: None,
            source_name: "analyze".into(),
            fetched_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            metadata: Default::default(),
        }];
        pipeline.analyze(items)
    })
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entities = result
        .entities
        .into_iter()
        .map(|e| AnalyzeEntityResponse {
            text: e.text,
            entity_type: e.entity_type,
            confidence: e.confidence,
            method: format!("{:?}", e.method),
            span: e.span,
            resolved_to: e.resolved_to,
        })
        .collect();

    let relations = result
        .relations
        .into_iter()
        .map(|r| AnalyzeRelationResponse {
            from: r.from,
            to: r.to,
            rel_type: r.rel_type,
            confidence: r.confidence,
            method: format!("{:?}", r.method),
        })
        .collect();

    Ok(Json(AnalyzeResponse {
        entities,
        relations,
        language: result.language,
        duration_ms: result.duration_ms,
        warnings: result.warnings,
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest_analyze() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled -- rebuild with --features ingest".into() }))
}

/// POST /ingest/file -- ingest from file upload (multipart)
#[cfg(feature = "ingest")]
pub async fn ingest_file(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> ApiResult<IngestResponse> {
    use engram_ingest::{PipelineConfig, types::StageConfig};

    let source = params.get("source").cloned().unwrap_or_else(|| "file-upload".into());

    let mut stages = StageConfig::default();
    let mut skipped_names = Vec::new();
    if let Some(skip) = params.get("skip") {
        let unknown = stages.apply_skip(skip);
        if !unknown.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                format!("unknown stages to skip: {}", unknown.join(", ")),
            ));
        }
        skipped_names = stages.skipped_stages().iter().map(|s| s.to_string()).collect();
    }

    let config = PipelineConfig {
        name: "file-ingest".into(),
        stages,
        ..Default::default()
    };

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled)
    };
    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();

    // Try to parse body as UTF-8 text
    let text = String::from_utf8(body.to_vec())
        .map_err(|_| api_err(StatusCode::BAD_REQUEST, "file body is not valid UTF-8"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let items = vec![engram_ingest::types::RawItem {
        content: engram_ingest::types::Content::Text(text),
        source_url: None,
        source_name: source,
        fetched_at: now,
        metadata: Default::default(),
    }];

    // Run build_pipeline + execute in spawn_blocking to avoid tokio runtime panic
    let parallel = params.get("parallel").is_some_and(|v| v == "true" || v == "1");
    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
        pipeline.set_doc_store(doc_store.clone());
        if parallel {
            pipeline.execute_parallel(items)
        } else {
            pipeline.execute(items)
        }
    })
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.mark_dirty();

    Ok(Json(IngestResponse {
        facts_stored: result.facts_stored,
        relations_created: result.relations_created,
        relations_deduplicated: result.relations_deduplicated,
        facts_resolved: result.facts_resolved,
        facts_deduped: result.facts_deduped,
        conflicts_detected: result.conflicts_detected,
        errors: result.errors,
        duration_ms: result.duration_ms,
        stages_skipped: skipped_names,
        warnings: result.warnings,
        kb_stats: result.kb_stats.map(|s| KbStatsResponse {
            endpoint: s.endpoint,
            entities_linked: s.entities_linked,
            entities_not_found: s.entities_not_found,
            relations_found: s.relations_found,
            errors: s.errors,
            lookup_ms: s.lookup_ms,
        }),
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest_file() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled -- rebuild with --features ingest".into() }))
}

/// POST /ingest/configure -- update pipeline defaults (runtime).
#[cfg(feature = "ingest")]
pub async fn ingest_configure(
    State(state): State<AppState>,
    Json(req): Json<IngestConfigureRequest>,
) -> ApiResult<serde_json::Value> {
    let mut stages = engram_ingest::types::StageConfig::default();
    if let Some(ref skip) = req.skip {
        let unknown = stages.apply_skip(skip);
        if !unknown.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                format!("unknown stages to skip: {}", unknown.join(", ")),
            ));
        }
    }

    // Merge pipeline settings into runtime config and persist
    {
        let mut cfg = state.config.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned")
        })?;
        let patch = crate::state::EngineConfig {
            pipeline_batch_size: req.batch_size.map(|v| v as u32),
            pipeline_workers: req.workers.map(|v| v as u32),
            pipeline_skip_stages: req.skip.as_ref().map(|s| {
                s.split(',').map(|t| t.trim().to_string()).collect()
            }),
            ..Default::default()
        };
        cfg.merge(&patch);
    }
    // Persist pipeline config changes
    state.save_config().ok();

    Ok(Json(serde_json::json!({
        "name": req.name.unwrap_or_else(|| "default".into()),
        "batch_size": req.batch_size.unwrap_or(1000),
        "workers": req.workers.unwrap_or(4),
        "stages_enabled": stages.enabled_stages(),
        "stages_skipped": stages.skipped_stages(),
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest_configure() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled -- rebuild with --features ingest".into() }))
}

// ── GET /sources -- list registered sources (stub) ──

#[cfg(feature = "ingest")]
pub async fn list_sources(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let sources = state.source_registry.list_info();
    Ok(Json(serde_json::json!({ "sources": sources })))
}

#[cfg(not(feature = "ingest"))]
pub async fn list_sources() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── GET /sources/{name}/usage ──

#[cfg(feature = "ingest")]
pub async fn source_usage(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<serde_json::Value> {
    match state.source_registry.get_usage(&name) {
        Some(usage) => Ok(Json(serde_json::json!({
            "source": name,
            "usage": usage,
        }))),
        None => Err(api_err(StatusCode::NOT_FOUND, format!("source '{}' not registered", name))),
    }
}

#[cfg(not(feature = "ingest"))]
pub async fn source_usage() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── GET /sources/{name}/ledger ──

#[cfg(feature = "ingest")]
pub async fn source_ledger(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<serde_json::Value> {
    // Try to load the ledger from the brain file path
    let graph = state.graph.read().map_err(|_| read_lock_err())?;
    let brain_path = graph.path().to_path_buf();
    drop(graph);

    let ledger = engram_ingest::SearchLedger::open(&brain_path);
    let entries: Vec<_> = ledger.entries_for_source(&name).into_iter().cloned().collect();

    Ok(Json(serde_json::json!({
        "source": name,
        "entries": entries.len(),
        "ledger": entries,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn source_ledger() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── SSE event streaming ──────────────────────────────────────────────

/// SSE endpoint: subscribe to graph change events in real time.
pub async fn event_stream(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt;

    let topics: Vec<String> = query.get("topics")
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(move |result| {
            match result {
                Ok(event) => {
                    let event_type = event_type_name(&event);
                    // Filter by topics if specified
                    if !topics.is_empty() && !topics.iter().any(|t| t == "*" || t == event_type) {
                        return None;
                    }
                    let data = serde_json::to_string(&format!("{:?}", event)).unwrap_or_default();
                    Some(Ok(axum::response::sse::Event::default()
                        .event(event_type)
                        .data(data)))
                }
                Err(_) => None, // lagged, skip
            }
        });

    axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}

// ── Ingest review mode ────────────────────────────────────────────────

/// GET /ingest/review?session=xxx -- get tiered relation groups for review.
#[cfg(feature = "ingest")]
pub async fn ingest_review(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let session_id = query.get("session")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "session query param required"))?;

    let page: usize = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(0);
    let page_size: usize = query.get("page_size").and_then(|p| p.parse().ok()).unwrap_or(20);

    let sessions = state.ingest_sessions.read().unwrap();
    let session = sessions.get(session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "ingest session not found"))?;

    let total = session.relations.len();
    let start = (page * page_size).min(total);
    let end = ((page + 1) * page_size).min(total);
    let items: Vec<serde_json::Value> = session.relations[start..end].iter().enumerate().map(|(i, rel)| {
        serde_json::json!({
            "idx": start + i,
            "from": rel.from,
            "to": rel.to,
            "rel_type": rel.rel_type,
            "confidence": rel.confidence,
            "source": rel.method,
            "tier": rel.tier,
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "entities": session.entities,
        "total": total,
        "page": page,
        "page_size": page_size,
        "items": items,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest_review() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /ingest/review/confirm -- commit accepted relations from an ingest review session.
#[cfg(feature = "ingest")]
pub async fn ingest_review_confirm(
    State(state): State<AppState>,
    Json(req): Json<IngestReviewConfirmRequest>,
) -> ApiResult<IngestResponse> {
    let start = std::time::Instant::now();

    let session = {
        let sessions = state.ingest_sessions.read().unwrap();
        sessions.get(&req.session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "ingest session not found"))?
            .clone()
    };

    let accepted_set: std::collections::HashSet<usize> = req.accepted.iter().copied().collect();
    let modified_map: std::collections::HashMap<usize, String> = req.modified.iter()
        .map(|m| (m.idx, m.new_rel_type.clone()))
        .collect();

    // Build set of explicitly accepted indices (no auto-accept)
    let mut keep: std::collections::HashSet<usize> = accepted_set;
    for m in &req.modified {
        keep.insert(m.idx);
    }

    let mut facts_stored = 0u32;
    let mut relations_created = 0u32;

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Api,
            source_id: "ingest-review".to_string(),
        };

        // Collect entity labels referenced by accepted relations
        let mut needed_entities: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (idx, rel) in session.relations.iter().enumerate() {
            if keep.contains(&idx) {
                needed_entities.insert(rel.from.to_lowercase());
                needed_entities.insert(rel.to.to_lowercase());
            }
        }

        // Store only entities referenced by accepted relations
        for ent in &session.entities {
            if !needed_entities.contains(&ent.label.to_lowercase()) {
                continue;
            }
            match g.store_with_confidence(&ent.label, ent.confidence, &prov) {
                Ok(_) => {
                    let _ = g.set_node_type(&ent.label, &ent.entity_type);
                    facts_stored += 1;
                }
                Err(_) => {}
            }
        }

        // Store only explicitly accepted relations
        for (idx, rel) in session.relations.iter().enumerate() {
            if !keep.contains(&idx) { continue; }

            let rel_type = modified_map.get(&idx).unwrap_or(&rel.rel_type);
            if g.find_node_id(&rel.from).ok().flatten().is_none() {
                let _ = g.store_with_confidence(&rel.from, 0.60, &prov);
                facts_stored += 1;
            }
            if g.find_node_id(&rel.to).ok().flatten().is_none() {
                let _ = g.store_with_confidence(&rel.to, 0.60, &prov);
                facts_stored += 1;
            }
            match g.relate(&rel.from, &rel.to, rel_type, &prov) {
                Ok(_) => relations_created += 1,
                Err(_) => {}
            }
        }
    }

    state.mark_dirty();

    // Clean up session
    state.ingest_sessions.write().unwrap().remove(&req.session_id);

    Ok(Json(IngestResponse {
        facts_stored,
        relations_created,
        relations_deduplicated: 0,
        facts_resolved: 0,
        facts_deduped: 0,
        conflicts_detected: 0,
        errors: Vec::new(),
        duration_ms: start.elapsed().as_millis() as u64,
        stages_skipped: Vec::new(),
        warnings: Vec::new(),
        kb_stats: None,
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest_review_confirm() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// GET /config/relation-types -- flat list of all known relation types.
pub async fn list_relation_types(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let cfg = state.config.read().map_err(|_| read_lock_err())?;
    let mut types: Vec<String> = Vec::new();

    // Configured templates
    if let Some(ref templates) = cfg.relation_templates {
        types.extend(templates.keys().cloned());
    } else {
        types.extend(["works_at", "headquartered_in", "located_in", "founded", "leads", "supports"]
            .iter().map(|s| s.to_string()));
    }
    drop(cfg);

    // Default type templates
    let defaults = engram_ingest::rel_type_templates::default_type_templates();
    for rels in defaults.values() {
        for r in rels {
            if !types.contains(r) {
                types.push(r.clone());
            }
        }
    }

    // Learned from graph's relation gazetteer
    if let Some(ref config_path) = state.config_path {
        let brain_path = config_path.with_extension("");
        let relgaz_path = brain_path.with_extension("relgaz");
        if relgaz_path.exists() {
            if let Ok(gaz) = engram_ingest::RelationGazetteer::load(&brain_path) {
                for rt in gaz.known_relation_types() {
                    if !types.contains(rt) {
                        types.push(rt.clone());
                    }
                }
            }
        }
    }

    types.sort();
    types.dedup();
    Ok(Json(serde_json::json!({ "types": types })))
}

fn event_type_name(event: &engram_core::events::GraphEvent) -> &'static str {
    use engram_core::events::GraphEvent;
    match event {
        GraphEvent::FactStored { .. } => "fact_stored",
        GraphEvent::FactUpdated { .. } => "fact_updated",
        GraphEvent::FactDeleted { .. } => "fact_deleted",
        GraphEvent::EdgeCreated { .. } => "edge_created",
        GraphEvent::PropertyChanged { .. } => "property_changed",
        GraphEvent::TierChanged { .. } => "tier_changed",
        GraphEvent::ThresholdCrossed { .. } => "threshold_crossed",
        GraphEvent::QueryGap { .. } => "query_gap",
        GraphEvent::TimerTick { .. } => "timer_tick",
        GraphEvent::ConflictDetected { .. } => "conflict_detected",
        GraphEvent::DecayApplied { .. } => "decay_applied",
        GraphEvent::TierSweepCompleted { .. } => "tier_sweep_completed",
        GraphEvent::EdgeDeleted { .. } => "edge_deleted",
        GraphEvent::SeedAoiDetected { .. } => "seed_aoi_detected",
        GraphEvent::SeedEntityLinked { .. } => "seed_entity_linked",
        GraphEvent::SeedEntityAmbiguous { .. } => "seed_entity_ambiguous",
        GraphEvent::SeedConnectionFound { .. } => "seed_connection_found",
        GraphEvent::SeedSparqlRelation { .. } => "seed_sparql_relation",
        GraphEvent::SeedPhaseComplete { .. } => "seed_phase_complete",
        GraphEvent::SeedComplete { .. } => "seed_complete",
    }
}
