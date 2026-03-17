use super::*;

// ── POST /store ──

pub async fn store(
    State(state): State<AppState>,
    Json(req): Json<StoreRequest>,
) -> ApiResult<StoreResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&req.source);

    let slot = if let Some(conf) = req.confidence {
        g.store_with_confidence(&req.entity, conf, &prov)
    } else {
        g.store(&req.entity, &prov)
    }
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref t) = req.entity_type {
        let _ = g.set_node_type(&req.entity, t);
    }

    if let Some(ref props) = req.properties {
        for (k, v) in props {
            let _ = g.set_property(&req.entity, k, v);
        }
    }

    let confidence = g
        .get_node(&req.entity)
        .ok()
        .flatten()
        .map(|n| n.confidence)
        .unwrap_or(0.0);

    drop(g); // Release write lock before marking dirty
    state.mark_dirty();
    state.fire_rules_async();

    Ok(Json(StoreResponse {
        node_id: slot,
        label: req.entity,
        confidence,
    }))
}

// ── POST /relate ──

pub async fn relate(
    State(state): State<AppState>,
    Json(req): Json<RelateRequest>,
) -> ApiResult<RelateResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);

    let has_temporal = req.valid_from.is_some() || req.valid_to.is_some();
    let edge_slot = if has_temporal || req.confidence.is_some() {
        g.relate_with_temporal(
            &req.from, &req.to, &req.relationship,
            req.confidence.unwrap_or(0.8),
            req.valid_from.as_deref(),
            req.valid_to.as_deref(),
            &prov,
        )
    } else {
        g.relate(&req.from, &req.to, &req.relationship, &prov)
    }
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(g);
    state.mark_dirty();
    state.fire_rules_async();

    Ok(Json(RelateResponse {
        from: req.from,
        to: req.to,
        relationship: req.relationship,
        edge_slot,
    }))
}

// ── POST /batch ──

pub async fn batch(
    State(state): State<AppState>,
    Json(req): Json<BatchRequest>,
) -> ApiResult<BatchResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&req.source);
    let mode = req.mode.unwrap_or_default();
    let strategy = req.confidence_strategy.unwrap_or_default();

    let mut nodes_stored: u32 = 0;
    let mut nodes_updated: u32 = 0;
    let mut edges_created: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    // Process entity stores
    if let Some(entities) = req.entities {
        for entity in entities {
            match store_entity(&mut g, &entity, &prov, mode, strategy) {
                Ok(StoreOutcome::Created) => nodes_stored += 1,
                Ok(StoreOutcome::Updated) => nodes_updated += 1,
                Ok(StoreOutcome::Unchanged) => nodes_stored += 1,
                Err(e) => errors.push(format!("store {}: {}", entity.entity, e)),
            }
        }
    }

    // Process relationships
    if let Some(relations) = req.relations {
        for rel in relations {
            let result = if let Some(conf) = rel.confidence {
                g.relate_with_confidence(&rel.from, &rel.to, &rel.relationship, conf, &prov)
            } else {
                g.relate(&rel.from, &rel.to, &rel.relationship, &prov)
            };
            match result {
                Ok(_) => edges_created += 1,
                Err(e) => errors.push(format!("relate {} -> {}: {}", rel.from, rel.to, e)),
            }
        }
    }

    drop(g);
    state.mark_dirty();
    state.fire_rules_async();

    Ok(Json(BatchResponse {
        nodes_stored,
        edges_created,
        nodes_updated,
        errors: if errors.is_empty() { None } else { Some(errors) },
    }))
}

/// Outcome of a single entity store operation.
enum StoreOutcome {
    Created,
    Updated,
    Unchanged,
}

/// Store a single entity with upsert support.
fn store_entity(
    g: &mut engram_core::Graph,
    entity: &StoreRequest,
    prov: &engram_core::graph::Provenance,
    mode: BatchMode,
    strategy: ConfidenceStrategy,
) -> std::result::Result<StoreOutcome, engram_core::StorageError> {
    // Check if entity already exists (for upsert logic)
    let existing = g.get_node(&entity.entity)?;

    match (existing, mode) {
        // Entity exists + upsert mode: update confidence
        (Some(node), BatchMode::Upsert) => {
            if let Some(incoming_conf) = entity.confidence {
                let old_conf = node.confidence;
                let new_conf = match strategy {
                    ConfidenceStrategy::Max => old_conf.max(incoming_conf),
                    ConfidenceStrategy::Replace => incoming_conf,
                    ConfidenceStrategy::Average => (old_conf + incoming_conf) / 2.0,
                };
                if (new_conf - old_conf).abs() > f32::EPSILON {
                    g.store_with_confidence(&entity.entity, new_conf, prov)?;
                    return Ok(StoreOutcome::Updated);
                }
            }
            // Set properties even on existing nodes
            if let Some(ref props) = entity.properties {
                for (k, v) in props {
                    let _ = g.set_property(&entity.entity, k, v);
                }
            }
            Ok(StoreOutcome::Unchanged)
        }
        // Entity exists + insert mode: dedup (existing behavior)
        (Some(_), BatchMode::Insert) => {
            if let Some(ref props) = entity.properties {
                for (k, v) in props {
                    let _ = g.set_property(&entity.entity, k, v);
                }
            }
            Ok(StoreOutcome::Unchanged)
        }
        // New entity: store normally
        (None, _) => {
            let _slot = if let Some(conf) = entity.confidence {
                g.store_with_confidence(&entity.entity, conf, prov)?
            } else {
                g.store(&entity.entity, prov)?
            };
            if let Some(ref t) = entity.entity_type {
                let _ = g.set_node_type(&entity.entity, t);
            }
            if let Some(ref props) = entity.properties {
                for (k, v) in props {
                    let _ = g.set_property(&entity.entity, k, v);
                }
            }
            Ok(StoreOutcome::Created)
        }
    }
}

