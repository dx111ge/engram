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

fn lock_err() -> (StatusCode, Json<ErrorResponse>) {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph lock poisoned")
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
    let mut g = state.graph.lock().map_err(|_| lock_err())?;
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
    let mut g = state.graph.lock().map_err(|_| lock_err())?;
    let prov = provenance(&None);

    let edge_slot = if let Some(conf) = req.confidence {
        g.relate_with_confidence(&req.from, &req.to, &req.relationship, conf, &prov)
    } else {
        g.relate(&req.from, &req.to, &req.relationship, &prov)
    }
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RelateResponse {
        from: req.from,
        to: req.to,
        relationship: req.relationship,
        edge_slot,
    }))
}

// ── POST /query ──

pub async fn query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> ApiResult<QueryResponse> {
    let g = state.graph.lock().map_err(|_| lock_err())?;

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
        .filter_map(|&(from_id, to_id, _edge_slot)| {
            let from_node = g.get_node_by_id(from_id).ok()??;
            let to_node = g.get_node_by_id(to_id).ok()??;
            Some(EdgeResponse {
                from: from_node.label().to_string(),
                to: to_node.label().to_string(),
                relationship: String::new(),
                confidence: 0.0,
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
    let g = state.graph.lock().map_err(|_| lock_err())?;
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
    let g = state.graph.lock().map_err(|_| lock_err())?;
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
    let g = state.graph.lock().map_err(|_| lock_err())?;

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
    let mut g = state.graph.lock().map_err(|_| lock_err())?;
    let prov = Provenance::user("api");

    let deleted = g
        .delete(&label, &prov)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    let mut g = state.graph.lock().map_err(|_| lock_err())?;

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
    let mut g = state.graph.lock().map_err(|_| lock_err())?;
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

    Ok(Json(CorrectResponse {
        corrected: req.entity,
        propagated_to,
    }))
}

// ── POST /learn/decay ──

pub async fn decay(State(state): State<AppState>) -> ApiResult<DecayResponse> {
    let mut g = state.graph.lock().map_err(|_| lock_err())?;

    let nodes_decayed = g
        .apply_decay()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(DecayResponse { nodes_decayed }))
}

// ── POST /learn/derive ──

pub async fn derive(
    State(state): State<AppState>,
    Json(req): Json<DeriveRequest>,
) -> ApiResult<DeriveResponse> {
    let mut g = state.graph.lock().map_err(|_| lock_err())?;

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
    let g = state.graph.lock().map_err(|_| lock_err())?;
    let (nodes, edges) = g.stats();
    Ok(Json(StatsResponse { nodes, edges }))
}

// ── GET /explain/{label} ──

pub async fn explain(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<ExplainResponse> {
    let g = state.graph.lock().map_err(|_| lock_err())?;

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
    let g = state.graph.lock().map_err(|_| lock_err())?;
    Ok(Json(natural::handle_ask(&g, &req.question)))
}

// ── POST /tell ──

pub async fn tell(
    State(state): State<AppState>,
    Json(req): Json<natural::TellRequest>,
) -> ApiResult<natural::TellResponse> {
    let mut g = state.graph.lock().map_err(|_| lock_err())?;
    Ok(Json(natural::handle_tell(
        &mut g,
        &req.statement,
        req.source.as_deref(),
    )))
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
