/// HTTP handlers for the REST API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use engram_core::Embedder;
use engram_core::graph::Provenance;

use crate::natural;
use crate::state::{AppState, EngineConfig};
use crate::types::*;

type ApiResult<T> = std::result::Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg.into() }))
}

fn read_lock_err() -> (StatusCode, Json<ErrorResponse>) {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph read lock poisoned")
}

fn write_lock_err() -> (StatusCode, Json<ErrorResponse>) {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph write lock poisoned")
}

fn provenance(source: &Option<String>) -> Provenance {
    match source {
        Some(s) => Provenance::user(s),
        None => Provenance::user("api"),
    }
}

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

    let edge_slot = if let Some(conf) = req.confidence {
        g.relate_with_confidence(&req.from, &req.to, &req.relationship, conf, &prov)
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

// ── POST /query ──

pub async fn query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> ApiResult<QueryResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let depth = req.depth.unwrap_or(2);
    let min_conf = req.min_confidence.unwrap_or(0.0);
    let direction = req.direction.as_deref().unwrap_or("both");

    let result = g
        .traverse_directed(&req.start, depth, min_conf, direction)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;

    let type_filter = req.node_type.as_deref();
    let mut nodes = Vec::new();
    for &nid in &result.nodes {
        if let Ok(Some(node)) = g.get_node_by_id(nid) {
            let label = g.label_for_id(nid).unwrap_or_else(|_| node.label().to_string());
            let node_type = g.get_node_type(&label);
            // Apply node_type filter if specified
            if let Some(filter) = type_filter {
                let nt = node_type.as_deref().unwrap_or("Entity");
                if !nt.eq_ignore_ascii_case(filter) {
                    continue;
                }
            }
            nodes.push(NodeHit {
                node_id: nid,
                label,
                confidence: node.confidence,
                node_type,
                score: None,
                depth: result.depths.get(&nid).copied(),
            });
        }
    }

    let edges = result
        .edges
        .iter()
        .filter_map(|&(_from_id, _to_id, edge_slot)| {
            let ev = g.read_edge_view(edge_slot).ok()?;
            Some(EdgeResponse {
                from: ev.from,
                to: ev.to,
                relationship: ev.relationship,
                confidence: ev.confidence,
                valid_from: None,
                valid_to: None,
            })
        })
        .collect();

    Ok(Json(QueryResponse { nodes, edges }))
}

// ── POST /similar ──

pub async fn similar(
    State(state): State<AppState>,
    Json(req): Json<SimilarRequest>,
) -> ApiResult<SearchResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(10);

    let results = g
        .search_hybrid_text(&req.text, limit)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = results.len();
    let hits: Vec<NodeHit> = results
        .into_iter()
        .map(|r| {
            let node_type = g.get_node_type(&r.label);
            NodeHit {
                node_id: r.node_id,
                label: r.label,
                confidence: r.confidence,
                node_type,
                score: Some(r.score),
                depth: None,
            }
        })
        .collect();

    Ok(Json(SearchResponse {
        results: hits,
        total,
    }))
}

// ── POST /search ──

pub async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> ApiResult<SearchResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(10);

    let results = g
        .search(&req.query, limit)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = results.len();
    let hits: Vec<NodeHit> = results
        .into_iter()
        .map(|r| {
            let node_type = g.get_node_type(&r.label);
            NodeHit {
                node_id: r.node_id,
                label: r.label,
                confidence: r.confidence,
                node_type,
                score: Some(r.score),
                depth: None,
            }
        })
        .collect();

    Ok(Json(SearchResponse {
        results: hits,
        total,
    }))
}

// ── GET /node/{label} ──

pub async fn get_node(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<NodeResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let node = g
        .get_node(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("node not found: {label}")))?;

    let node_id = node.id;
    let confidence = node.confidence;

    let properties = g
        .get_properties(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or_default();

    let edges_from: Vec<EdgeResponse> = g
        .edges_from(&label)
        .unwrap_or_default()
        .into_iter()
        .map(|e| EdgeResponse {
            from: e.from,
            to: e.to,
            relationship: e.relationship,
            confidence: e.confidence,
            valid_from: None,
            valid_to: None,
        })
        .collect();

    let edges_to: Vec<EdgeResponse> = g
        .edges_to(&label)
        .unwrap_or_default()
        .into_iter()
        .map(|e| EdgeResponse {
            from: e.from,
            to: e.to,
            relationship: e.relationship,
            confidence: e.confidence,
            valid_from: None,
            valid_to: None,
        })
        .collect();

    let node_type = g.get_node_type(&label);

    Ok(Json(NodeResponse {
        node_id,
        label,
        confidence,
        node_type,
        properties,
        edges_from,
        edges_to,
    }))
}

// ── DELETE /node/{label} ──

pub async fn delete_node(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<DeleteResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = Provenance::user("api");

    let deleted = g
        .delete(&label, &prov)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        drop(g);
        state.mark_dirty();
    }

    Ok(Json(DeleteResponse {
        deleted,
        entity: label,
    }))
}

// ── POST /learn/reinforce ──

pub async fn reinforce(
    State(state): State<AppState>,
    Json(req): Json<ReinforceRequest>,
) -> ApiResult<ReinforceResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    if req.source.is_some() {
        let prov = provenance(&req.source);
        g.reinforce_confirm(&req.entity, &prov)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        g.reinforce_access(&req.entity)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let new_confidence = g
        .get_node(&req.entity)
        .ok()
        .flatten()
        .map(|n| n.confidence)
        .unwrap_or(0.0);

    drop(g);
    state.mark_dirty();

    Ok(Json(ReinforceResponse {
        entity: req.entity,
        new_confidence,
    }))
}

// ── POST /learn/correct ──

pub async fn correct(
    State(state): State<AppState>,
    Json(req): Json<CorrectRequest>,
) -> ApiResult<CorrectResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&req.source);

    let result = g
        .correct(&req.entity, &prov, 3)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let propagated_to: Vec<String> = match &result {
        Some(cr) => cr
            .propagated
            .iter()
            .filter_map(|&(slot, _, _)| g.get_node_label_by_slot(slot))
            .collect(),
        None => Vec::new(),
    };

    drop(g);
    state.mark_dirty();

    Ok(Json(CorrectResponse {
        corrected: req.entity,
        propagated_to,
    }))
}

// ── POST /learn/decay ──

pub async fn decay(State(state): State<AppState>) -> ApiResult<DecayResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    let nodes_decayed = g
        .apply_decay()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(g);
    if nodes_decayed > 0 {
        state.mark_dirty();
    }

    Ok(Json(DecayResponse { nodes_decayed }))
}

// ── POST /learn/derive ──

pub async fn derive(
    State(state): State<AppState>,
    Json(req): Json<DeriveRequest>,
) -> ApiResult<DeriveResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    let rules: Vec<engram_core::learning::rules::Rule> = match req.rules {
        Some(rule_strs) => {
            let mut parsed = Vec::new();
            for s in &rule_strs {
                let rule = engram_core::learning::rules::parse_rule(s)
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;
                parsed.push(rule);
            }
            parsed
        }
        None => Vec::new(),
    };

    let prov = Provenance::user("api");
    let result = g
        .forward_chain(&rules, &prov)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let did_work = result.edges_created > 0 || result.flags_raised > 0;
    drop(g);
    if did_work {
        state.mark_dirty();
    }

    Ok(Json(DeriveResponse {
        rules_evaluated: result.rules_evaluated,
        rules_fired: result.rules_fired,
        edges_created: result.edges_created,
        flags_raised: result.flags_raised,
    }))
}

// ── GET /health ──

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ── GET /stats ──

pub async fn stats(State(state): State<AppState>) -> ApiResult<StatsResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let (nodes, edges) = g.stats();
    Ok(Json(StatsResponse { nodes, edges }))
}

// ── GET /compute ──

pub async fn compute(
    State(state): State<AppState>,
) -> Json<crate::state::ComputeInfo> {
    let compute = state.compute.read().unwrap().clone();
    Json(compute)
}

// ── POST /quantize ── Enable or disable int8 vector quantization

pub async fn set_quantization(
    State(state): State<AppState>,
    Json(req): Json<QuantizeRequest>,
) -> ApiResult<QuantizeResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let mode = if req.enabled {
        engram_core::QuantizationMode::Int8
    } else {
        engram_core::QuantizationMode::None
    };
    g.set_vector_quantization(mode);
    let memory_bytes = g.vector_memory_bytes();
    let quant = g.vector_quantization_mode();
    drop(g);

    // Persist to config
    {
        let mut cfg = state.config.write().unwrap_or_else(|e| e.into_inner());
        cfg.quantization_enabled = Some(req.enabled);
        if let Some(ref path) = state.config_path {
            let _ = cfg.save(path);
        }
    }

    Ok(Json(QuantizeResponse {
        mode: match quant {
            engram_core::QuantizationMode::Int8 => "int8".to_string(),
            engram_core::QuantizationMode::None => "none".to_string(),
        },
        vector_memory_bytes: memory_bytes as u64,
    }))
}

// ── GET /explain/{label} ──