// ── POST /batch/stream (NDJSON) ──

/// Streaming NDJSON batch endpoint.
/// Accepts newline-delimited JSON, processes each line independently.
/// Uses chunked write locking (default 1000 items per chunk) to keep
/// reads alive during large imports.
pub async fn batch_stream(
    State(state): State<AppState>,
    body: axum::body::Body,
) -> ApiResult<BatchResponse> {
    use axum::body::to_bytes;

    // Read body (axum doesn't have built-in line streaming, so we read
    // and split -- for truly huge payloads, a streaming line reader
    // would be better, but this handles millions of lines fine)
    let bytes = to_bytes(body, 256 * 1024 * 1024) // 256MB max
        .await
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("body read error: {e}")))?;

    let text = std::str::from_utf8(&bytes)
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("invalid UTF-8: {e}")))?;

    // Parse all lines first (fast, no lock needed)
    let mut items: Vec<BatchItem> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<BatchItem>(line) {
            Ok(item) => items.push(item),
            Err(e) => parse_errors.push(format!("line {}: {}", i + 1, e)),
        }
    }

    // Process in chunks with write lock per chunk
    const CHUNK_SIZE: usize = 1000;
    let mut nodes_stored: u32 = 0;
    let mut nodes_updated: u32 = 0;
    let mut edges_created: u32 = 0;
    let mut errors = parse_errors;

    for chunk in items.chunks(CHUNK_SIZE) {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;

        for item in chunk {
            match item {
                BatchItem::Entity {
                    entity,
                    entity_type,
                    properties,
                    confidence,
                    source,
                } => {
                    let prov = provenance(source);
                    let req = StoreRequest {
                        entity: entity.clone(),
                        entity_type: entity_type.clone(),
                        properties: properties.clone(),
                        source: source.clone(),
                        confidence: *confidence,
                    };
                    match store_entity(&mut g, &req, &prov, BatchMode::Upsert, ConfidenceStrategy::Max) {
                        Ok(StoreOutcome::Created) => nodes_stored += 1,
                        Ok(StoreOutcome::Updated) => nodes_updated += 1,
                        Ok(StoreOutcome::Unchanged) => nodes_stored += 1,
                        Err(e) => errors.push(format!("store {}: {}", entity, e)),
                    }
                }
                BatchItem::Relation {
                    from,
                    to,
                    relationship,
                    confidence,
                    source,
                } => {
                    let prov = provenance(source);
                    let result = if let Some(conf) = confidence {
                        g.relate_with_confidence(from, to, relationship, *conf, &prov)
                    } else {
                        g.relate(from, to, relationship, &prov)
                    };
                    match result {
                        Ok(_) => edges_created += 1,
                        Err(e) => errors.push(format!("relate {} -> {}: {}", from, to, e)),
                    }
                }
            }
        }

        drop(g);
        // Mark dirty after each chunk so checkpoint can run between chunks
        state.mark_dirty();
    }

    state.fire_rules_async();

    Ok(Json(BatchResponse {
        nodes_stored,
        edges_created,
        nodes_updated,
        errors: if errors.is_empty() { None } else { Some(errors) },
    }))
}

// ── POST /edge/delete ──

pub async fn delete_edge(
    State(state): State<AppState>,
    Json(req): Json<DeleteEdgeRequest>,
) -> ApiResult<DeleteEdgeResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = Provenance::user("api");

    let deleted = g
        .delete_edge(&req.from, &req.to, &req.rel_type, &prov)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;

    if deleted {
        drop(g);
        state.mark_dirty();
    } else {
        drop(g);
    }

    Ok(Json(DeleteEdgeResponse {
        deleted,
        from: req.from,
        to: req.to,
        rel_type: req.rel_type,
    }))
}

// ── PATCH /edge ──

pub async fn rename_edge(
    State(state): State<AppState>,
    Json(req): Json<RenameEdgeRequest>,
) -> ApiResult<RenameEdgeResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    let renamed = g
        .rename_edge(&req.from, &req.to, &req.old_rel_type, &req.new_rel_type)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;

    if renamed {
        let brain_path = g.path().to_path_buf();
        drop(g);
        state.mark_dirty();

        // Best-effort relation gazetteer update: teach future ingests the new type
        #[cfg(feature = "ingest")]
        {
            if let Ok(mut gaz) = engram_ingest::RelationGazetteer::load(&brain_path) {
                let normalized = req.new_rel_type.to_lowercase().replace(' ', "_").replace('-', "_");
                gaz.insert(engram_ingest::RelGazetteerEntry {
                    head: req.from.to_lowercase(),
                    tail: req.to.to_lowercase(),
                    rel_type: normalized,
                    confidence: 0.95,
                });
                let _ = gaz.save();
            }
        }
    } else {
        drop(g);
    }

    Ok(Json(RenameEdgeResponse {
        renamed,
        from: req.from,
        to: req.to,
        old_rel_type: req.old_rel_type,
        new_rel_type: req.new_rel_type,
    }))
}
