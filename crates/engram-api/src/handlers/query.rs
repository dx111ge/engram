use super::*;

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
                valid_from: ev.valid_from,
                valid_to: ev.valid_to,
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
            valid_from: e.valid_from,
            valid_to: e.valid_to,
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
            valid_from: e.valid_from,
            valid_to: e.valid_to,
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

// ── POST /paths ──

pub async fn find_paths(
    State(state): State<AppState>,
    Json(req): Json<PathsRequest>,
) -> ApiResult<PathsResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let max_depth = req.max_depth.unwrap_or(5).min(8); // Cap at 8 to prevent explosion

    let paths = g
        .find_all_paths(&req.from, &req.to, max_depth, req.min_depth, req.via.as_deref())
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;

    let count = paths.len();
    Ok(Json(PathsResponse { paths, count }))
}