pub async fn explain(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<ExplainResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let node = g
        .get_node(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("node not found: {label}")))?;

    let confidence = node.confidence;

    let properties = g
        .get_properties(&label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or_default();

    let cooccurrences: Vec<CooccurrenceHit> = g
        .cooccurrences_for(&label)
        .into_iter()
        .map(|(entity, count)| CooccurrenceHit {
            entity,
            count,
            probability: 0.0,
        })
        .collect();

    let edges_from: Vec<EdgeResponse> = g
        .edges_from(&label)
        .unwrap_or_default()
        .into_iter()
        .map(|e| EdgeResponse {
            from: e.from,
            to: e.to,
            relationship: e.relationship,
            confidence: e.confidence,
            valid_from: None,
            valid_to: None,
        })
        .collect();

    let edges_to: Vec<EdgeResponse> = g
        .edges_to(&label)
        .unwrap_or_default()
        .into_iter()
        .map(|e| EdgeResponse {
            from: e.from,
            to: e.to,
            relationship: e.relationship,
            confidence: e.confidence,
            valid_from: None,
            valid_to: None,
        })
        .collect();

    Ok(Json(ExplainResponse {
        entity: label,
        confidence,
        properties,
        cooccurrences,
        edges_from,
        edges_to,
    }))
}

// ── POST /ask ──

pub async fn ask(
    State(state): State<AppState>,
    Json(req): Json<natural::AskRequest>,
) -> ApiResult<natural::AskResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    Ok(Json(natural::handle_ask(&g, &req.question)))
}

// ── POST /tell ──

pub async fn tell(
    State(state): State<AppState>,
    Json(req): Json<natural::TellRequest>,
) -> ApiResult<natural::TellResponse> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let resp = natural::handle_tell(&mut g, &req.statement, req.source.as_deref());
    drop(g);
    state.mark_dirty();
    state.fire_rules_async();
    Ok(Json(resp))
}

// ── gRPC-style body-based variants ──

#[derive(serde::Deserialize)]
pub struct LabelBody {
    pub label: String,
}

pub async fn get_node_by_body(
    State(state): State<AppState>,
    Json(req): Json<LabelBody>,
) -> ApiResult<NodeResponse> {
    get_node(State(state), Path(req.label)).await
}

pub async fn delete_node_by_body(
    State(state): State<AppState>,
    Json(req): Json<LabelBody>,
) -> ApiResult<DeleteResponse> {
    delete_node(State(state), Path(req.label)).await
}

pub async fn stats_post(State(state): State<AppState>) -> ApiResult<StatsResponse> {
    stats(State(state)).await
}

// ── POST /rules ── Load inference rules for push-based triggers

pub async fn load_rules(
    State(state): State<AppState>,
    Json(req): Json<RulesRequest>,
) -> ApiResult<RulesResponse> {
    let mut parsed = Vec::new();
    for s in &req.rules {
        let rule = engram_core::learning::rules::parse_rule(s)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;
        parsed.push(rule);
    }

    let count = parsed.len();
    let names: Vec<String> = parsed.iter().map(|r| r.name.clone()).collect();

    if req.append.unwrap_or(false) {
        let mut rules = state.rules.write().map_err(|_| write_lock_err())?;
        rules.extend(parsed);
    } else {
        let mut rules = state.rules.write().map_err(|_| write_lock_err())?;
        *rules = parsed;
    }

    Ok(Json(RulesResponse {
        loaded: count as u32,
        names,
    }))
}

// ── GET /rules ── List loaded rules

pub async fn list_rules(
    State(state): State<AppState>,
) -> ApiResult<RulesListResponse> {
    let rules = state.rules.read().map_err(|_| read_lock_err())?;
    let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
    Ok(Json(RulesListResponse {
        count: rules.len() as u32,
        names,
    }))
}

// ── DELETE /rules ── Clear all loaded rules

pub async fn clear_rules(
    State(state): State<AppState>,
) -> ApiResult<RulesResponse> {
    let mut rules = state.rules.write().map_err(|_| write_lock_err())?;
    rules.clear();
    Ok(Json(RulesResponse {
        loaded: 0,
        names: Vec::new(),
    }))
}

// ── GET /export/jsonld ── Export entire graph as JSON-LD

pub async fn export_jsonld(
    State(state): State<AppState>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let nodes = g.all_nodes()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges = g.all_edges()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Build JSON-LD @context
    let context = serde_json::json!({
        "engram": "engram://vocab/",
        "schema": "https://schema.org/",
        "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
        "engram:confidence": { "@type": "http://www.w3.org/2001/XMLSchema#float" },
        "engram:memoryTier": { "@type": "http://www.w3.org/2001/XMLSchema#integer" },
    });

    // Build JSON-LD @graph
    let mut graph_nodes: Vec<serde_json::Value> = Vec::with_capacity(nodes.len());

    for node in &nodes {
        let uri = format!("engram://node/{}", urlencode(&node.label));
        let mut obj = serde_json::json!({
            "@id": uri,
            "rdfs:label": node.label,
            "engram:confidence": node.confidence,
            "engram:memoryTier": node.memory_tier,
        });

        if let Some(ref t) = node.node_type {
            obj["@type"] = serde_json::json!(format!("engram:{t}"));
        }

        // Add properties as datatype assertions
        for (k, v) in &node.properties {
            obj[format!("engram:{k}")] = serde_json::json!(v);
        }

        // Add outgoing edges as relationships
        let node_edges: Vec<&engram_core::graph::EdgeView> = edges.iter()
            .filter(|e| e.from == node.label)
            .collect();
        for edge in node_edges {
            let predicate = format!("engram:{}", edge.relationship);
            let target_uri = format!("engram://node/{}", urlencode(&edge.to));
            let edge_obj = serde_json::json!({
                "@id": target_uri,
                "engram:confidence": edge.confidence,
            });
            // Append to existing array or create one
            if let Some(existing) = obj.get(&predicate) {
                if existing.is_array() {
                    obj[&predicate] = serde_json::json!([existing.as_array().unwrap().clone(), vec![edge_obj]].concat());
                } else {
                    obj[predicate] = serde_json::json!([existing.clone(), edge_obj]);
                }
            } else {
                obj[predicate] = edge_obj;
            }
        }

        graph_nodes.push(obj);
    }

    let doc = serde_json::json!({
        "@context": context,
        "@graph": graph_nodes,
    });

    Ok(Json(doc))
}

// ── POST /import/jsonld ── Import JSON-LD data into the graph

pub async fn import_jsonld(
    State(state): State<AppState>,
    Json(req): Json<JsonLdImportRequest>,
) -> ApiResult<JsonLdImportResponse> {
    let prov = provenance(&req.source);
    let trust = req.trust.unwrap_or(0.5).clamp(0.0, 1.0);
    let mut nodes_imported: u32 = 0;
    let mut nodes_merged: u32 = 0;
    let mut edges_imported: u32 = 0;
    let mut edges_merged: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    // Extract @graph array, or treat the whole doc as a single node
    let items = if let Some(graph) = req.data.get("@graph") {
        match graph.as_array() {
            Some(arr) => arr.clone(),
            None => vec![graph.clone()],
        }
    } else {
        vec![req.data.clone()]
    };

    let mut g = state.graph.write().map_err(|_| write_lock_err())?;

    // First pass: create or merge nodes
    for item in &items {
        let label = extract_label(item);
        if label.is_empty() {
            continue;
        }

        let import_conf = item.get("engram:confidence")
            .or_else(|| item.get("confidence"))
            .and_then(|v| v.as_f64())
            .map(|c| c as f32);

        // Check if node already exists
        let existing = g.find_node_id(&label).unwrap_or(None);

        if let Some(_existing_id) = existing {
            // Node exists — merge confidence using trust-weighted formula
            if let Some(c_import) = import_conf {
                if let Ok(Some(c_local)) = g.node_confidence(&label) {
                    let c_new = c_local + trust * (c_import - c_local);
                    let _ = g.set_node_confidence(&label, c_new);
                }
            }
            nodes_merged += 1;
        } else {
            // New node — create with imported confidence
            let store_result = if let Some(c) = import_conf {
                g.store_with_confidence(&label, c, &prov)
            } else {
                g.store(&label, &prov)
            };
            match store_result {
                Ok(_) => { nodes_imported += 1; }
                Err(e) => { errors.push(format!("store {label}: {e}")); continue; }
            }
        }

        // Set @type if present
        if let Some(type_val) = item.get("@type") {
            let type_str = match type_val {
                serde_json::Value::String(s) => strip_prefix(s),
                serde_json::Value::Array(arr) => {
                    arr.first()
                        .and_then(|v| v.as_str())
                        .map(strip_prefix)
                        .unwrap_or_default()
                }
                _ => String::new(),
            };
            if !type_str.is_empty() {
                let _ = g.set_node_type(&label, &type_str);
            }
        }

        // Import properties (skip JSON-LD keywords and relationships)
        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                if k.starts_with('@') || k == "engram:confidence" || k == "engram:memoryTier" {
                    continue;
                }
                if let Some(s) = v.as_str() {
                    let prop_key = strip_prefix(k);
                    if !prop_key.is_empty() {
                        let _ = g.set_property(&label, &prop_key, s);
                    }
                }
            }
        }
    }

    // Second pass: create or merge edges
    for item in &items {
        let from_label = extract_label(item);
        if from_label.is_empty() {
            continue;
        }

        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                if k.starts_with('@') || k == "engram:confidence" || k == "engram:memoryTier" {
                    continue;
                }
                let rel = strip_prefix(k);
                let targets = if v.is_array() {
                    v.as_array().unwrap().clone()
                } else if v.is_object() {
                    vec![v.clone()]
                } else {
                    continue;
                };

                for target in &targets {
                    if let Some(target_id) = target.get("@id").and_then(|v| v.as_str()) {
                        let to_label = uri_to_label(target_id);
                        if to_label.is_empty() || to_label == from_label {
                            continue;
                        }
                        // Ensure target node exists
                        if g.find_node_id(&to_label).unwrap_or(None).is_none() {
                            match g.store(&to_label, &prov) {
                                Ok(_) => { nodes_imported += 1; }
                                Err(e) => { errors.push(format!("store {to_label}: {e}")); continue; }
                            }
                        }

                        let c_import = target.get("engram:confidence")
                            .or_else(|| target.get("confidence"))
                            .and_then(|v| v.as_f64())
                            .map(|c| c as f32)
                            .unwrap_or(0.8);

                        // Check if edge already exists
                        match g.find_edge_slot(&from_label, &to_label, &rel) {
                            Ok(Some(slot)) => {
                                // Edge exists — merge confidence
                                if let Ok(c_local) = g.edge_confidence(slot) {
                                    let c_new = c_local + trust * (c_import - c_local);
                                    let _ = g.update_edge_confidence(slot, c_new);
                                }
                                edges_merged += 1;
                            }
                            Ok(None) => {
                                // New edge — create
                                match g.relate_with_confidence(&from_label, &to_label, &rel, c_import, &prov) {
                                    Ok(_) => { edges_imported += 1; }
                                    Err(e) => errors.push(format!("relate {from_label} -> {to_label}: {e}")),
                                }
                            }
                            Err(e) => errors.push(format!("find edge {from_label} -> {to_label}: {e}")),
                        }
                    }
                }
            }
        }
    }

    drop(g);
    state.mark_dirty();
    state.fire_rules_async();

    Ok(Json(JsonLdImportResponse {
        nodes_imported,
        edges_imported,
        nodes_merged,
        edges_merged,
        errors: if errors.is_empty() { None } else { Some(errors) },
    }))
}

