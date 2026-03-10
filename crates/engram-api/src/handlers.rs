/// HTTP handlers for the REST API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use engram_core::graph::Provenance;

use crate::natural;
use crate::state::AppState;
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

    let mut nodes = Vec::new();
    for &nid in &result.nodes {
        if let Ok(Some(node)) = g.get_node_by_id(nid) {
            nodes.push(NodeHit {
                node_id: nid,
                label: g.label_for_id(nid).unwrap_or_else(|_| node.label().to_string()),
                confidence: node.confidence,
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
        .map(|r| NodeHit {
            node_id: r.node_id,
            label: r.label,
            confidence: r.confidence,
            score: Some(r.score),
            depth: None,
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
        .map(|r| NodeHit {
            node_id: r.node_id,
            label: r.label,
            confidence: r.confidence,
            score: Some(r.score),
            depth: None,
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
        })
        .collect();

    Ok(Json(NodeResponse {
        node_id,
        label,
        confidence,
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
    Json(state.compute.clone())
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
    let mut nodes_imported: u32 = 0;
    let mut edges_imported: u32 = 0;
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

    // First pass: create all nodes
    for item in &items {
        let label = extract_label(item);
        if label.is_empty() {
            continue;
        }

        // Get confidence if present
        let conf = item.get("engram:confidence")
            .or_else(|| item.get("confidence"))
            .and_then(|v| v.as_f64())
            .map(|c| c as f32);

        let store_result = if let Some(c) = conf {
            g.store_with_confidence(&label, c, &prov)
        } else {
            g.store(&label, &prov)
        };

        match store_result {
            Ok(_slot) => {
                nodes_imported += 1;

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
                        // If value is a string, treat as property
                        if let Some(s) = v.as_str() {
                            let prop_key = strip_prefix(k);
                            if !prop_key.is_empty() {
                                let _ = g.set_property(&label, &prop_key, s);
                            }
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("store {label}: {e}")),
        }
    }

    // Second pass: create edges (relationships to other nodes)
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
                // Check if value is a reference (object with @id) or array of references
                let targets = if v.is_array() {
                    v.as_array().unwrap().clone()
                } else if v.is_object() {
                    vec![v.clone()]
                } else {
                    continue; // string properties already handled
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
                        // Get edge confidence if present
                        let conf = target.get("engram:confidence")
                            .or_else(|| target.get("confidence"))
                            .and_then(|v| v.as_f64())
                            .map(|c| c as f32);
                        match g.relate_with_confidence(&from_label, &to_label, &rel, conf.unwrap_or(0.8), &prov) {
                            Ok(_) => { edges_imported += 1; }
                            Err(e) => errors.push(format!("relate {from_label} -> {to_label}: {e}")),
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

    // Primary: SearXNG (self-hosted meta search, no API key needed)
    let searxng_base = std::env::var("ENGRAM_SEARXNG_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());
    // Optional time_range: "day", "week", "month", "year"
    let time_range = params.get("time_range").cloned().unwrap_or_default();
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
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    use axum::response::IntoResponse;

    let endpoint = std::env::var("ENGRAM_LLM_ENDPOINT")
        .or_else(|_| std::env::var("ENGRAM_EMBED_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
    let api_key = std::env::var("ENGRAM_LLM_API_KEY")
        .or_else(|_| std::env::var("ENGRAM_EMBED_API_KEY"))
        .unwrap_or_default();
    let default_model = std::env::var("ENGRAM_LLM_MODEL")
        .unwrap_or_else(|_| "llama3.2".to_string());

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

    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));

    let request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
    });

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

#[cfg(feature = "ingest")]
pub async fn ingest(
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> ApiResult<IngestResponse> {
    use engram_ingest::{Pipeline, PipelineConfig, types::StageConfig};

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

    let pipeline = Pipeline::new(state.graph.clone(), config);

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

    // Execute pipeline
    let result = if req.parallel.unwrap_or(false) {
        pipeline.execute_parallel(items)
    } else {
        pipeline.execute(items)
    }
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
    }))
}

#[cfg(not(feature = "ingest"))]
pub async fn ingest() -> impl axum::response::IntoResponse {
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
    use engram_ingest::{Pipeline, PipelineConfig, types::StageConfig};

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

    let pipeline = Pipeline::new(state.graph.clone(), config);

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

    let parallel = params.get("parallel").is_some_and(|v| v == "true" || v == "1");
    let result = if parallel {
        pipeline.execute_parallel(items)
    } else {
        pipeline.execute(items)
    }
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
    Json(req): Json<IngestConfigureRequest>,
) -> ApiResult<serde_json::Value> {
    // For now, return the effective config. Runtime config persistence comes
    // with Phase 8.17+ (source trait).
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
pub async fn list_sources() -> ApiResult<serde_json::Value> {
    // Source registry will be wired to AppState in a future phase.
    // For now, return an empty list with the endpoint shape.
    Ok(Json(serde_json::json!({
        "sources": serde_json::Value::Array(vec![]),
        "note": "source registry not yet wired to server state"
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn list_sources() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}

// ── GET /sources/{name}/usage ──

#[cfg(feature = "ingest")]
pub async fn source_usage(
    Path(name): Path<String>,
) -> ApiResult<serde_json::Value> {
    Ok(Json(serde_json::json!({
        "source": name,
        "usage": {
            "requests": 0,
            "items": 0,
            "errors": 0,
            "cost": 0.0
        },
        "note": "source registry not yet wired to server state"
    })))
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
    if removed {
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
    use engram_ingest::pipeline::{Pipeline, PipelineConfig};

    let config = PipelineConfig::default();
    let pipeline = Pipeline::new(config);

    let graph = state.graph.clone();
    let results = pipeline.execute(&body, &pipeline_id, &graph)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "pipeline": pipeline_id,
        "entities": results.entities_stored,
        "relations": results.relations_stored,
    })))
}

#[cfg(not(feature = "ingest"))]
pub async fn webhook_receive() -> impl axum::response::IntoResponse {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: "ingest feature not enabled".into() }))
}
