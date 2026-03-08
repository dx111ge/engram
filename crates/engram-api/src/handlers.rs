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

    let mut nodes_stored: u32 = 0;
    let mut edges_created: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    // Process entity stores
    if let Some(entities) = req.entities {
        for entity in entities {
            let result = if let Some(conf) = entity.confidence {
                g.store_with_confidence(&entity.entity, conf, &prov)
            } else {
                g.store(&entity.entity, &prov)
            };
            match result {
                Ok(_slot) => {
                    if let Some(ref t) = entity.entity_type {
                        let _ = g.set_node_type(&entity.entity, t);
                    }
                    if let Some(ref props) = entity.properties {
                        for (k, v) in props {
                            let _ = g.set_property(&entity.entity, k, v);
                        }
                    }
                    nodes_stored += 1;
                }
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

    let result = g
        .traverse(&req.start, depth, min_conf)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;

    let mut nodes = Vec::new();
    for &nid in &result.nodes {
        if let Ok(Some(node)) = g.get_node_by_id(nid) {
            nodes.push(NodeHit {
                node_id: nid,
                label: node.label().to_string(),
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
        .search_text(&req.query, limit)
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