/// Percent-encode a label for use in URIs.
fn urlencode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
        .replace('/', "%2F")
}

/// Extract a label from a JSON-LD node. Tries rdfs:label, schema:name, then @id.
/// Normalize an LLM endpoint URL to produce a full `/chat/completions` URL.
///
/// Handles all common provider patterns:
///   `http://localhost:11434`                -> `http://localhost:11434/v1/chat/completions`
///   `http://localhost:11434/v1`             -> `http://localhost:11434/v1/chat/completions`
///   `http://localhost:11434/v1/chat/completions` -> as-is
///   `https://api.openai.com/v1`            -> `https://api.openai.com/v1/chat/completions`
///   `http://localhost:8000/v1`             -> `http://localhost:8000/v1/chat/completions`  (vLLM)
///   `http://localhost:1234/v1`             -> `http://localhost:1234/v1/chat/completions`  (LM Studio)
pub(crate) fn normalize_llm_endpoint(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');

    // Already a full chat completions URL
    if s.ends_with("/chat/completions") {
        return s.to_string();
    }

    // Strip /completions if user partially entered it
    let s = s.strip_suffix("/completions").unwrap_or(s);

    // Has a recognized API prefix -- just append /chat/completions
    if s.ends_with("/v1") || s.ends_with("/api") || s.ends_with("/v2") {
        return format!("{s}/chat/completions");
    }

    // Bare host+port (no meaningful path) -- add /v1/chat/completions
    let after_scheme = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else if let Some(rest) = s.strip_prefix("http://") {
        rest
    } else {
        return format!("{s}/chat/completions");
    };

    let path = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => "",
    };

    if path.is_empty() || path == "/" {
        format!("{s}/v1/chat/completions")
    } else {
        format!("{s}/chat/completions")
    }
}

fn extract_label(item: &serde_json::Value) -> String {
    if let Some(label) = item.get("rdfs:label").and_then(|v| v.as_str()) {
        return label.to_string();
    }
    if let Some(label) = item.get("schema:name").and_then(|v| v.as_str()) {
        return label.to_string();
    }
    if let Some(label) = item.get("label").and_then(|v| v.as_str()) {
        return label.to_string();
    }
    if let Some(id) = item.get("@id").and_then(|v| v.as_str()) {
        return uri_to_label(id);
    }
    String::new()
}

/// Convert a URI to a human-readable label by stripping namespace prefixes.
fn uri_to_label(uri: &str) -> String {
    // Strip engram:// prefix
    if let Some(rest) = uri.strip_prefix("engram://node/") {
        return urldecode(rest);
    }
    // Strip common URI patterns: take last path segment or fragment
    if let Some(idx) = uri.rfind('#') {
        return urldecode(&uri[idx + 1..]);
    }
    if let Some(idx) = uri.rfind('/') {
        let segment = &uri[idx + 1..];
        if !segment.is_empty() {
            return urldecode(segment);
        }
    }
    urldecode(uri)
}

/// Decode percent-encoded URI components.
fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Strip namespace prefix (e.g., "engram:Person" -> "Person", "schema:name" -> "name").
fn strip_prefix(s: &str) -> String {
    if let Some(idx) = s.rfind(':') {
        s[idx + 1..].to_string()
    } else {
        s.to_string()
    }
}

// ── GET /proxy/gdelt ── CORS proxy for GDELT API ──

