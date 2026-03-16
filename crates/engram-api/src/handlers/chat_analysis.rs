/// Chat analysis handlers: compare, shortest_path, most_connected, isolated,
/// what_if, influence_path, briefing, export_subgraph, entity_timeline.

use axum::extract::State;
use axum::Json;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::state::AppState;
use super::{api_err, read_lock_err, ApiResult};
use super::chat::{
    CompareRequest, CompareResponse, EntitySummary, ShortestPathRequest, PathResponse,
    PathStep, MostConnectedRequest, ConnectedNode, IsolatedRequest, WhatIfRequest,
    WhatIfResponse, AffectedEntity, InfluencePathRequest, BriefingRequest,
    ExportSubgraphRequest, EntityTimelineRequest,
};

/// POST /chat/compare -- side-by-side entity comparison
pub async fn compare(
    State(state): State<AppState>,
    Json(req): Json<CompareRequest>,
) -> ApiResult<CompareResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let summary_a = entity_summary(&g, &req.entity_a)?;
    let summary_b = entity_summary(&g, &req.entity_b)?;

    let neighbors_a = get_neighbors(&g, &req.entity_a)?;
    let neighbors_b = get_neighbors(&g, &req.entity_b)?;

    let set_a: HashSet<&str> = neighbors_a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = neighbors_b.iter().map(|s| s.as_str()).collect();

    let shared: Vec<String> = set_a.intersection(&set_b).map(|s| s.to_string()).collect();
    let unique_a: Vec<String> = set_a.difference(&set_b).map(|s| s.to_string()).collect();
    let unique_b: Vec<String> = set_b.difference(&set_a).map(|s| s.to_string()).collect();

    Ok(Json(CompareResponse {
        entity_a: summary_a,
        entity_b: summary_b,
        shared_neighbors: shared,
        unique_to_a: unique_a,
        unique_to_b: unique_b,
    }))
}

/// POST /chat/shortest_path -- BFS shortest path
pub async fn shortest_path(
    State(state): State<AppState>,
    Json(req): Json<ShortestPathRequest>,
) -> ApiResult<PathResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let max_depth = req.max_depth.unwrap_or(6);

    let from_id = match g.find_node_id(&req.from).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(id) => id,
        None => return Ok(Json(PathResponse { found: false, path: Vec::new(), length: 0 })),
    };
    let to_id = match g.find_node_id(&req.to).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(id) => id,
        None => return Ok(Json(PathResponse { found: false, path: Vec::new(), length: 0 })),
    };

    let result = g.traverse_directed(&req.from, max_depth, 0.0, "both")
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !result.nodes.contains(&to_id) {
        return Ok(Json(PathResponse { found: false, path: Vec::new(), length: 0 }));
    }

    // BFS parent tracking for path reconstruction
    let mut parent: HashMap<u64, (u64, u64)> = HashMap::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(from_id);
    queue.push_back(from_id);

    'bfs: while let Some(current) = queue.pop_front() {
        for &(src, dst, edge_slot) in &result.edges {
            let next = if src == current { dst } else if dst == current { src } else { continue };
            if visited.insert(next) {
                parent.insert(next, (current, edge_slot));
                if next == to_id { break 'bfs; }
                queue.push_back(next);
            }
        }
    }

    let mut path = Vec::new();
    let mut current = to_id;
    while let Some(&(prev, edge_slot)) = parent.get(&current) {
        let edge_view = g.read_edge_view(edge_slot)
            .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let label = g.label_for_id(current).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        path.push(PathStep {
            entity: label,
            relationship: edge_view.relationship,
            direction: if edge_view.from == g.label_for_id(prev).unwrap_or_default() { "->".to_string() } else { "<-".to_string() },
        });
        current = prev;
    }
    path.reverse();
    let length = path.len() as u32;

    Ok(Json(PathResponse { found: true, path, length }))
}

