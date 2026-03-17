use super::*;
#[cfg(feature = "ingest")]
use super::ingest::build_pipeline;

// ── Seed enrichment endpoints ────────────────────────────────────────

/// POST /ingest/seed/start -- start interactive seed session: NER + AoI detection.
#[cfg(feature = "ingest")]
pub async fn seed_start(
    State(state): State<AppState>,
    Json(req): Json<SeedStartRequest>,
) -> ApiResult<SeedStartResponse> {
    use engram_ingest::PipelineConfig;

    let text = req.text.clone();
    let graph = state.graph.clone();
    let config_snap = state.config.read().unwrap().clone();
    let ner_cache = state.cached_ner.clone();
    let rel_cache = state.cached_rel.clone();

    // Run NER + AoI detection in spawn_blocking (blocking reqwest)
    let result = tokio::task::spawn_blocking(move || {
        let config = PipelineConfig::default();
        let kb_endpoints = config_snap.kb_endpoints.clone();
        let ner_model = config_snap.ner_model.clone();
        let rel_model = config_snap.rel_model.clone();
        let relation_templates = config_snap.relation_templates.clone();
        let rel_threshold = config_snap.rel_threshold;
        let coreference_enabled = config_snap.coreference_enabled;

        let pipeline = build_pipeline(
            graph.clone(), config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled,
            ner_cache, rel_cache,
        );

        // Run NER only (analyze)
        let items = vec![engram_ingest::types::RawItem {
            content: engram_ingest::types::Content::Text(text.clone()),
            source_url: None,
            source_name: "seed".into(),
            fetched_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            metadata: Default::default(),
        }];
        let analyze_result = pipeline.analyze(items);

        // Detect area of interest using LLM or heuristic
        let entity_labels: Vec<String> = analyze_result.as_ref()
            .map(|r| r.entities.iter().map(|e| e.text.clone()).collect())
            .unwrap_or_default();

        let kb_extractor = engram_ingest::KbRelationExtractor::with_config(
            Vec::new(),
            graph,
            config_snap.llm_endpoint.clone(),
            config_snap.llm_model.clone(),
            None,
            None,
            None,
            None,
        );
        let aoi = kb_extractor.detect_area_of_interest(&text, &entity_labels);

        (analyze_result, aoi)
    })
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (analyze_result, aoi) = result;
    let analyze = analyze_result
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create session
    let session_id = format!("seed-{:016x}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64);
    let entities: Vec<crate::state::SeedEntity> = {
        // Noise words to filter out
        const NOISE: &[&str] = &[
            "events", "developments", "things", "analyst",
            "stock analyst", "events or developments",
        ];

        // Collect raw entities
        let raw: Vec<crate::state::SeedEntity> = analyze.entities.iter().map(|e| {
            crate::state::SeedEntity {
                label: e.text.clone(),
                entity_type: e.entity_type.clone(),
                confidence: e.confidence,
            }
        }).collect();

        // Deduplicate by label (case-insensitive), keeping highest confidence
        let mut dedup_map: std::collections::HashMap<String, crate::state::SeedEntity> =
            std::collections::HashMap::new();
        for ent in raw {
            let key = ent.label.to_lowercase();
            match dedup_map.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut existing) => {
                    if ent.confidence > existing.get().confidence {
                        existing.insert(ent);
                    }
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(ent);
                }
            }
        }

        dedup_map.into_values()
            .filter(|e| {
                // Filter: label must be at least 3 characters
                if e.label.len() < 3 { return false; }

                // Filter: noise words (case-insensitive)
                let lower = e.label.to_lowercase();
                if NOISE.iter().any(|n| lower == *n) { return false; }

                // Filter: all-lowercase labels (proper nouns should have
                // at least one uppercase letter)
                if !e.label.chars().any(|c| c.is_uppercase()) {
                    return false;
                }

                true
            })
            .collect()
    };

    let session = crate::state::SeedSession {
        session_id: session_id.clone(),
        seed_text: req.text,
        area_of_interest: Some(aoi.clone()),
        entities: entities.clone(),
        entity_links: Vec::new(),
        connections: Vec::new(),
        confirmed: false,
    };

    state.seed_sessions.write().unwrap().insert(session_id.clone(), session);

    // Emit AoI event
    state.event_bus.publish(engram_core::events::GraphEvent::SeedAoiDetected {
        session_id: std::sync::Arc::from(session_id.as_str()),
        area_of_interest: std::sync::Arc::from(aoi.as_str()),
    });

    Ok(Json(SeedStartResponse {
        session_id,
        entities: entities.iter().map(|e| SeedEntityResponse {
            label: e.label.clone(),
            entity_type: e.entity_type.clone(),
            confidence: e.confidence,
        }).collect(),
        area_of_interest: Some(aoi),
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_start() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /ingest/seed/confirm-aoi -- confirm area of interest, trigger entity linking (Step 1).
#[cfg(feature = "ingest")]
pub async fn seed_confirm_aoi(
    State(state): State<AppState>,
    Json(req): Json<SeedConfirmAoiRequest>,
) -> ApiResult<serde_json::Value> {
    // Update session with confirmed AoI
    {
        let mut sessions = state.seed_sessions.write().unwrap();
        let session = sessions.get_mut(&req.session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "session not found"))?;
        session.area_of_interest = Some(req.area_of_interest.clone());
    }

    let session_id = req.session_id.clone();
    let aoi = req.area_of_interest.clone();
    let graph = state.graph.clone();
    let event_bus = state.event_bus.clone();
    let config_snap = state.config.read().unwrap().clone();

    // Get entities from session
    let entities = {
        let sessions = state.seed_sessions.read().unwrap();
        let session = sessions.get(&session_id).unwrap();
        session.entities.clone()
    };

    // Run entity linking + AoI article co-occurrence in background
    let sid = session_id.clone();
    let sessions_arc = state.seed_sessions.clone();
    tokio::task::spawn_blocking(move || {
        use engram_ingest::RelationExtractor;

        let kb_endpoints: Vec<engram_ingest::KbEndpoint> = config_snap.kb_endpoints
            .unwrap_or_default()
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

        let mut extractor = engram_ingest::KbRelationExtractor::with_config(
            kb_endpoints,
            graph.clone(),
            config_snap.llm_endpoint,
            config_snap.llm_model,
            Some(event_bus),
            config_snap.web_search_provider,
            config_snap.web_search_api_key,
            config_snap.web_search_url,
        );

        // Wire GLiNER2 for co-occurrence pair classification
        #[cfg(feature = "gliner2")]
        {
            use engram_ingest::gliner2_backend::{Gliner2Backend, find_gliner2_model};
            if let Some(cfg) = find_gliner2_model() {
                if let Ok(backend) = Gliner2Backend::load(&cfg.model_dir, "fp16") {
                    let relation_types: Vec<String> = config_snap.relation_templates
                        .as_ref()
                        .map(|t| t.keys().cloned().collect())
                        .unwrap_or_else(|| vec![
                            "works_at".into(), "headquartered_in".into(),
                            "located_in".into(), "founded".into(),
                            "leads".into(), "supports".into(),
                        ]);
                    let threshold = config_snap.rel_threshold.unwrap_or(0.85);
                    let pb = engram_ingest::gliner2_backend::Gliner2PipelineBackend::new(
                        backend,
                        vec!["person".into(), "organization".into(), "location".into(),
                             "date".into(), "event".into(), "product".into()],
                        relation_types,
                        0.5,
                        threshold,
                    );
                    let re_arc: std::sync::Arc<dyn engram_ingest::RelationExtractor> = std::sync::Arc::new(pb);
                    extractor.set_gliner2_backend(re_arc);
                    tracing::info!("Seed KB extractor: GLiNER2 wired for co-occurrence classification");
                }
            }
        }

        // Build RelationExtractionInput from session entities
        let extracted: Vec<engram_ingest::ExtractedEntity> = entities.iter().map(|e| {
            engram_ingest::ExtractedEntity {
                text: e.label.clone(),
                entity_type: e.entity_type.clone(),
                span: (0, 0),
                confidence: e.confidence,
                method: engram_ingest::ExtractionMethod::Gazetteer,
                language: "en".into(),
                resolved_to: None,
            }
        }).collect();

        let input = engram_ingest::RelationExtractionInput {
            text: String::new(),
            entities: extracted,
            language: "en".into(),
            area_of_interest: Some(aoi),
        };

        let relations = extractor.extract_relations(&input);

        // Store connections in session
        if let Ok(mut sessions) = sessions_arc.write() {
            if let Some(session) = sessions.get_mut(&sid) {
                for rel in &relations {
                    if rel.head_idx < session.entities.len() && rel.tail_idx < session.entities.len() {
                        let is_sparql = rel.confidence >= 0.70 && rel.rel_type != "related_to";
                        session.connections.push(crate::state::SeedConnection {
                            from: session.entities[rel.head_idx].label.clone(),
                            to: session.entities[rel.tail_idx].label.clone(),
                            rel_type: rel.rel_type.clone(),
                            source: format!("{:?}", rel.method),
                            confidence: rel.confidence,
                            tier: crate::state::ConnectionTier::from_confidence(rel.confidence, is_sparql),
                        });
                    }
                }

                // Entity links are populated via SSE events during extraction
            }
        }
    });

    Ok(Json(serde_json::json!({
        "status": "enrichment_started",
        "session_id": session_id,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_confirm_aoi() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /ingest/seed/confirm-entities -- confirm entity matches, trigger connections.
#[cfg(feature = "ingest")]
pub async fn seed_confirm_entities(
    State(state): State<AppState>,
    Json(req): Json<SeedConfirmEntitiesRequest>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.seed_sessions.write().unwrap();
    let session = sessions.get_mut(&req.session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "session not found"))?;

    // Update entity links from user confirmation
    session.entity_links = req.entities.iter()
        .filter(|e| !e.skip)
        .map(|e| crate::state::SeedEntityLink {
            label: e.label.clone(),
            canonical: e.canonical.clone().unwrap_or_else(|| e.label.clone()),
            description: String::new(),
            qid: e.qid.clone().unwrap_or_default(),
        })
        .collect();

    Ok(Json(serde_json::json!({
        "status": "entities_confirmed",
        "session_id": req.session_id,
        "confirmed_count": session.entity_links.len(),
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_confirm_entities() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /ingest/seed/commit -- write confirmed entities + relations to graph.
#[cfg(feature = "ingest")]
pub async fn seed_commit(
    State(state): State<AppState>,
    Json(req): Json<SeedCommitRequest>,
) -> ApiResult<SeedCommitResponse> {
    let start = std::time::Instant::now();

    let session = {
        let sessions = state.seed_sessions.read().unwrap();
        sessions.get(&req.session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "session not found"))?
            .clone()
    };

    let mut facts_stored = 0u32;
    let mut relations_created = 0u32;

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Api,
            source_id: "seed-enrichment".to_string(),
        };

        // Store all entities
        for entity in &session.entities {
            match g.store_with_confidence(&entity.label, entity.confidence, &prov) {
                Ok(_) => {
                    let _ = g.set_node_type(&entity.label, &entity.entity_type);
                    facts_stored += 1;
                }
                Err(_) => {}
            }
        }

        // Store canonical names as properties
        for link in &session.entity_links {
            if link.canonical != link.label {
                let _ = g.set_property(&link.label, "canonical_name", &link.canonical);
            }
            if !link.qid.is_empty() {
                let _ = g.set_property(&link.label, "wikidata_qid", &link.qid);
            }
        }

        // Create all edges
        for conn in &session.connections {
            // Auto-create nodes if they don't exist
            if g.find_node_id(&conn.from).ok().flatten().is_none() {
                let _ = g.store_with_confidence(&conn.from, 0.60, &prov);
            }
            if g.find_node_id(&conn.to).ok().flatten().is_none() {
                let _ = g.store_with_confidence(&conn.to, 0.60, &prov);
            }

            match g.relate(&conn.from, &conn.to, &conn.rel_type, &prov) {
                Ok(_) => relations_created += 1,
                Err(_) => {}
            }
        }
    }

    state.mark_dirty();

    // Emit completion event
    state.event_bus.publish(engram_core::events::GraphEvent::SeedComplete {
        session_id: std::sync::Arc::from(req.session_id.as_str()),
        facts_stored,
        relations_created,
    });

    // Clean up session
    state.seed_sessions.write().unwrap().remove(&req.session_id);

    Ok(Json(SeedCommitResponse {
        facts_stored,
        relations_created,
        duration_ms: start.elapsed().as_millis() as u64,
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_commit() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// GET /ingest/seed/connections?session_id=xxx -- get tiered connections for review.
#[cfg(feature = "ingest")]
pub async fn seed_connections(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let session_id = query.get("session_id")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "session_id required"))?;

    let sessions = state.seed_sessions.read().unwrap();
    let session = sessions.get(session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "session not found"))?;

    // Group connections by tier
    let mut confirmed = Vec::new();
    let mut likely = Vec::new();
    let mut uncertain = Vec::new();
    let mut no_relation = Vec::new();

    for (idx, conn) in session.connections.iter().enumerate() {
        let entry = serde_json::json!({
            "idx": idx,
            "from": conn.from,
            "to": conn.to,
            "rel_type": conn.rel_type,
            "confidence": conn.confidence,
            "source": conn.source,
            "tier": conn.tier,
        });
        match conn.tier {
            crate::state::ConnectionTier::Confirmed => confirmed.push(entry),
            crate::state::ConnectionTier::Likely => likely.push(entry),
            crate::state::ConnectionTier::Uncertain => uncertain.push(entry),
            crate::state::ConnectionTier::NoRelation => no_relation.push(entry),
        }
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "groups": [
            { "tier": "confirmed", "label": "Confirmed (SPARQL + high-confidence GLiNER2)", "connections": confirmed },
            { "tier": "likely", "label": "Likely (GLiNER2 50-70%)", "connections": likely },
            { "tier": "uncertain", "label": "Uncertain (GLiNER2 < 50%)", "connections": uncertain },
            { "tier": "no_relation", "label": "Co-occurred but unclassified", "connections": no_relation },
        ]
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_connections() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// POST /ingest/seed/confirm-relations -- review and confirm relations before commit.
#[cfg(feature = "ingest")]
pub async fn seed_confirm_relations(
    State(state): State<AppState>,
    Json(req): Json<SeedConfirmRelationsRequest>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.seed_sessions.write().unwrap();
    let session = sessions.get_mut(&req.session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "session not found"))?;

    // Apply modifications
    for m in &req.modified {
        if m.idx < session.connections.len() {
            session.connections[m.idx].rel_type = m.new_rel_type.clone();
            session.connections[m.idx].tier = crate::state::ConnectionTier::Confirmed;
        }
    }

    // Build set of accepted + modified indices
    let mut keep: std::collections::HashSet<usize> = req.accepted.iter().copied().collect();
    for m in &req.modified {
        keep.insert(m.idx);
    }
    // Also keep all Confirmed tier that weren't explicitly skipped
    let skipped: std::collections::HashSet<usize> = req.skipped.iter().copied().collect();
    for (idx, conn) in session.connections.iter().enumerate() {
        if conn.tier == crate::state::ConnectionTier::Confirmed && !skipped.contains(&idx) {
            keep.insert(idx);
        }
    }

    // Filter connections to only kept ones
    let filtered: Vec<crate::state::SeedConnection> = session.connections.iter().enumerate()
        .filter(|(idx, _)| keep.contains(idx))
        .map(|(_, c)| c.clone())
        .collect();

    let count = filtered.len();
    session.connections = filtered;

    Ok(Json(serde_json::json!({
        "status": "relations_confirmed",
        "session_id": req.session_id,
        "accepted_count": count,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn seed_confirm_relations() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

/// GET /ingest/seed/stream?session_id=xxx -- SSE stream filtered to a seed session.
pub async fn seed_stream(
    State(state): State<AppState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt;

    let session_id = query.get("session_id").cloned().unwrap_or_default();

    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(move |result| {
            match result {
                Ok(event) => {
                    // Filter to seed events for this session
                    let (event_type, data) = match &event {
                        engram_core::events::GraphEvent::SeedAoiDetected { session_id: sid, area_of_interest } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_aoi_detected", serde_json::json!({
                                "detected": area_of_interest.as_ref()
                            }))
                        }
                        engram_core::events::GraphEvent::SeedEntityLinked { session_id: sid, label, canonical, description, qid } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_entity_linked", serde_json::json!({
                                "label": label.as_ref(),
                                "canonical": canonical.as_ref(),
                                "description": description.as_ref(),
                                "qid": qid.as_ref()
                            }))
                        }
                        engram_core::events::GraphEvent::SeedEntityAmbiguous { session_id: sid, label, candidates } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_entity_ambiguous", serde_json::json!({
                                "label": label.as_ref(),
                                "candidates": candidates.iter().map(|(c, d, q)| {
                                    serde_json::json!({"canonical": c.as_ref(), "description": d.as_ref(), "qid": q.as_ref()})
                                }).collect::<Vec<_>>()
                            }))
                        }
                        engram_core::events::GraphEvent::SeedConnectionFound { session_id: sid, from, to, rel_type, source } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_connection_found", serde_json::json!({
                                "from": from.as_ref(), "to": to.as_ref(),
                                "rel_type": rel_type.as_ref(), "source": source.as_ref()
                            }))
                        }
                        engram_core::events::GraphEvent::SeedSparqlRelation { session_id: sid, from, to, rel_type } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_sparql_relation", serde_json::json!({
                                "from": from.as_ref(), "to": to.as_ref(), "rel_type": rel_type.as_ref()
                            }))
                        }
                        engram_core::events::GraphEvent::SeedPhaseComplete { session_id: sid, phase, entities_processed, relations_found } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_phase_complete", serde_json::json!({
                                "phase": phase, "entities_processed": entities_processed,
                                "relations_found": relations_found
                            }))
                        }
                        engram_core::events::GraphEvent::SeedComplete { session_id: sid, facts_stored, relations_created } => {
                            if sid.as_ref() != session_id { return None; }
                            ("seed_complete", serde_json::json!({
                                "facts_stored": facts_stored, "relations_created": relations_created
                            }))
                        }
                        _ => return None, // non-seed events
                    };

                    Some(Ok(axum::response::sse::Event::default()
                        .event(event_type)
                        .data(data.to_string())))
                }
                Err(_) => None,
            }
        });

    axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}