/// Proxy GDELT requests to avoid CORS restrictions in browser.
/// Forwards query params to api.gdeltproject.org and returns the response.
pub async fn proxy_gdelt(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let mut url = "https://api.gdeltproject.org/api/v2/doc/doc?".to_string();
    let query_string: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect();
    url.push_str(&query_string.join("&"));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "engram-intel/0.1")
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("GDELT request failed: {e}")))?;

    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let body = resp
        .bytes()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok((
        status,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

/// Proxy for Google News RSS -- fetches RSS XML and converts to JSON for the dashboard.
pub async fn proxy_news_rss(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "missing 'q' parameter".to_string()));
    }

    let rss_url = format!(
        "https://news.google.com/rss/search?q={}&hl=en&gl=US&ceid=US:en",
        urlencoding::encode(&query)
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let resp = client
        .get(&rss_url)
        .header("User-Agent", "engram-intel/0.1")
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("RSS fetch failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, "RSS feed returned error".to_string()));
    }

    let xml = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    // Parse RSS XML into simple JSON array of {title, link, pubDate, domain}
    let mut items = Vec::new();
    for item_block in xml.split("<item>").skip(1) {
        let title = extract_xml_tag(item_block, "title").unwrap_or_default();
        let link = extract_xml_tag(item_block, "link").unwrap_or_default();
        let pub_date = extract_xml_tag(item_block, "pubDate").unwrap_or_default();
        let source = extract_xml_tag(item_block, "source").unwrap_or_default();

        if !title.is_empty() && !link.is_empty() {
            let domain = link.split('/').nth(2).unwrap_or("").to_string();
            items.push(serde_json::json!({
                "title": title,
                "link": link,
                "pubDate": pub_date,
                "source": source,
                "domain": domain,
            }));
        }
    }

    let body = serde_json::json!({ "items": items }).to_string();

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

// ── GET /proxy/search ── Web search via DuckDuckGo HTML ──

/// Proxy web search via Brave Search API or DuckDuckGo fallback.
/// Set ENGRAM_SEARCH_API_KEY for Brave Search, otherwise falls back to DuckDuckGo instant answers.
/// Returns JSON array of search results with title, url, snippet.
pub async fn proxy_web_search(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "missing 'q' parameter".to_string()));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut results = Vec::new();

    // Read web search config from AppState
    let (provider, api_key, search_base_url) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.web_search_provider.clone().unwrap_or_else(|| "searxng".to_string()),
            cfg.web_search_api_key.clone().unwrap_or_default(),
            cfg.web_search_url.clone(),
        )
    };

    // Optional time_range: "day", "week", "month", "year"
    let time_range = params.get("time_range").cloned().unwrap_or_default();

    match provider.as_str() {
        "brave" => {
            // Brave Search API
            let brave_url = format!(
                "https://api.search.brave.com/res/v1/web/search?q={}",
                urlencoding::encode(&query),
            );
            let resp = client
                .get(&brave_url)
                .header("Accept", "application/json")
                .header("X-Subscription-Token", &api_key)
                .send()
                .await;

            if let Ok(resp) = resp {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(web_results) = data.pointer("/web/results").and_then(|r| r.as_array()) {
                            for r in web_results.iter().take(50) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or_default();
                                let domain = url.split('/').nth(2).unwrap_or("");
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(serde_json::json!({
                                        "title": title,
                                        "url": url,
                                        "snippet": snippet,
                                        "domain": domain,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
        "duckduckgo" => {
            // DuckDuckGo only — skip SearXNG entirely
        }
        _ => {
            // Default: SearXNG (self-hosted meta search, no API key needed)
            let searxng_base = search_base_url.clone()
                .or_else(|| std::env::var("ENGRAM_SEARXNG_URL").ok())
                .unwrap_or_else(|| "http://localhost:8090".to_string());
            let time_param = if !time_range.is_empty() {
                format!("&time_range={}", urlencoding::encode(&time_range))
            } else {
                String::new()
            };
            let search_url = format!(
                "{}/search?q={}&format=json&categories=general&engines=google,bing,duckduckgo{}",
                searxng_base,
                urlencoding::encode(&query),
                time_param
            );
            let resp = client
                .get(&search_url)
                .header("Accept", "application/json")
                .send()
                .await;

            if let Ok(resp) = resp {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(web_results) = data.get("results").and_then(|r| r.as_array()) {
                            for r in web_results.iter().take(50) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                let snippet = r.get("content").and_then(|d| d.as_str()).unwrap_or_default();
                                let domain = url.split('/').nth(2).unwrap_or("");
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(serde_json::json!({
                                        "title": title,
                                        "url": url,
                                        "snippet": snippet,
                                        "domain": domain,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: DuckDuckGo Instant Answer API (limited but no auth needed)
    if results.is_empty() {
        let ddg_url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(&query)
        );
        let resp = client
            .get(&ddg_url)
            .header("User-Agent", "engram-intel/1.0")
            .send()
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    // Abstract
                    if let (Some(heading), Some(abs)) = (
                        data.get("Heading").and_then(|h| h.as_str()),
                        data.get("Abstract").and_then(|a| a.as_str()),
                    ) {
                        if !abs.is_empty() {
                            let url = data.get("AbstractURL").and_then(|u| u.as_str()).unwrap_or_default();
                            let domain = url.split('/').nth(2).unwrap_or("");
                            results.push(serde_json::json!({
                                "title": heading,
                                "url": url,
                                "snippet": abs,
                                "domain": domain,
                            }));
                        }
                    }
                    // Related topics
                    if let Some(topics) = data.get("RelatedTopics").and_then(|r| r.as_array()) {
                        for t in topics.iter().take(8) {
                            let text = t.get("Text").and_then(|x| x.as_str()).unwrap_or_default();
                            let url = t.get("FirstURL").and_then(|u| u.as_str()).unwrap_or_default();
                            if !text.is_empty() {
                                let title = text.split(" - ").next().unwrap_or(text);
                                let domain = url.split('/').nth(2).unwrap_or("");
                                results.push(serde_json::json!({
                                    "title": title,
                                    "url": url,
                                    "snippet": text,
                                    "domain": domain,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    let body = serde_json::json!({ "results": results }).to_string();

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    ).into_response())
}

// ── POST /proxy/llm ── Forward chat completion requests to configured LLM ──

/// Proxy LLM chat completion requests to an OpenAI-compatible endpoint.
/// Uses ENGRAM_EMBED_ENDPOINT (same as embeddings) or ENGRAM_LLM_ENDPOINT if set.
/// Model defaults to ENGRAM_LLM_MODEL env var, then request body, then "llama3.2".
pub async fn proxy_llm(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    // Read config from AppState, then secrets store, then env vars.
    let (endpoint, api_key, default_model) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        // API key: secrets store first, then config (legacy), then env
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        let model = cfg.llm_model.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_MODEL").ok());
        (ep, key, model)
    };

    let endpoint = endpoint.ok_or_else(|| {
        api_err(StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured. Set endpoint via POST /config or ENGRAM_LLM_ENDPOINT env var.")
    })?;
    let default_model = default_model.unwrap_or_else(|| "llama3.2".to_string());

    let messages = body.get("messages").cloned().unwrap_or(serde_json::json!([]));
    let model = body.get("model")
        .and_then(|m| m.as_str())
        .unwrap_or(&default_model);
    let temperature = body.get("temperature")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.7);
    let max_tokens = body.get("max_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(1024);
    let tools = body.get("tools").cloned();

    let url = normalize_llm_endpoint(&endpoint);

    let mut request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });
    // Pass through tools for function calling
    if let Some(tools_val) = tools {
        request_body["tools"] = tools_val;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut req = client.post(&url)
        .header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req
        .json(&request_body)
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("LLM request failed: {e}")))?;

    if !resp.status().is_success() {
        let status_code = resp.status().as_u16();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(api_err(
            StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("LLM returned {status_code}: {body_text}"),
        ));
    }

    let body_text = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;

    let json_value: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("invalid JSON from LLM: {e}")))?;

    Ok(Json(json_value))
}

/// GET /proxy/models — fetch available models from the configured LLM endpoint (avoids CORS).
pub async fn proxy_llm_models(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let (endpoint, api_key) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        let ep = cfg.llm_endpoint.clone()
            .or_else(|| std::env::var("ENGRAM_LLM_ENDPOINT").ok());
        let key = state.secrets.read().ok()
            .and_then(|guard| guard.as_ref().and_then(|s| s.get("llm.api_key").map(String::from)))
            .or_else(|| cfg.llm_api_key.clone())
            .or_else(|| std::env::var("ENGRAM_LLM_API_KEY").ok())
            .unwrap_or_default();
        (ep, key)
    };

    let endpoint = endpoint.ok_or_else(|| {
        api_err(StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured. Set endpoint via POST /config or ENGRAM_LLM_ENDPOINT env var.")
    })?;

    // Build /v1/models URL from the raw endpoint
    let base = endpoint.trim().trim_end_matches('/');
    let models_url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        // Strip any path after host (e.g. /v1/chat/completions) and use /v1/models
        let after_scheme = base.strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
            .unwrap_or(base);
        let scheme_end = base.len() - after_scheme.len();
        let host_end = after_scheme.find('/').map(|i| scheme_end + i).unwrap_or(base.len());
        format!("{}/v1/models", &base[..host_end])
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut req = client.get(&models_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let resp = req.send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Failed to reach LLM: {e}")))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_err(
            StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("LLM returned {code}: {body}"),
        ));
    }

    let body = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Invalid JSON from LLM: {e}")))?;

    Ok(Json(json))
}

/// POST /proxy/fetch-models — fetch available models from ANY endpoint (for wizard/setup).
/// Body: { "endpoint": "http://localhost:11434/api/embed" }
/// Returns the raw JSON from {base}/api/tags (Ollama) or {base}/v1/models (OpenAI-compatible).
pub async fn proxy_fetch_models(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let endpoint = body.get("endpoint").and_then(|v| v.as_str()).unwrap_or("");
    if endpoint.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "Missing 'endpoint' field"));
    }

    // Derive base URL by stripping known paths
    let base = endpoint.trim().trim_end_matches('/')
        .replace("/v1/chat/completions", "")
        .replace("/v1/embeddings", "")
        .replace("/api/embed", "")
        .replace("/api/generate", "");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Try Ollama-style /api/tags first
    let ollama_url = format!("{base}/api/tags");
    if let Ok(resp) = client.get(&ollama_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("models").and_then(|m| m.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
                        return Ok(Json(json));
                    }
                }
            }
        }
    }

    // Try OpenAI-style /v1/models
    let openai_url = format!("{base}/v1/models");
    let api_key = body.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let mut req = client.get(&openai_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let resp = req.send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Failed to reach endpoint: {e}")))?;

    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("Endpoint returned {}", resp.status().as_u16())));
    }

    let text = resp.text().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Invalid JSON: {e}")))?;

    Ok(Json(json))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after_open = &xml[start..];
    let content_start = after_open.find('>')? + 1;
    let content = &after_open[content_start..];
    let end = content.find(&close)?;
    let val = &content[..end];
    // Handle CDATA
    let val = val.trim();
    let val = if val.starts_with("<![CDATA[") && val.ends_with("]]>") {
        &val[9..val.len()-3]
    } else {
        val
    };
    Some(val.to_string())
}

// ── POST /ingest — ingest pipeline ──

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
    // MCP / A2A callers don't have access to AppState caches — pass empty caches.
    let no_ner_cache = std::sync::Arc::new(std::sync::RwLock::new(None));
    let no_rel_cache = std::sync::Arc::new(std::sync::RwLock::new(None));
    build_pipeline(graph, config, kb_endpoints, ner_model, rel_model, None, None, None,
        no_ner_cache, no_rel_cache)
}

/// Build a pipeline with all available NER + relation extraction backends wired up.
/// `ner_model` / `rel_model` come from EngineConfig — the user's chosen models.
/// `ner_cache` / `rel_cache` are shared caches — if populated, the backend is reused
/// instead of reloading the model from disk/HF. If empty, a new backend is created and cached.
#[cfg(feature = "ingest")]
fn build_pipeline(
    graph: std::sync::Arc<std::sync::RwLock<engram_core::graph::Graph>>,
    config: engram_ingest::PipelineConfig,
    kb_endpoints: Option<Vec<crate::state::KbEndpointConfig>>,
    ner_model: Option<String>,
    rel_model: Option<String>,
    relation_templates: Option<std::collections::HashMap<String, String>>,
    rel_threshold: Option<f32>,
    _coreference_enabled: Option<bool>,
    ner_cache: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn engram_ingest::Extractor>>>>,
    rel_cache: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn engram_ingest::RelationExtractor>>>>,
) -> engram_ingest::Pipeline {
    use engram_ingest::{NerChain, ChainStrategy};

    let mut pipeline = engram_ingest::Pipeline::new(graph.clone(), config);

    // Build a MergeAll NER chain: run ALL backends, merge + dedup results.
    // This ensures gazetteer resolved_to IDs are preserved AND GLiNER finds
    // new entities the gazetteer doesn't know about yet.
    let mut chain = NerChain::new(ChainStrategy::MergeAll);

    // 1. Rule-based NER (emails, IPs, dates — always available, fast)
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

    // 0. Knowledge Base (SPARQL) — bootstraps empty graphs
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
            rel_chain.add_backend(Box::new(
                engram_ingest::KbRelationExtractor::new(enabled, graph.clone()),
            ));
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

    // Convert IngestItems to RawItems
    let items: Vec<engram_ingest::types::RawItem> = req.items
        .into_iter()
        .map(|item| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            match item {
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

    // Run build_pipeline + execute in spawn_blocking to avoid tokio runtime panic
    // from reqwest::blocking (KbRelationExtractor)
    let parallel = req.parallel.unwrap_or(false);
    let result = tokio::task::spawn_blocking(move || {
        let pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
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
     Json(ErrorResponse { error: "ingest feature not enabled — rebuild with --features ingest".into() }))
}

/// POST /ingest/analyze — run NER on text without storing, for preview
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
    let text = req.text;

    // Run build_pipeline + analyze in spawn_blocking to avoid tokio runtime panic
    // from reqwest::blocking (KbRelationExtractor)
    let result = tokio::task::spawn_blocking(move || {
        let pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
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
     Json(ErrorResponse { error: "ingest feature not enabled — rebuild with --features ingest".into() }))
}

/// POST /ingest/file — ingest from file upload (multipart)
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
        let pipeline = build_pipeline(graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled, ner_cache, rel_cache);
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
     Json(ErrorResponse { error: "ingest feature not enabled — rebuild with --features ingest".into() }))
}

/// POST /ingest/configure — update pipeline defaults (runtime).
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
     Json(ErrorResponse { error: "ingest feature not enabled — rebuild with --features ingest".into() }))
}