/// POST /chat/most_connected -- top-N by edge count
pub async fn most_connected(
    State(state): State<AppState>,
    Json(req): Json<MostConnectedRequest>,
) -> ApiResult<Vec<ConnectedNode>> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(10);

    let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut nodes: Vec<ConnectedNode> = all.into_iter()
        .filter(|n| {
            req.node_type.as_ref().map_or(true, |nt| n.node_type.as_deref() == Some(nt.as_str()))
        })
        .map(|n| ConnectedNode {
            label: n.label,
            node_type: n.node_type,
            confidence: n.confidence,
            edge_count: n.edge_out_count as u32 + n.edge_in_count as u32,
        })
        .collect();

    nodes.sort_by(|a, b| b.edge_count.cmp(&a.edge_count));
    nodes.truncate(limit);

    Ok(Json(nodes))
}

/// POST /chat/isolated -- nodes with few/no connections
pub async fn isolated(
    State(state): State<AppState>,
    Json(req): Json<IsolatedRequest>,
) -> ApiResult<Vec<ConnectedNode>> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let max_edges = req.max_edges.unwrap_or(1);

    let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let nodes: Vec<ConnectedNode> = all.into_iter()
        .filter(|n| {
            let total = n.edge_out_count as u32 + n.edge_in_count as u32;
            total <= max_edges && req.node_type.as_ref().map_or(true, |nt| n.node_type.as_deref() == Some(nt.as_str()))
        })
        .map(|n| ConnectedNode {
            label: n.label,
            node_type: n.node_type,
            confidence: n.confidence,
            edge_count: n.edge_out_count as u32 + n.edge_in_count as u32,
        })
        .collect();

    Ok(Json(nodes))
}

/// POST /chat/what_if -- confidence cascade simulation
pub async fn what_if(
    State(state): State<AppState>,
    Json(req): Json<WhatIfRequest>,
) -> ApiResult<WhatIfResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let new_conf = req.new_confidence.unwrap_or(0.0) as f32;

    let current = g.node_confidence(&req.entity)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or(0.0);

    let edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let diff = new_conf - current;
    let impact_desc = if diff.abs() < 0.1 { "minimal" }
        else if diff < -0.3 { "significant decrease" }
        else if diff > 0.3 { "significant increase" }
        else if diff < 0.0 { "moderate decrease" }
        else { "moderate increase" };

    let mut affected = Vec::new();
    for edge in edges_out.iter().chain(edges_in.iter()) {
        let neighbor = if edge.from == req.entity { &edge.to } else { &edge.from };
        let neighbor_conf = g.node_confidence(neighbor)
            .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .unwrap_or(0.0);
        affected.push(AffectedEntity {
            label: neighbor.clone(),
            relationship: edge.relationship.clone(),
            current_confidence: neighbor_conf,
            impact: format!("Indirect {impact_desc} via {} edge", edge.relationship),
        });
    }

    Ok(Json(WhatIfResponse {
        entity: req.entity,
        current_confidence: current,
        simulated_confidence: new_conf,
        affected,
    }))
}

/// POST /chat/influence_path -- how A affects B
pub async fn influence_path(
    State(state): State<AppState>,
    Json(req): Json<InfluencePathRequest>,
) -> ApiResult<PathResponse> {
    shortest_path(
        State(state),
        Json(ShortestPathRequest {
            from: req.from,
            to: req.to,
            max_depth: req.max_depth.or(Some(5)),
        }),
    ).await
}

