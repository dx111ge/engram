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

/// Normalize an entity name for fuzzy matching: lowercase, replace hyphens/underscores with spaces, trim.
fn normalize_entity_name(name: &str) -> String {
    name.to_lowercase().replace('-', " ").replace('_', " ").trim().to_string()
}

/// Resolve an entity name: try exact match first, then case-insensitive normalized match.
/// Returns the actual label from the graph if a fuzzy match is found, otherwise returns the input unchanged.
fn resolve_entity_label(g: &engram_core::Graph, name: &str) -> String {
    // Exact match -- fast path
    if g.get_node(name).ok().flatten().is_some() {
        return name.to_string();
    }
    // Normalized fuzzy match -- scan all nodes
    let normalized_input = normalize_entity_name(name);
    if let Ok(nodes) = g.all_nodes() {
        for node in &nodes {
            if normalize_entity_name(&node.label) == normalized_input {
                return node.label.clone();
            }
        }
    }
    // No match found, return original (will create new node on relate)
    name.to_string()
}

pub async fn relate(
    State(state): State<AppState>,
    Json(req): Json<RelateRequest>,
) -> ApiResult<RelateResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);

    // Resolve entity names: exact match first, then normalized fuzzy match
    let resolved_from = resolve_entity_label(&g, &req.from);
    let resolved_to = resolve_entity_label(&g, &req.to);

    let has_temporal = req.valid_from.is_some() || req.valid_to.is_some();
    let edge_slot = if has_temporal || req.confidence.is_some() {
        g.relate_with_temporal(
            &resolved_from, &resolved_to, &req.relationship,
            req.confidence.unwrap_or(0.8),
            req.valid_from.as_deref(),
            req.valid_to.as_deref(),
            &prov,
        )
    } else {
        g.relate(&resolved_from, &resolved_to, &req.relationship, &prov)
    }
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(g);
    state.mark_dirty();
    state.fire_rules_async();

    Ok(Json(RelateResponse {
        from: resolved_from,
        to: resolved_to,
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

    let is_rename = req.old_rel_type != req.new_rel_type;
    let has_temporal = req.valid_from.is_some() || req.valid_to.is_some();

    let renamed = if is_rename {
        g.rename_edge(&req.from, &req.to, &req.old_rel_type, &req.new_rel_type)
            .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?
    } else {
        true // no rename needed, but may still update temporal
    };

    // Update temporal dates if provided (use new_rel_type for lookup after rename)
    let mut temporal_updated = false;
    if has_temporal && renamed {
        let rel_type_for_lookup = &req.new_rel_type;
        let vf = req.valid_from.as_deref();
        let vt = req.valid_to.as_deref();
        temporal_updated = g
            .update_edge_temporal(&req.from, &req.to, rel_type_for_lookup, vf, vt)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let changed = (is_rename && renamed) || temporal_updated;

    if changed {
        let brain_path = g.path().to_path_buf();
        drop(g);
        state.mark_dirty();

        // Best-effort relation gazetteer update: teach future ingests the new type
        #[cfg(feature = "ingest")]
        {
            if is_rename {
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
        }
    } else {
        drop(g);
    }

    Ok(Json(RenameEdgeResponse {
        renamed: changed,
        from: req.from,
        to: req.to,
        old_rel_type: req.old_rel_type,
        new_rel_type: req.new_rel_type,
        valid_from: req.valid_from,
        valid_to: req.valid_to,
    }))
}

// ── PATCH /node ── Update node type, properties, confidence

#[derive(serde::Deserialize)]
pub struct PatchNodeRequest {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Properties to set (key-value). Existing properties not in this map are kept.
    #[serde(default)]
    pub properties: std::collections::HashMap<String, String>,
}

#[derive(serde::Serialize)]
pub struct PatchNodeResponse {
    pub label: String,
    pub updated: bool,
}

pub async fn patch_node(
    State(state): State<AppState>,
    Json(req): Json<PatchNodeRequest>,
) -> ApiResult<PatchNodeResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    // Verify node exists
    if g.find_node_id(&req.label).map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_none() {
        return Err(api_err(StatusCode::NOT_FOUND, format!("node not found: {}", req.label)));
    }

    let mut changed = false;

    if let Some(ref nt) = req.node_type {
        if let Ok(_) = g.set_node_type(&req.label, nt) {
            changed = true;
        }
    }

    if let Some(conf) = req.confidence {
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::User,
            source_id: "manual_edit".to_string(),
        };
        if g.store_with_confidence(&req.label, conf, &prov).is_ok() {
            changed = true;
        }
    }

    for (key, value) in &req.properties {
        let _ = g.set_property(&req.label, key, value);
        changed = true;
    }

    if changed {
        drop(g);
        state.mark_dirty();
    }

    Ok(Json(PatchNodeResponse {
        label: req.label,
        updated: changed,
    }))
}

// ── GET /config/node-types ── List known node types in the graph

pub async fn node_types(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let g = state.graph.read().unwrap();
    let types = g.all_node_types();
    Json(serde_json::json!({ "types": types }))
}