// ── GET /sources — list registered sources (stub) ──

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
    let ids: Vec<&str> = engine.list_rules();
    Ok(Json(serde_json::json!({ "rules": ids })))
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
    let limit: usize = query.get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

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
        .take(limit)
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

// ── Seed enrichment endpoints ────────────────────────────────────────

/// POST /ingest/seed/start — start interactive seed session: NER + AoI detection.
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

/// POST /ingest/seed/confirm-aoi — confirm area of interest, trigger entity linking (Step 1).
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

        let extractor = engram_ingest::KbRelationExtractor::with_config(
            kb_endpoints,
            graph,
            config_snap.llm_endpoint,
            config_snap.llm_model,
            Some(event_bus),
            config_snap.web_search_provider,
            config_snap.web_search_api_key,
            config_snap.web_search_url,
        );

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
                        session.connections.push(crate::state::SeedConnection {
                            from: session.entities[rel.head_idx].label.clone(),
                            to: session.entities[rel.tail_idx].label.clone(),
                            rel_type: rel.rel_type.clone(),
                            source: "kb".into(),
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

/// POST /ingest/seed/confirm-entities — confirm entity matches, trigger connections.
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

/// POST /ingest/seed/commit — write confirmed entities + relations to graph.
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

/// GET /ingest/seed/stream?session_id=xxx — SSE stream filtered to a seed session.
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
    let pipeline = Pipeline::new(graph, config);

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

/// GET /mesh/profiles — list all known peer knowledge profiles.
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

/// GET /mesh/discover?topic=X — find peers covering a topic.
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

/// POST /mesh/query — execute a federated query.
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

/// GET /batch/jobs/{id}/stream — SSE progress stream for a batch ingest job.
pub async fn batch_job_stream(
    Path(_job_id): Path<String>,
) -> impl axum::response::IntoResponse {
    // Batch job tracking is not yet implemented — return a stub SSE stream
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

    // Invalidate cached NER/REL backends if relevant config fields changed
    #[cfg(feature = "ingest")]
    {
        let ner_changed = patch.ner_model.is_some() || patch.ner_provider.is_some();
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

// ── ONNX model upload ────────────────────────────────────────────────

/// POST /config/onnx-model — Upload ONNX embedding model and tokenizer files.
/// Accepts multipart/form-data with "model" and "tokenizer" file fields.
/// Files are streamed to disk as sidecars next to the .brain file.
pub async fn upload_onnx_model(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    use std::io::Write;

    // Derive brain base path from config_path (strip .config suffix)
    let brain_path = state.config_path.as_ref()
        .and_then(|p| p.to_str())
        .and_then(|s| s.strip_suffix(".config"))
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine brain file path"))?;

    let model_path = format!("{}.model.onnx", brain_path);
    let tokenizer_path = format!("{}.tokenizer.json", brain_path);

    // Process each multipart field
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        api_err(StatusCode::BAD_REQUEST, format!("multipart error: {e}"))
    })? {
        let name = field.name().unwrap_or("").to_string();
        let dest = match name.as_str() {
            "model" => &model_path,
            "tokenizer" => &tokenizer_path,
            _ => continue,
        };
        let data = field.bytes().await.map_err(|e| {
            api_err(StatusCode::BAD_REQUEST, format!("read field '{name}' failed: {e}"))
        })?;
        let mut f = std::fs::File::create(dest)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {name} failed: {e}")))?;
        f.write_all(&data)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {name} failed: {e}")))?;
    }

    // Check what files exist now
    let model_exists = std::path::Path::new(&model_path).exists();
    let tokenizer_exists = std::path::Path::new(&tokenizer_path).exists();
    let model_size = if model_exists {
        std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0)
    } else { 0 };

    // Hot-load the ONNX embedder if both files are present
    let mut activated = false;
    #[cfg(feature = "onnx")]
    if model_exists && tokenizer_exists {
        match engram_core::OnnxEmbedder::load(
            std::path::Path::new(&model_path),
            std::path::Path::new(&tokenizer_path),
        ) {
            Ok(embedder) => {
                let dim = embedder.dim();
                let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                g.set_embedder(Box::new(embedder));
                drop(g);
                state.set_embedder_info("ONNX Local".into(), dim, "local".into());
                tracing::info!("hot-loaded ONNX embedder ({}D) from {}", dim, model_path);
                activated = true;
            }
            Err(e) => {
                tracing::warn!("ONNX embedder load failed (files saved but not activated): {e}");
            }
        }
    }

    let message = if !model_exists || !tokenizer_exists {
        "Partial upload. Both model.onnx and tokenizer.json are required."
    } else if activated {
        "ONNX embedder activated. You can now reindex."
    } else {
        "ONNX model files installed. Restart the server to activate."
    };

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_path": model_path,
        "tokenizer_path": tokenizer_path,
        "model_exists": model_exists,
        "tokenizer_exists": tokenizer_exists,
        "model_size_mb": model_size as f64 / 1_048_576.0,
        "activated": activated,
        "message": message,
    })))
}

/// GET /config/onnx-model — Check if ONNX model files exist.
pub async fn check_onnx_model(
    State(_state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    // Check ~/.engram/models/embed/ for any installed model
    let embed_dir = engram_home().map(|h| h.join("models").join("embed"));
    let found_model = embed_dir.as_ref().and_then(|dir| {
        std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).find(|e| {
            let p = e.path();
            p.join("model.onnx").exists() && p.join("tokenizer.json").exists()
        }).map(|e| e.path())
    });

    if let Some(ref model_dir) = found_model {
        let model_path = model_dir.join("model.onnx");
        let model_size = std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0);
        Ok(Json(serde_json::json!({
            "ready": true,
            "model_exists": true,
            "tokenizer_exists": true,
            "model_path": model_path.to_string_lossy(),
            "model_size_mb": model_size as f64 / 1_048_576.0,
        })))
    } else {
        Ok(Json(serde_json::json!({
            "ready": false,
            "model_exists": false,
            "tokenizer_exists": false,
            "model_size_mb": 0.0,
        })))
    }
}

/// POST /config/onnx-download — Download ONNX embedding model from HuggingFace.
///
/// Accepts JSON: { "model_url": "...", "tokenizer_url": "..." }
/// Downloads both files server-side and installs them as the active embedder.
pub async fn download_onnx_model(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    // Validate URLs point to huggingface.co
    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    // Derive model name from URL or request body
    let model_name = body["model_id"].as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Extract from URL: https://huggingface.co/intfloat/multilingual-e5-small/resolve/...
            model_url.split('/').nth(4).unwrap_or("default").to_string()
        });

    // Save to ~/.engram/models/embed/<model_name>/
    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("embed").join(&model_name);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("ONNX embed model already installed ({} MB), skipping download", size / 1_048_576);
            // Still hot-load if not yet active
            let mut activated = false;
            #[cfg(feature = "onnx")]
            {
                match engram_core::OnnxEmbedder::load(&model_path, &tokenizer_path) {
                    Ok(embedder) => {
                        let dim = embedder.dim();
                        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                        g.set_embedder(Box::new(embedder));
                        drop(g);
                        state.set_embedder_info(model_name.clone(), dim, "local".into());
                        activated = true;
                    }
                    Err(e) => {
                        tracing::debug!("ONNX hot-load on skip: {e}");
                    }
                }
            }
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_size_mb": size / 1_048_576,
                "activated": activated,
            })));
        }
    }

    let client = reqwest::Client::new();

    // Download tokenizer first (small, quick validation)
    tracing::info!("downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download model (large, stream to disk)
    tracing::info!("downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("downloaded {} MB model to {}", downloaded / 1_048_576, model_path.display());

    // Hot-load the ONNX embedder
    let mut activated = false;
    let mut load_error: Option<String> = None;
    #[cfg(feature = "onnx")]
    {
        match engram_core::OnnxEmbedder::load(&model_path, &tokenizer_path) {
            Ok(embedder) => {
                let dim = embedder.dim();
                let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                g.set_embedder(Box::new(embedder));
                drop(g);
                state.set_embedder_info(model_name.clone(), dim, "local".into());
                tracing::info!("hot-loaded ONNX embedder {} ({}D)", model_name, dim);
                activated = true;
            }
            Err(e) => {
                tracing::warn!("ONNX load failed after download: {e}");
                load_error = Some(e.to_string());
            }
        }
    }

    let message = if activated {
        "ONNX embedder downloaded and activated.".to_string()
    } else if let Some(ref err) = load_error {
        format!("ONNX model downloaded but hot-load failed: {err}. Restart the server to activate.")
    } else {
        "ONNX model downloaded. Restart the server to activate.".to_string()
    };

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_path": model_path.to_string_lossy(),
        "tokenizer_path": tokenizer_path.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
        "activated": activated,
        "message": message,
    })))
}

