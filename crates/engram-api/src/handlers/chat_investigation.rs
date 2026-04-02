/// Chat investigation handlers: network analysis, entity 360-degree view,
/// and entity gap detection.

use axum::extract::State;
use axum::Json;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::state::AppState;
use super::{api_err, read_lock_err, ApiResult};

/// POST /chat/network_analysis -- N-hop connectivity map
pub async fn network_analysis(
    State(state): State<AppState>,
    Json(req): Json<super::chat::NetworkAnalysisRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let depth = req.depth.unwrap_or(2).min(4);

    let result = g.traverse_directed(&req.entity, depth, 0.0, "both")
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Group nodes by hop distance using BFS
    let start_id = match g.find_node_id(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(id) => id,
        None => return Ok(Json(serde_json::json!({
            "entity": req.entity, "error": format!("Entity '{}' not found", req.entity),
            "layers": [], "total_nodes": 0, "total_edges": 0,
        }))),
    };

    let mut distances: HashMap<u64, u32> = HashMap::new();
    let mut queue = VecDeque::new();
    distances.insert(start_id, 0);
    queue.push_back(start_id);

    while let Some(current) = queue.pop_front() {
        let d = distances[&current];
        if d >= depth { continue; }
        for &(src, dst, _) in &result.edges {
            let next = if src == current { dst } else if dst == current { src } else { continue };
            if !distances.contains_key(&next) {
                distances.insert(next, d + 1);
                queue.push_back(next);
            }
        }
    }

    // Build layers
    let mut layers: Vec<serde_json::Value> = Vec::new();
    for hop in 1..=depth {
        let nodes_at_hop: Vec<serde_json::Value> = distances.iter()
            .filter(|(_, d)| **d == hop)
            .filter_map(|(nid, _)| {
                let nid = *nid;
                let label = g.label_for_id(nid).ok()?;
                let nt = g.get_node_type(&label);
                let conf = g.node_confidence(&label).ok().flatten().unwrap_or(0.0);
                Some(serde_json::json!({ "label": label, "node_type": nt, "confidence": conf }))
            })
            .take(20)
            .collect();
        if !nodes_at_hop.is_empty() {
            layers.push(serde_json::json!({ "hop": hop, "count": nodes_at_hop.len(), "nodes": nodes_at_hop }));
        }
    }

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "depth": depth,
        "layers": layers,
        "total_nodes": result.nodes.len(),
        "total_edges": result.edges.len(),
    })))
}

/// POST /chat/entity_360 -- everything about an entity in one view
pub async fn entity_360(
    State(state): State<AppState>,
    Json(req): Json<super::chat::Entity360Request>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let conf = g.node_confidence(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
    let nt = g.get_node_type(&req.entity);
    let props = g.get_properties(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or_default();

    let edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let out: Vec<serde_json::Value> = edges_out.iter().take(30).map(|e| {
        serde_json::json!({ "to": e.to, "relationship": e.relationship, "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to })
    }).collect();

    let inc: Vec<serde_json::Value> = edges_in.iter().take(30).map(|e| {
        serde_json::json!({ "from": e.from, "relationship": e.relationship, "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to })
    }).collect();

    // Count facts (Fact-type nodes connected via subject_of)
    let facts_count = edges_in.iter().filter(|e| e.relationship == "subject_of").count();

    // Source distribution
    let mut sources: HashMap<String, u32> = HashMap::new();
    for e in edges_out.iter().chain(edges_in.iter()) {
        let tier = if e.confidence >= 0.80 { "high" } else if e.confidence >= 0.50 { "medium" } else { "low" };
        *sources.entry(tier.to_string()).or_default() += 1;
    }

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "node_type": nt,
        "confidence": conf,
        "properties": props,
        "edges_out": out,
        "edges_out_count": edges_out.len(),
        "edges_in": inc,
        "edges_in_count": edges_in.len(),
        "total_edges": edges_out.len() + edges_in.len(),
        "facts_count": facts_count,
        "confidence_distribution": sources,
    })))
}

/// POST /chat/entity_gaps -- what's missing based on entity type
pub async fn entity_gaps(
    State(state): State<AppState>,
    Json(req): Json<super::chat::EntityGapsRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let nt = g.get_node_type(&req.entity);
    let edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let existing_rels: HashSet<String> = edges_out.iter().chain(edges_in.iter())
        .map(|e| e.relationship.clone())
        .collect();

    // Expected relationships by type
    let expected: Vec<(&str, &str)> = match nt.as_deref() {
        Some("person") => vec![
            ("citizen_of", "Nationality/citizenship"),
            ("born_in", "Place of birth"),
            ("educated_at", "Education"),
            ("employed_by", "Employment/organization"),
            ("holds_position", "Position/role"),
            ("member_of", "Memberships"),
            ("spouse_of", "Spouse/partner"),
        ],
        Some("organization") => vec![
            ("headquartered_in", "Headquarters location"),
            ("founded_by", "Founder"),
            ("industry", "Industry/sector"),
            ("member_of", "Parent organization"),
            ("head_of_state", "Leadership"),
        ],
        Some("location") => vec![
            ("capital_of", "Capital relationship"),
            ("located_in", "Parent region/country"),
            ("shares_border_with", "Borders"),
            ("head_of_state", "Head of state"),
        ],
        Some("event") => vec![
            ("location", "Event location"),
            ("country", "Country involved"),
            ("participant", "Participants"),
        ],
        Some("product") => vec![
            ("manufactured_by", "Manufacturer"),
            ("used_by", "Users/operators"),
        ],
        _ => vec![
            ("related_to", "Any relationship"),
        ],
    };

    let mut missing = Vec::new();
    let mut present = Vec::new();
    for (rel, desc) in &expected {
        if existing_rels.contains(*rel) {
            present.push(serde_json::json!({ "relationship": rel, "description": desc }));
        } else {
            missing.push(serde_json::json!({ "relationship": rel, "description": desc }));
        }
    }

    let total = expected.len();
    let completeness = if total > 0 { (present.len() as f64 / total as f64 * 100.0).round() as u32 } else { 100 };

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "node_type": nt,
        "completeness_pct": completeness,
        "present": present,
        "missing": missing,
        "total_expected": total,
        "total_edges": edges_out.len() + edges_in.len(),
    })))
}
