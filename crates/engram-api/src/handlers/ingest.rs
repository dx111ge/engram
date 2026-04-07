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

    // Register PDF parser (if pdf feature enabled)
    #[cfg(feature = "pdf")]
    pipeline.add_parser(Box::new(engram_ingest::pdf::PdfParser));

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

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled, llm_endpoint, llm_model) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled,
         c.llm_endpoint.clone(), c.llm_model.clone())
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
            if let (Some(ep), Some(m)) = (llm_endpoint.as_ref(), llm_model.as_ref()) {
                pipeline.set_llm(ep.clone(), m.clone());
            }
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
    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled, llm_endpoint, llm_model) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled,
         c.llm_endpoint.clone(), c.llm_model.clone())
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

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled, llm_endpoint, llm_model) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled,
         c.llm_endpoint.clone(), c.llm_model.clone())
    };
    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Detect PDF: check ?mime= param or %PDF- magic bytes
    let mime_hint = params.get("mime").cloned().unwrap_or_default();
    let is_pdf = mime_hint.contains("pdf")
        || (body.len() >= 5 && &body[..5] == b"%PDF-");

    let items = if is_pdf {
        let mut metadata = std::collections::HashMap::new();
        if let Some(title) = params.get("title") {
            metadata.insert("title".into(), title.clone());
        }
        vec![engram_ingest::types::RawItem {
            content: engram_ingest::types::Content::Bytes {
                data: body.to_vec(),
                mime: "application/pdf".into(),
            },
            source_url: params.get("url").cloned(),
            source_name: source,
            fetched_at: now,
            metadata,
        }]
    } else {
        // Parse as UTF-8 text (existing behavior)
        let text = String::from_utf8(body.to_vec())
            .map_err(|_| api_err(StatusCode::BAD_REQUEST,
                "file body is not valid UTF-8 (hint: pass ?mime=application/pdf for PDFs)"))?;
        vec![engram_ingest::types::RawItem {
            content: engram_ingest::types::Content::Text(text),
            source_url: params.get("url").cloned(),
            source_name: source,
            fetched_at: now,
            metadata: Default::default(),
        }]
    };

    // Run build_pipeline + execute in spawn_blocking to avoid tokio runtime panic
    let parallel = params.get("parallel").is_some_and(|v| v == "true" || v == "1");
    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
        pipeline.set_doc_store(doc_store.clone());
        // Wire LLM for translation + fact extraction
        if let (Some(ep), Some(m)) = (llm_endpoint.as_ref(), llm_model.as_ref()) {
            pipeline.set_llm(ep.clone(), m.clone());
        }
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

// ── Reprocess existing documents ─────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ReprocessRequest {
    /// Max documents to process in this batch (default 10).
    #[serde(default)]
    pub batch_size: Option<usize>,
    /// Re-process even docs marked ner_complete=true.
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct ReprocessResponse {
    pub documents_found: usize,
    pub status: String,
    pub message: String,
}

/// POST /ingest/reprocess-docs -- run full NER/RE on existing Document nodes.
/// Returns immediately. Processing runs in background with SSE progress events.
/// Subscribe to `/events/stream?topics=ingest_progress` for updates.
#[cfg(feature = "ingest")]
pub async fn reprocess_docs(
    State(state): State<AppState>,
    Json(req): Json<ReprocessRequest>,
) -> ApiResult<ReprocessResponse> {
    let batch_size = req.batch_size.unwrap_or(10);
    let force = req.force.unwrap_or(false);

    // Find Document nodes that need processing
    let docs_to_process: Vec<(String, std::collections::HashMap<String, String>)> = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        let all_nodes = g.all_nodes()
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        all_nodes.iter()
            .filter(|n| {
                let nt = g.get_node_type(&n.label).unwrap_or_default();
                nt == "Document"
            })
            .filter(|n| {
                if force { return true; }
                let props = g.get_properties(&n.label)
                    .unwrap_or_default().unwrap_or_default();
                props.get("ner_complete").map(|v| v != "true").unwrap_or(true)
            })
            .take(batch_size)
            .filter_map(|n| {
                let props = g.get_properties(&n.label)
                    .unwrap_or_default().unwrap_or_default();
                Some((n.label.clone(), props))
            })
            .collect()
    };

    let docs_found = docs_to_process.len();
    if docs_found == 0 {
        return Ok(Json(ReprocessResponse {
            documents_found: 0,
            status: "complete".into(),
            message: "no documents need processing".into(),
        }));
    }

    // Spawn background task for actual processing
    let bg_state = state.clone();
    tokio::spawn(async move {
        reprocess_docs_background(bg_state, docs_to_process).await;
    });

    Ok(Json(ReprocessResponse {
        documents_found: docs_found,
        status: "started".into(),
        message: format!("processing {} documents in background -- subscribe to /events/stream?topics=ingest_progress", docs_found),
    }))
}