// ── POST /config/ollama-pull — Pull a model from Ollama ──

pub async fn ollama_pull(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let model = body["model"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model required"))?;

    // Get Ollama endpoint from config -- prefer llm_endpoint, skip non-HTTP (e.g. onnx://)
    let ollama_base = {
        let cfg = state.config.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock"))?;
        let candidates = [cfg.llm_endpoint.clone(), cfg.embed_endpoint.clone()];
        candidates.into_iter()
            .flatten()
            .find(|ep| ep.starts_with("http://") || ep.starts_with("https://"))
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    };
    // Extract base URL (strip path)
    let base = if let Some(idx) = ollama_base.find("/api/") {
        &ollama_base[..idx]
    } else if let Some(idx) = ollama_base.find("/v1/") {
        &ollama_base[..idx]
    } else {
        ollama_base.trim_end_matches('/')
    };

    let pull_url = format!("{}/api/pull", base);
    tracing::info!("pulling Ollama model '{}' from {}", model, pull_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 min timeout for large models
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("client error: {e}")))?;

    let resp = client.post(&pull_url)
        .json(&serde_json::json!({ "name": model, "stream": false }))
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Ollama pull failed: {e}. Is Ollama running at {}?", base)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("Ollama pull returned {}: {}", status, body)));
    }

    let result = resp.text().await.unwrap_or_default();
    tracing::info!("Ollama pull complete for '{}'", model);

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model": model,
        "response": result,
    })))
}

// ── POST /config/ner-download — Download GLiNER ONNX NER model from HuggingFace ──

/// Download a GLiNER ONNX NER model from HuggingFace to `~/.engram/models/ner/`.
///
/// Downloads `tokenizer.json` + `onnx/model_{variant}.onnx` (saved as `model.onnx`).
/// Default model: `knowledgator/gliner-x-small`, default variant: `quantized` (173 MB).
// ── POST /config/gliner2-download — Download GLiNER2 ONNX model from HuggingFace ──

/// Download GLiNER2 multi-file ONNX model from HuggingFace.
///
/// Request: `{"repo_id": "dx111ge/gliner2-multi-v1-onnx", "variant": "fp16", "force": false}`
///
/// Downloads all required ONNX files + tokenizer + config to `~/.engram/models/gliner2/<model>/`.
pub async fn download_gliner2_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let repo_id = body["repo_id"].as_str()
        .unwrap_or("dx111ge/gliner2-multi-v1-onnx")
        .to_string();
    let variant = body["variant"].as_str().unwrap_or("fp16").to_string();
    let force = body["force"].as_bool().unwrap_or(false);

    if repo_id.matches('/').count() != 1 || repo_id.contains("..") || repo_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid repo_id format"));
    }

    // Model name from repo_id (e.g., "gliner2-multi-v1-onnx")
    let model_name = repo_id.split('/').last().unwrap_or("gliner2");

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("gliner2").join(model_name);

    // Skip if already installed
    let config_path = model_dir.join("gliner2_config.json");
    if !force && config_path.exists() {
        return Ok(Json(serde_json::json!({
            "status": "ok",
            "skipped": true,
            "message": "model already installed",
            "model_dir": model_dir.to_string_lossy(),
        })));
    }

    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir: {e}")))?;

    let base_url = format!("https://huggingface.co/{}/resolve/main", repo_id);
    let client = reqwest::Client::new();

    // Files to download: config first (tells us which ONNX files to get)
    let config_files = vec![
        "gliner2_config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "special_tokens_map.json",
        "added_tokens.json",
        "spm.model",
    ];

    // Download config files
    for filename in &config_files {
        let url = format!("{}/{}", base_url, filename);
        let dest = model_dir.join(filename);
        tracing::info!(file = %filename, "downloading");
        let resp = client.get(&url).send().await
            .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download {filename}: {e}")))?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await
                .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("read {filename}: {e}")))?;
            tokio::fs::write(&dest, &bytes).await
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
        }
    }

    // Read config to find ONNX files for requested variant
    let cfg_str = tokio::fs::read_to_string(&config_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("read config: {e}")))?;
    let cfg: serde_json::Value = serde_json::from_str(&cfg_str)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("parse config: {e}")))?;

    let onnx_files = &cfg["onnx_files"][&variant];
    if onnx_files.is_null() {
        return Err(api_err(StatusCode::BAD_REQUEST,
            format!("variant '{}' not found in config", variant)));
    }

    // Collect unique ONNX filenames (+ their .data files)
    let mut files_to_download: Vec<String> = Vec::new();
    for (_key, val) in onnx_files.as_object().unwrap_or(&serde_json::Map::new()) {
        if let Some(fname) = val.as_str() {
            if !files_to_download.contains(&fname.to_string()) {
                files_to_download.push(fname.to_string());
                files_to_download.push(format!("{}.data", fname));
            }
        }
    }

    // Stream-download ONNX files
    let mut total_bytes: u64 = 0;
    for filename in &files_to_download {
        let url = format!("{}/{}", base_url, filename);
        let dest = model_dir.join(filename);

        if !force && dest.exists() {
            let size = tokio::fs::metadata(&dest).await.map(|m| m.len()).unwrap_or(0);
            if size > 0 {
                total_bytes += size;
                continue;
            }
        }

        tracing::info!(file = %filename, "downloading ONNX file");
        let resp = client.get(&url).send().await
            .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download {filename}: {e}")))?;

        if !resp.status().is_success() {
            // .data files might not exist for single-file models (e.g., int8)
            if filename.ends_with(".data") {
                continue;
            }
            return Err(api_err(StatusCode::BAD_GATEWAY,
                format!("{} returned {}", filename, resp.status())));
        }

        let mut file = tokio::fs::File::create(&dest).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create {filename}: {e}")))?;
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("stream {filename}: {e}")))?;
            total_bytes += chunk.len() as u64;
            file.write_all(&chunk).await
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "repo_id": repo_id,
        "variant": variant,
        "model_dir": model_dir.to_string_lossy(),
        "total_mb": total_bytes / 1_048_576,
    })))
}

///
/// Request: `{"model_id": "knowledgator/gliner-x-small", "variant": "quantized", "force": false}`
///
/// For air-gapped systems, use `/config/model-upload` instead.
pub async fn download_ner_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str().unwrap_or("knowledgator/gliner-x-small").to_string();
    let variant = body["variant"].as_str().unwrap_or("quantized").to_string();
    let force = body["force"].as_bool().unwrap_or(false);

    // Validate model_id format: must contain exactly one "/"
    if model_id.matches('/').count() != 1 || model_id.contains("..") || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id format (expected 'org/model')"));
    }

    // Derive safe local directory name (replace / with _)
    let safe_name = model_id.replace('/', "_");

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(&safe_name);

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip if already installed
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
                "model_dir": model_dir.to_string_lossy(),
            })));
        }
    }

    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    // Construct HuggingFace download URLs
    let base_url = format!("https://huggingface.co/{}/resolve/main", model_id);
    let onnx_filename = if variant == "full" || variant == "fp32" {
        "onnx/model.onnx".to_string()
    } else {
        format!("onnx/model_{variant}.onnx")
    };
    let model_url = format!("{}/{}", base_url, onnx_filename);
    let tokenizer_url = format!("{}/tokenizer.json", base_url);

    let client = reqwest::Client::new();

    // Download tokenizer first (small, ~16 MB)
    tracing::info!(model = %model_id, "downloading tokenizer.json");
    let resp = client.get(&tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("tokenizer download returned {} for {}", resp.status(), tokenizer_url)));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download ONNX model (large, stream to disk)
    tracing::info!(model = %model_id, variant = %variant, "downloading ONNX model");
    let resp = client.get(&model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("model download returned {} for {}", resp.status(), model_url)));
    }

    // Stream response body to file to avoid holding entire model in memory
    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
    }
    file.flush().await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush model failed: {e}")))?;

    let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
    tracing::info!(
        model = %model_id,
        variant = %variant,
        size_mb = size / 1_048_576,
        dir = %model_dir.display(),
        "NER ONNX model downloaded"
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "variant": variant,
        "model_size_mb": size / 1_048_576,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

// ── POST /config/ner-download-onnx — Download legacy ONNX NER model ──

/// Legacy endpoint: download ONNX-format NER model files from HuggingFace URLs.
pub async fn download_ner_model_onnx(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id required"))?;
    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    // Validate URLs point to huggingface.co
    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    // Sanitize model_id (alphanumeric, hyphens, underscores, dots only)
    if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id"));
    }

    // Target: ~/.engram/models/ner/{model_id}/
    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(model_id);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("NER ONNX model already installed ({} MB), skipping download", size / 1_048_576);
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
            })));
        }
    }

    let client = reqwest::Client::new();

    // Download tokenizer first (small)
    tracing::info!("NER: downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download model (large, stream to disk)
    tracing::info!("NER: downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("NER: downloaded {} MB model to {}", downloaded / 1_048_576, model_dir.display());

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "model_dir": model_dir.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
    })))
}