/// POST /chat/briefing -- structured briefing on topic
pub async fn briefing(
    State(state): State<AppState>,
    Json(req): Json<BriefingRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let depth = match req.depth.as_deref() {
        Some("shallow") => 1u32,
        Some("deep") => 3,
        _ => 2,
    };

    let hits = g.search_text(&req.topic, 5)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if hits.is_empty() {
        return Ok(Json(serde_json::json!({
            "topic": req.topic,
            "status": "no_data",
            "message": format!("No entities found matching '{}'", req.topic),
        })));
    }

    let primary = &hits[0];
    let result = g.traverse(&primary.label, depth, 0.0)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut entities = Vec::new();
    for &nid in &result.nodes {
        let label = g.label_for_id(nid).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let conf = g.node_confidence(&label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
        let nt = g.get_node_type(&label);
        entities.push(serde_json::json!({ "label": label, "type": nt, "confidence": conf }));
    }

    let mut edges = Vec::new();
    for &(_, _, slot) in &result.edges {
        let ev = g.read_edge_view(slot).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        edges.push(serde_json::json!({
            "from": ev.from, "to": ev.to, "relationship": ev.relationship,
            "confidence": ev.confidence, "valid_from": ev.valid_from, "valid_to": ev.valid_to,
        }));
    }

    Ok(Json(serde_json::json!({
        "topic": req.topic, "primary_entity": primary.label,
        "entities": entities, "relationships": edges,
        "entity_count": entities.len(), "relationship_count": edges.len(),
    })))
}

/// POST /chat/export_subgraph -- export neighborhood
pub async fn export_subgraph(
    State(state): State<AppState>,
    Json(req): Json<ExportSubgraphRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let depth = req.depth.unwrap_or(2);

    let result = g.traverse(&req.entity, depth, 0.0)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut nodes = Vec::new();
    for &nid in &result.nodes {
        let label = g.label_for_id(nid).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let conf = g.node_confidence(&label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
        let nt = g.get_node_type(&label);
        let props = g.get_properties(&label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or_default();
        nodes.push(serde_json::json!({ "label": label, "type": nt, "confidence": conf, "properties": props }));
    }

    let mut edges = Vec::new();
    for &(_, _, slot) in &result.edges {
        let ev = g.read_edge_view(slot).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        edges.push(serde_json::json!({
            "from": ev.from, "to": ev.to, "relationship": ev.relationship,
            "confidence": ev.confidence, "valid_from": ev.valid_from, "valid_to": ev.valid_to,
        }));
    }

    Ok(Json(serde_json::json!({ "center": req.entity, "depth": depth, "nodes": nodes, "edges": edges })))
}

/// POST /chat/entity_timeline -- chronological narrative
pub async fn entity_timeline(
    State(state): State<AppState>,
    Json(req): Json<EntityTimelineRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let mut all_edges = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all_edges.extend(edges_in);

    all_edges.sort_by(|a, b| {
        a.valid_from.as_deref().unwrap_or("9999").cmp(b.valid_from.as_deref().unwrap_or("9999"))
    });

    let events: Vec<serde_json::Value> = all_edges.into_iter()
        .filter(|e| {
            if let Some(ref from) = req.from_date {
                if let Some(ref vf) = e.valid_from {
                    if vf.as_str() < from.as_str() { return false; }
                }
            }
            if let Some(ref to) = req.to_date {
                if let Some(ref vf) = e.valid_from {
                    if vf.as_str() > to.as_str() { return false; }
                }
            }
            true
        })
        .map(|e| serde_json::json!({
            "from": e.from, "to": e.to, "relationship": e.relationship,
            "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to,
        }))
        .collect();

    Ok(Json(serde_json::json!({ "entity": req.entity, "events": events, "event_count": events.len() })))
}

// ── Helpers ──

fn entity_summary(
    g: &engram_core::graph::Graph,
    label: &str,
) -> Result<EntitySummary, (axum::http::StatusCode, Json<crate::types::ErrorResponse>)> {
    let conf = g.node_confidence(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
    let nt = g.get_node_type(label);
    let props = g.get_properties(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or_default();
    let edges_out = g.edges_from(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(EntitySummary {
        label: label.to_string(), node_type: nt, confidence: conf,
        edge_count: (edges_out.len() + edges_in.len()) as u32, properties: props,
    })
}

fn get_neighbors(
    g: &engram_core::graph::Graph,
    label: &str,
) -> Result<Vec<String>, (axum::http::StatusCode, Json<crate::types::ErrorResponse>)> {
    let mut neighbors = HashSet::new();
    let edges_out = g.edges_from(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(label).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for e in &edges_out { neighbors.insert(e.to.clone()); }
    for e in &edges_in { neighbors.insert(e.from.clone()); }
    Ok(neighbors.into_iter().collect())
}
