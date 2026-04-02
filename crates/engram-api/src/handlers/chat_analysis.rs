/// Chat analysis handlers: compare, shortest_path, most_connected, isolated,
/// what_if, influence_path.

use axum::extract::State;
use axum::Json;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::state::AppState;
use super::{api_err, read_lock_err, ApiResult};
use super::chat::{
    CompareRequest, CompareResponse, EntitySummary, ShortestPathRequest, PathResponse,
    PathStep, MostConnectedRequest, ConnectedNode, IsolatedRequest, WhatIfRequest,
    WhatIfResponse, AffectedEntity, InfluencePathRequest,
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
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(10).min(50);

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

    Ok(Json(serde_json::json!({ "entities": nodes })))
}

/// POST /chat/isolated -- nodes with few/no connections
pub async fn isolated(
    State(state): State<AppState>,
    Json(req): Json<IsolatedRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let max_edges = req.max_edges.unwrap_or(1);

    let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut nodes: Vec<ConnectedNode> = all.into_iter()
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

    nodes.sort_by(|a, b| a.edge_count.cmp(&b.edge_count));
    nodes.truncate(50);

    Ok(Json(serde_json::json!({ "entities": nodes })))
}

/// POST /chat/what_if -- confidence cascade simulation
/// POST /chat/what_if -- 2-hop confidence cascade simulation
pub async fn what_if(
    State(state): State<AppState>,
    Json(req): Json<WhatIfRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let new_conf = req.new_confidence.unwrap_or(0.0) as f32;
    let max_depth = req.depth.unwrap_or(2).min(3);

    let current = g.node_confidence(&req.entity)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or(0.0);

    let conf_delta = new_conf - current;

    // BFS cascade: propagate confidence change through 2 hops
    let mut affected: Vec<serde_json::Value> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(req.entity.clone());

    // Queue: (label, depth, propagated_delta)
    let mut queue: VecDeque<(String, u32, f32)> = VecDeque::new();

    // Seed: direct neighbors at hop 1
    let edges_out = g.edges_from(&req.entity).unwrap_or_default();
    let edges_in = g.edges_to(&req.entity).unwrap_or_default();

    for edge in edges_out.iter().chain(edges_in.iter()) {
        let neighbor = if edge.from == req.entity { &edge.to } else { &edge.from };
        if visited.insert(neighbor.clone()) {
            // Propagated delta = original delta * edge confidence * decay
            let propagated = conf_delta * edge.confidence * 0.5;
            let neighbor_conf = g.node_confidence(neighbor).ok().flatten().unwrap_or(0.0);
            let simulated = (neighbor_conf + propagated).clamp(0.05, 0.95);

            affected.push(serde_json::json!({
                "label": neighbor,
                "relationship": edge.relationship,
                "current_confidence": neighbor_conf,
                "simulated_confidence": simulated,
                "delta": simulated - neighbor_conf,
                "hop": 1,
                "node_type": g.get_node_type(neighbor),
            }));

            if max_depth > 1 {
                queue.push_back((neighbor.clone(), 1, propagated));
            }
        }
    }

    // Hop 2+: cascade through neighbors of neighbors
    while let Some((label, depth, parent_delta)) = queue.pop_front() {
        if depth >= max_depth { continue; }
        let hop_edges_out = g.edges_from(&label).unwrap_or_default();
        let hop_edges_in = g.edges_to(&label).unwrap_or_default();

        for edge in hop_edges_out.iter().chain(hop_edges_in.iter()) {
            let neighbor = if edge.from == label { &edge.to } else { &edge.from };
            if visited.insert(neighbor.clone()) {
                let propagated = parent_delta * edge.confidence * 0.5;
                if propagated.abs() < 0.01 { continue; } // Skip negligible impacts

                let neighbor_conf = g.node_confidence(neighbor).ok().flatten().unwrap_or(0.0);
                let simulated = (neighbor_conf + propagated).clamp(0.05, 0.95);

                affected.push(serde_json::json!({
                    "label": neighbor,
                    "relationship": edge.relationship,
                    "current_confidence": neighbor_conf,
                    "simulated_confidence": simulated,
                    "delta": simulated - neighbor_conf,
                    "hop": depth + 1,
                    "node_type": g.get_node_type(neighbor),
                }));

                if depth + 1 < max_depth {
                    queue.push_back((neighbor.clone(), depth + 1, propagated));
                }
            }
        }
    }

    // Sort by absolute delta (biggest impact first), limit
    affected.sort_by(|a, b| {
        let da = a.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0).abs();
        let db = b.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0).abs();
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    affected.truncate(30);

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "current_confidence": current,
        "simulated_confidence": new_conf,
        "confidence_delta": conf_delta,
        "affected": affected,
        "affected_count": affected.len(),
        "max_depth": max_depth,
    })))
}

/// POST /chat/influence_path -- find ALL paths between two entities (multi-path)
pub async fn influence_path(
    State(state): State<AppState>,
    Json(req): Json<InfluencePathRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let max_depth = req.max_depth.unwrap_or(4).min(5);

    let from_id = match g.find_node_id(&req.from).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(id) => id,
        None => return Ok(Json(serde_json::json!({ "from": req.from, "to": req.to, "found": false, "paths": [] }))),
    };
    let to_id = match g.find_node_id(&req.to).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(id) => id,
        None => return Ok(Json(serde_json::json!({ "from": req.from, "to": req.to, "found": false, "paths": [] }))),
    };

    // DFS to find all paths up to max_depth
    let result = g.traverse_directed(&req.from, max_depth, 0.0, "both")
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !result.nodes.contains(&to_id) {
        return Ok(Json(serde_json::json!({ "from": req.from, "to": req.to, "found": false, "paths": [] })));
    }

    // BFS collecting ALL paths (not just shortest)
    let mut all_paths: Vec<Vec<(String, String)>> = Vec::new(); // Vec of (entity, relationship) pairs
    let mut queue: VecDeque<(u64, Vec<(String, String)>, HashSet<u64>)> = VecDeque::new();

    let mut start_visited = HashSet::new();
    start_visited.insert(from_id);
    queue.push_back((from_id, vec![(req.from.clone(), String::new())], start_visited));

    while let Some((current, path, visited)) = queue.pop_front() {
        if path.len() > max_depth as usize + 1 { continue; }
        if all_paths.len() >= 5 { break; } // Cap at 5 paths

        for &(src, dst, edge_slot) in &result.edges {
            let next = if src == current { dst } else if dst == current { src } else { continue };
            if visited.contains(&next) { continue; }

            let edge_view = match g.read_edge_view(edge_slot) {
                Ok(ev) => ev,
                Err(_) => continue,
            };
            let next_label = match g.label_for_id(next) {
                Ok(l) => l,
                Err(_) => continue,
            };

            let mut new_path = path.clone();
            new_path.push((next_label.clone(), edge_view.relationship.clone()));

            if next == to_id {
                all_paths.push(new_path);
            } else {
                let mut new_visited = visited.clone();
                new_visited.insert(next);
                queue.push_back((next, new_path, new_visited));
            }
        }
    }

    // Format paths for response
    let paths: Vec<serde_json::Value> = all_paths.iter().map(|path| {
        let steps: Vec<serde_json::Value> = path.iter().map(|(entity, rel)| {
            serde_json::json!({ "entity": entity, "relationship": rel })
        }).collect();
        serde_json::json!({ "hops": path.len() - 1, "steps": steps })
    }).collect();

    Ok(Json(serde_json::json!({
        "from": req.from,
        "to": req.to,
        "found": !all_paths.is_empty(),
        "path_count": all_paths.len(),
        "paths": paths,
    })))
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