/// GET /config/ner-model?id={model_id} — Check if a NER model is installed.
pub async fn check_ner_model(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let model_id = params.get("id")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "id query parameter required"))?;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(model_id);

    let has_model = model_dir.join("model.onnx").exists();
    let has_tokenizer = model_dir.join("tokenizer.json").exists();
    let ready = has_model && has_tokenizer;

    let model_size = if has_model {
        std::fs::metadata(model_dir.join("model.onnx"))
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "ready": ready,
        "has_model": has_model,
        "has_tokenizer": has_tokenizer,
        "model_size_mb": model_size,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

/// Resolve the engram home directory (~/.engram/).
/// Delegates to the canonical implementation in engram-ingest.
#[cfg(feature = "ingest")]
fn engram_home() -> Option<std::path::PathBuf> {
    engram_ingest::engram_home()
}

/// Resolve the engram home directory (~/.engram/) — fallback when ingest feature is off.
#[cfg(not(feature = "ingest"))]
fn engram_home() -> Option<std::path::PathBuf> {
    std::env::var_os("ENGRAM_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| std::path::PathBuf::from(h).join(".engram"))
        })
}

// ── POST /reindex — Re-embed all nodes ──────────────────────────────

/// POST /reindex — Re-embed all active nodes using the current embedder.
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

// ── Assessment endpoints ─────────────────────────────────────────────

// POST /assessments -- Create assessment
#[cfg(feature = "assess")]
pub async fn create_assessment(
    State(state): State<AppState>,
    Json(req): Json<engram_assess::CreateAssessmentRequest>,
) -> ApiResult<serde_json::Value> {
    let label = format!("Assessment:{}", req.title.to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', ""));

    let initial_prob = req.initial_probability.unwrap_or(0.50).clamp(0.05, 0.95);
    let prov = provenance(&None);

    // Create graph node
    let node_id = {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let slot = g.store_with_confidence(&label, initial_prob, &prov)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let _ = g.set_node_type(&label, "assessment");
        let _ = g.set_property(&label, "title", &req.title);
        if let Some(ref cat) = req.category { let _ = g.set_property(&label, "category", cat); }
        if let Some(ref desc) = req.description { let _ = g.set_property(&label, "description", desc); }
        if let Some(ref tf) = req.timeframe { let _ = g.set_property(&label, "timeframe", tf); }
        let _ = g.set_property(&label, "status", "active");
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", initial_prob));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = g.set_property(&label, "last_evaluated", &now.to_string());

        // Create watch edges
        for entity in &req.watches {
            let _ = g.relate(&label, entity, "watches", &prov);
        }

        slot
    };

    state.mark_dirty();

    // Create sidecar record
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let record = engram_assess::AssessmentRecord {
            label: label.clone(),
            node_id,
            history: vec![engram_assess::ScorePoint {
                timestamp: now,
                probability: initial_prob,
                shift: 0.0,
                trigger: engram_assess::ScoreTrigger::Created,
                reason: "Initial assessment created".to_string(),
                path: None,
            }],
            evidence_for: vec![],
            evidence_against: vec![],
        };

        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        store.insert(record);
    }

    Ok(Json(serde_json::json!({
        "label": label,
        "node_id": node_id,
        "probability": initial_prob,
        "status": "active",
        "watches": req.watches,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn create_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments -- List assessments
#[cfg(feature = "assess")]
pub async fn list_assessments(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;

    let category_filter = params.get("category").filter(|s| !s.is_empty());
    let status_filter = params.get("status").filter(|s| !s.is_empty());

    let mut assessments = Vec::new();
    for record in store.all() {
        let props = g.get_properties(&record.label).ok().flatten().unwrap_or_default();
        let category = props.get("category").cloned().unwrap_or_default();
        let status = props.get("status").cloned().unwrap_or_else(|| "active".to_string());

        if let Some(cf) = category_filter {
            if &category != cf { continue; }
        }
        if let Some(sf) = status_filter {
            if &status != sf { continue; }
        }

        let last_shift = record.history.last().map(|p| p.shift).unwrap_or(0.0);
        let last_evaluated: i64 = props.get("last_evaluated")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let watch_count = g.edges_from(&record.label)
            .unwrap_or_default()
            .iter()
            .filter(|e| e.relationship == "watches")
            .count();

        assessments.push(serde_json::json!({
            "label": record.label,
            "title": props.get("title").cloned().unwrap_or_else(|| record.label.clone()),
            "category": category,
            "status": status,
            "description": props.get("description").cloned().unwrap_or_default(),
            "timeframe": props.get("timeframe").cloned().unwrap_or_default(),
            "current_probability": engram_assess::engine::recalculate_probability(record),
            "last_evaluated": last_evaluated,
            "evidence_count": record.evidence_for.len() + record.evidence_against.len(),
            "watch_count": watch_count,
            "last_shift": last_shift,
        }));
    }

    Ok(Json(serde_json::json!({ "assessments": assessments })))
}

#[cfg(not(feature = "assess"))]
pub async fn list_assessments() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments/:label -- Full detail
#[cfg(feature = "assess")]
pub async fn get_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;

    let record = store.get(&label)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

    let props = g.get_properties(&label).ok().flatten().unwrap_or_default();

    let watches: Vec<String> = g.edges_from(&label)
        .unwrap_or_default()
        .iter()
        .filter(|e| e.relationship == "watches")
        .map(|e| e.to.clone())
        .collect();

    let evidence_for: Vec<serde_json::Value> = record.evidence_for.iter().enumerate().map(|(i, &c)| {
        serde_json::json!({ "node_label": format!("evidence_{}", i), "confidence": c })
    }).collect();

    let evidence_against: Vec<serde_json::Value> = record.evidence_against.iter().enumerate().map(|(i, &c)| {
        serde_json::json!({ "node_label": format!("evidence_{}", i), "confidence": c })
    }).collect();

    Ok(Json(serde_json::json!({
        "label": record.label,
        "title": props.get("title").cloned().unwrap_or_else(|| record.label.clone()),
        "category": props.get("category").cloned().unwrap_or_default(),
        "status": props.get("status").cloned().unwrap_or_else(|| "active".to_string()),
        "description": props.get("description").cloned().unwrap_or_default(),
        "timeframe": props.get("timeframe").cloned().unwrap_or_default(),
        "current_probability": engram_assess::engine::recalculate_probability(record),
        "last_evaluated": props.get("last_evaluated").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0),
        "history": record.history,
        "evidence_for": evidence_for,
        "evidence_against": evidence_against,
        "watches": watches,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn get_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label
#[cfg(feature = "assess")]
pub async fn delete_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    // Remove from graph
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = provenance(&None);
        let _ = g.delete(&label, &prov);
    }
    state.mark_dirty();

    // Remove from sidecar
    {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        store.remove(&label);
    }

    Ok(Json(serde_json::json!({ "deleted": label })))
}

#[cfg(not(feature = "assess"))]
pub async fn delete_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// PATCH /assessments/:label
#[cfg(feature = "assess")]
pub async fn update_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::UpdateAssessmentRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        if let Some(ref title) = req.title { let _ = g.set_property(&label, "title", title); }
        if let Some(ref desc) = req.description { let _ = g.set_property(&label, "description", desc); }
        if let Some(ref cat) = req.category { let _ = g.set_property(&label, "category", cat); }
        if let Some(ref status) = req.status { let _ = g.set_property(&label, "status", status); }
        if let Some(ref tf) = req.timeframe { let _ = g.set_property(&label, "timeframe", tf); }
    }

    // Manual probability override
    if let Some(prob) = req.probability {
        let prob = prob.clamp(0.05, 0.95);
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        if let Some(record) = store.get_mut(&label) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let old_prob = record.history.last().map(|p| p.probability).unwrap_or(0.50);
            record.history.push(engram_assess::ScorePoint {
                timestamp: now,
                probability: prob,
                shift: prob - old_prob,
                trigger: engram_assess::ScoreTrigger::Manual,
                reason: "Manual probability adjustment".to_string(),
                path: None,
            });
        }

        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", prob));
    }

    state.mark_dirty();
    Ok(Json(serde_json::json!({ "updated": label })))
}

#[cfg(not(feature = "assess"))]
pub async fn update_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/evaluate
#[cfg(feature = "assess")]
pub async fn evaluate_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    let point = {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        let record = store.get_mut(&label)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;
        engram_assess::engine::evaluate(record)
    };

    // Update graph property
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", point.probability));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = g.set_property(&label, "last_evaluated", &now.to_string());
    }
    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "label": label,
        "old_probability": point.probability - point.shift,
        "new_probability": point.probability,
        "shift": point.shift,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn evaluate_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/evidence
#[cfg(feature = "assess")]
pub async fn add_assessment_evidence(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::AddEvidenceRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let supports = req.direction == "supports";

    // Get evidence node confidence
    let evidence_conf = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        req.confidence.unwrap_or_else(|| {
            g.get_node(&req.node_label).ok().flatten().map(|n| n.confidence).unwrap_or(0.50)
        })
    };

    // Create evidence edge in graph
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = provenance(&None);
        let edge_type = if supports { "supported_by" } else { "contradicted_by" };
        let _ = g.relate(&req.node_label, &label, edge_type, &prov);
    }

    // Update sidecar
    let point = {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        let record = store.get_mut(&label)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

        let node_id = 0; // We don't have the node_id easily here
        engram_assess::engine::add_evidence(
            record,
            evidence_conf,
            supports,
            engram_assess::ScoreTrigger::EvidenceAdded { node_id },
            format!("Evidence '{}' {} assessment", req.node_label, if supports { "supports" } else { "contradicts" }),
            None,
        )
    };

    // Update graph property
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", point.probability));
    }
    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "added": true,
        "direction": req.direction,
        "new_probability": point.probability,
        "shift": point.shift,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn add_assessment_evidence() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label/evidence/:id