/// Background worker for document reprocessing.
/// Emits IngestProgress events via the event bus.
#[cfg(feature = "ingest")]
async fn reprocess_docs_background(
    state: AppState,
    docs_to_process: Vec<(String, std::collections::HashMap<String, String>)>,
) {
    use engram_core::events::GraphEvent;

    let total = docs_to_process.len() as u32;

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold,
         coreference_enabled, llm_endpoint, llm_model) = {
        let c = state.config.read().unwrap();
        (c.kb_endpoints.clone(), c.ner_model.clone(), c.rel_model.clone(),
         c.relation_templates.clone(), c.rel_threshold, c.coreference_enabled,
         c.llm_endpoint.clone(), c.llm_model.clone())
    };

    let http_client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("engram/1.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("reprocess: HTTP client failed: {e}");
            state.event_bus.publish(GraphEvent::IngestProgress {
                operation: "reprocess".into(),
                document: "".into(),
                processed: 0, total,
                stage: format!("error: HTTP client: {e}").into(),
            });
            return;
        }
    };

    let mut items: Vec<engram_ingest::RawItem> = Vec::new();
    let mut processed_count = 0u32;

    for (doc_label, props) in &docs_to_process {
        processed_count += 1;
        let url = props.get("url");
        let title = props.get("title").cloned();
        let doc_date = props.get("doc_date").cloned();
        let content_hash = props.get("content_hash");

        state.event_bus.publish(GraphEvent::IngestProgress {
            operation: "reprocess".into(),
            document: doc_label.as_str().into(),
            processed: processed_count, total,
            stage: "fetching".into(),
        });

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Strategy 1: Re-fetch from URL
        if let Some(u) = url {
            match http_client.get(u).send().await {
                Ok(resp) => {
                    let ct = resp.headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    let is_pdf = u.to_lowercase().ends_with(".pdf") || ct.contains("pdf");

                    if let Ok(bytes) = resp.bytes().await {
                        let content = if is_pdf {
                            engram_ingest::Content::Bytes {
                                data: bytes.to_vec(),
                                mime: "application/pdf".into(),
                            }
                        } else {
                            let text = String::from_utf8(bytes.to_vec())
                                .unwrap_or_else(|_| "[binary content]".to_string());
                            let text = if ct.contains("html") {
                                dom_smoothie::Readability::new(text.clone(), None, None)
                                    .ok()
                                    .and_then(|mut r| r.parse().ok())
                                    .map(|a| a.text_content.to_string())
                                    .unwrap_or(text)
                            } else {
                                text
                            };
                            engram_ingest::Content::Text(text)
                        };

                        let mut metadata = std::collections::HashMap::new();
                        if let Some(t) = title.clone() { metadata.insert("title".into(), t); }
                        if let Some(d) = doc_date.clone() { metadata.insert("doc_date".into(), d); }

                        items.push(engram_ingest::RawItem {
                            content,
                            source_url: Some(u.clone()),
                            source_name: format!("reprocess:{doc_label}"),
                            fetched_at: now,
                            metadata,
                        });
                        continue;
                    }
                }
                Err(e) => {
                    tracing::warn!(doc = %doc_label, error = %e, "reprocess: fetch failed");
                }
            }
        }

        // Strategy 2: Load from DocStore (fallback)
        if let Some(hash_hex) = content_hash {
            let hash_bytes: Vec<u8> = (0..hash_hex.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&hash_hex[i..i+2], 16).ok())
                .collect();

            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);

                if let Ok(store) = state.doc_store.read() {
                    if let Ok((content_bytes, _mime)) = store.load(&hash) {
                        if let Ok(text) = String::from_utf8(content_bytes) {
                            let mut metadata = std::collections::HashMap::new();
                            if let Some(t) = title { metadata.insert("title".into(), t); }
                            if let Some(d) = doc_date { metadata.insert("doc_date".into(), d); }

                            items.push(engram_ingest::RawItem {
                                content: engram_ingest::Content::Text(text),
                                source_url: url.cloned(),
                                source_name: format!("reprocess:{doc_label}"),
                                fetched_at: now,
                                metadata,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        tracing::warn!(doc = %doc_label, "reprocess: no URL and no cached content");
    }

    if items.is_empty() {
        state.event_bus.publish(GraphEvent::IngestProgress {
            operation: "reprocess".into(),
            document: "".into(),
            processed: total, total,
            stage: "complete (no fetchable content)".into(),
        });
        return;
    }

    // Run pipeline
    state.event_bus.publish(GraphEvent::IngestProgress {
        operation: "reprocess".into(),
        document: "".into(),
        processed: total, total,
        stage: "running NER/RE pipeline".into(),
    });

    let graph = state.graph.clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();
    let doc_store = state.doc_store.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut pipeline = build_pipeline(graph, engram_ingest::PipelineConfig {
            name: "reprocess-docs".into(),
            ..Default::default()
        }, kb_endpoints, ner_model, rel_model,
           relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
        pipeline.set_doc_store(doc_store);
        if let (Some(ep), Some(m)) = (llm_endpoint.as_ref(), llm_model.as_ref()) {
            pipeline.set_llm(ep.clone(), m.clone());
        }
        pipeline.execute(items)
    }).await;

    match result {
        Ok(Ok(r)) => {
            // Mark original documents as ner_complete.
            // The pipeline may have created new Doc nodes with different hashes
            // (if content changed). Mark the originals anyway so they aren't
            // re-queued. The new Doc nodes are already marked by the pipeline.
            if let Ok(mut g) = state.graph.write() {
                for (doc_label, _) in &docs_to_process {
                    let _ = g.set_property(doc_label, "ner_complete", "true");
                }
            }
            state.mark_dirty();

            state.event_bus.publish(GraphEvent::IngestProgress {
                operation: "reprocess".into(),
                document: "".into(),
                processed: total, total,
                stage: format!("complete: {} facts, {} relations", r.facts_stored, r.relations_created).into(),
            });
            tracing::info!(
                facts = r.facts_stored, relations = r.relations_created,
                duration_ms = r.duration_ms, "reprocess-docs complete"
            );
        }
        Ok(Err(e)) => {
            state.event_bus.publish(GraphEvent::IngestProgress {
                operation: "reprocess".into(),
                document: "".into(),
                processed: total, total,
                stage: format!("error: {e}").into(),
            });
            tracing::error!("reprocess-docs pipeline error: {e}");
        }
        Err(e) => {
            state.event_bus.publish(GraphEvent::IngestProgress {
                operation: "reprocess".into(),
                document: "".into(),
                processed: total, total,
                stage: format!("error: task join: {e}").into(),
            });
            tracing::error!("reprocess-docs task join error: {e}");
        }
    }
}

#[cfg(not(feature = "ingest"))]
pub async fn reprocess_docs() -> impl axum::response::IntoResponse {
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
        GraphEvent::SeedArticleProgress { .. } => "seed_article_progress",
        GraphEvent::SeedFactProgress { .. } => "seed_fact_progress",
        GraphEvent::SeedComplete { .. } => "seed_complete",
        GraphEvent::SeedProgress { .. } => "seed_progress",
        GraphEvent::IngestProgress { .. } => "ingest_progress",
    }
}