#[cfg(feature = "assess")]
pub async fn remove_assessment_evidence(
    State(state): State<AppState>,
    Path((label, evidence_label)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let evidence_label = urlencoding::decode(&evidence_label).unwrap_or_default().to_string();

    // Remove the evidence edge (supported_by or contradicted_by)
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);

    let mut removed = false;
    let mut was_supporting = true;

    // Try supported_by first
    if g.delete_edge(&evidence_label, &label, "supported_by", &prov).is_ok() {
        removed = true;
        was_supporting = true;
    }
    // Try contradicted_by
    if !removed {
        if g.delete_edge(&evidence_label, &label, "contradicted_by", &prov).is_ok() {
            removed = true;
            was_supporting = false;
        }
    }
    drop(g);

    if removed {
        // Update evidence arrays in sidecar
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        if let Some(record) = store.get_mut(&label) {
            if was_supporting {
                // Remove last evidence_for entry (best effort)
                record.evidence_for.pop();
            } else {
                record.evidence_against.pop();
            }
        }
        state.mark_dirty();
    }

    Ok(Json(serde_json::json!({ "removed": removed, "evidence": evidence_label })))
}

#[cfg(not(feature = "assess"))]
pub async fn remove_assessment_evidence() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments/:label/history
#[cfg(feature = "assess")]
pub async fn assessment_history(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;
    let record = store.get(&label)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

    Ok(Json(serde_json::json!({ "label": label, "history": record.history })))
}

#[cfg(not(feature = "assess"))]
pub async fn assessment_history() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/watch
#[cfg(feature = "assess")]
pub async fn add_assessment_watch(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::AddWatchRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);
    let _ = g.relate(&label, &req.entity_label, "watches", &prov);
    drop(g);
    state.mark_dirty();

    Ok(Json(serde_json::json!({ "added": true, "entity": req.entity_label })))
}

#[cfg(not(feature = "assess"))]
pub async fn add_assessment_watch() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label/watch/:entity
#[cfg(feature = "assess")]
pub async fn remove_assessment_watch(
    State(state): State<AppState>,
    Path((label, entity)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let entity = urlencoding::decode(&entity).unwrap_or_default().to_string();

    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);
    let removed = g.delete_edge(&label, &entity, "watches", &prov).is_ok();
    drop(g);

    if removed {
        state.mark_dirty();
    }

    Ok(Json(serde_json::json!({ "removed": removed, "entity": entity })))
}

#[cfg(not(feature = "assess"))]
pub async fn remove_assessment_watch() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

#[allow(dead_code)]
fn feature_not_enabled(name: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: format!("{name} feature not enabled -- rebuild with --features {name}") }))
}

// ── Secrets endpoints ────────────────────────────────────────────────

// GET /secrets -- List secret keys (never values)
pub async fn list_secrets(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let guard = state.secrets.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref s) = *guard {
        let keys: Vec<&str> = s.keys();
        Ok(Json(serde_json::json!({ "keys": keys })))
    } else {
        Ok(Json(serde_json::json!({ "keys": [], "message": "secrets store not unlocked (admin must login first)" })))
    }
}

// POST /secrets/:key -- Set a secret
pub async fn set_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let value = body.get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "missing 'value' field"))?
        .to_string();

    let mut guard = state.secrets.write().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref mut s) = *guard {
        s.set(&key, value);
        s.save().map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save secrets: {e}"))
        })?;
        Ok(Json(serde_json::json!({ "set": key })))
    } else {
        Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "secrets store not unlocked"))
    }
}

// DELETE /secrets/:key -- Remove a secret
pub async fn delete_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut guard = state.secrets.write().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref mut s) = *guard {
        let removed = s.remove(&key);
        if removed {
            s.save().map_err(|e| {
                api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save secrets: {e}"))
            })?;
        }
        Ok(Json(serde_json::json!({ "deleted": removed, "key": key })))
    } else {
        Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "secrets store not unlocked"))
    }
}

// GET /secrets/:key/check -- Check if a secret exists (never expose value)
pub async fn check_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<serde_json::Value> {
    let guard = state.secrets.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref s) = *guard {
        Ok(Json(serde_json::json!({ "key": key, "exists": s.has(&key) })))
    } else {
        Ok(Json(serde_json::json!({ "key": key, "exists": false })))
    }
}

// ── POST /config/rel-download — Download GLiREL model from HuggingFace ──

pub async fn download_rel_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id required"))?;
    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id"));
    }

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("rel").join(model_id);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("REL model already installed ({} MB), skipping download", size / 1_048_576);
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
            })));
        }
    }

    let client = reqwest::Client::new();

    tracing::info!("REL: downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    tracing::info!("REL: downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("REL: downloaded {} MB model to {}", downloaded / 1_048_576, model_dir.display());

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "model_dir": model_dir.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
    })))
}

/// GET /config/rel-model?id={model_id} — Check if a GLiREL model is installed.
pub async fn check_rel_model(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let model_id = params.get("id")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "id query parameter required"))?;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("rel").join(model_id);

    let has_model = model_dir.join("model.onnx").exists();
    let has_tokenizer = model_dir.join("tokenizer.json").exists();
    let ready = has_model && has_tokenizer;

    let model_size = if has_model {
        std::fs::metadata(model_dir.join("model.onnx"))
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "ready": ready,
        "has_model": has_model,
        "has_tokenizer": has_tokenizer,
        "model_size_mb": model_size,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

// ── GET /config/relation-templates/export — Export configured + learned relation templates ──

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
    if let Some(ref config_path) = state.config_path {
        // Derive brain path from config path (config is .brain.config, brain is .brain)
        let brain_path = config_path.with_extension("");
        let relgaz_path = brain_path.with_extension("relgaz");
        if relgaz_path.exists() {
            if let Ok(gaz) = engram_ingest::RelationGazetteer::load(&brain_path) {
                for rt in gaz.known_relation_types() {
                    if !configured.contains_key(rt) {
                        learned_types.push(rt.clone());
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

// ── POST /config/relation-templates/import — Import relation templates ──

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

// ── POST /config/model-upload — Upload model files for air-gapped systems ──

/// Upload model files via multipart/form-data for air-gapped systems.
///
/// Fields:
/// - `model_type`: "embed" | "ner" | "rel"
/// - `model_id`: directory name (e.g. "multilingual-MiniLMv2-L6-mnli-xnli")
/// - File fields: saved to `~/.engram/models/{type}/{id}/{filename}`
pub async fn upload_model(
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;

    let mut model_type: Option<String> = None;
    let mut model_id: Option<String> = None;
    let mut files_written: Vec<String> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut model_dir: Option<std::path::PathBuf> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("multipart read error: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "model_type" => {
                let val = field.text().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read model_type: {e}")))?;
                if !matches!(val.as_str(), "embed" | "ner" | "rel") {
                    return Err(api_err(StatusCode::BAD_REQUEST, "model_type must be 'embed', 'ner', or 'rel'"));
                }
                model_type = Some(val);
            }
            "model_id" => {
                let val = field.text().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read model_id: {e}")))?;
                if val.contains("..") || val.contains('/') || val.contains('\\') {
                    return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id (no path traversal)"));
                }
                model_id = Some(val);
            }
            _ => {
                // File field — save to model directory
                let mt = model_type.as_ref()
                    .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_type must be sent before file fields"))?;
                let mid = model_id.as_ref()
                    .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id must be sent before file fields"))?;

                let dir = home.join("models").join(mt).join(mid);
                if model_dir.is_none() {
                    tokio::fs::create_dir_all(&dir).await
                        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir: {e}")))?;
                    model_dir = Some(dir.clone());
                }

                let filename = field.file_name().unwrap_or(&field_name).to_string();
                if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                    return Err(api_err(StatusCode::BAD_REQUEST, format!("invalid filename: {filename}")));
                }

                let file_path = dir.join(&filename);
                let data = field.bytes().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read file {filename}: {e}")))?;

                let mut f = tokio::fs::File::create(&file_path).await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create {filename}: {e}")))?;
                f.write_all(&data).await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
                f.flush().await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush {filename}: {e}")))?;

                total_bytes += data.len() as u64;
                files_written.push(filename);
                tracing::info!(file = %file_path.display(), bytes = data.len(), "model file uploaded");
            }
        }
    }

    if files_written.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "no files uploaded"));
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_type": model_type,
        "model_id": model_id,
        "files": files_written,
        "total_bytes": total_bytes,
        "model_dir": model_dir.map(|d| d.to_string_lossy().to_string()),
    })))
}

/// POST /kge/train — Trigger KGE (RotatE) training on current graph.
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

// ── POST /admin/reset ──

pub async fn admin_reset(
    State(state): State<AppState>,
) -> ApiResult<ResetResponse> {
    let brain_path = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        g.path().to_path_buf()
    };

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        g.reset().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        g.checkpoint().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let sidecars_cleaned = cleanup_sidecars(&brain_path);

    Ok(Json(ResetResponse {
        success: true,
        sidecars_cleaned,
    }))
}

fn cleanup_sidecars(brain_path: &std::path::Path) -> Vec<String> {
    let mut cleaned = Vec::new();
    let delete_extensions = [
        "props", "types", "vectors", "cooccur", "wal", "rules",
        "schedules", "peers", "audit", "assessments", "relgaz", "kge",
        "ledger",
    ];
    for ext in &delete_extensions {
        let sidecar = brain_path.with_extension(ext);
        if sidecar.exists() {
            if std::fs::remove_file(&sidecar).is_ok() {
                cleaned.push(ext.to_string());
            }
        }
    }
    cleaned
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
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
