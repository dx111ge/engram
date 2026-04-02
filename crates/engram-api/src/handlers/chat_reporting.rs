/// Chat reporting handlers: briefings, subgraph export, dossiers,
/// topic maps, and graph statistics.

use axum::extract::State;
use axum::Json;
use std::collections::{HashMap, HashSet};

use crate::state::AppState;
use super::{api_err, read_lock_err, ApiResult};
use super::chat::BriefingRequest;
use super::chat::ExportSubgraphRequest;

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

/// POST /chat/dossier -- comprehensive entity report
pub async fn dossier(
    State(state): State<AppState>,
    Json(req): Json<super::chat::DossierRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let conf = g.node_confidence(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
    let nt = g.get_node_type(&req.entity);
    let props = g.get_properties(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or_default();

    let edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Key connections (top 20 by confidence)
    let mut all_connections: Vec<serde_json::Value> = edges_out.iter().map(|e| {
        serde_json::json!({ "target": e.to, "relationship": e.relationship, "direction": "out", "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to })
    }).chain(edges_in.iter().map(|e| {
        serde_json::json!({ "target": e.from, "relationship": e.relationship, "direction": "in", "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to })
    })).collect();
    all_connections.sort_by(|a, b| {
        let ca = a.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cb = b.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
        cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
    });
    all_connections.truncate(20);

    // Source distribution
    let mut high = 0u32; let mut med = 0u32; let mut low = 0u32;
    for e in edges_out.iter().chain(edges_in.iter()) {
        if e.confidence >= 0.70 { high += 1; } else if e.confidence >= 0.40 { med += 1; } else { low += 1; }
    }

    // Facts count
    let facts_count = edges_in.iter().filter(|e| e.relationship == "subject_of").count();

    // Temporal range
    let mut dates: Vec<&str> = edges_out.iter().chain(edges_in.iter())
        .filter_map(|e| e.valid_from.as_deref())
        .collect();
    dates.sort();
    let earliest = dates.first().copied().unwrap_or("unknown");
    let latest = dates.last().copied().unwrap_or("unknown");

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "node_type": nt,
        "confidence": conf,
        "properties": props,
        "connections": all_connections,
        "total_connections": edges_out.len() + edges_in.len(),
        "facts_count": facts_count,
        "confidence_distribution": { "high": high, "medium": med, "low": low },
        "temporal_range": { "earliest": earliest, "latest": latest },
    })))
}

/// POST /chat/topic_map -- map real entities known about a topic
///
/// Internal node types (Fact, Document, Source) are NOT shown directly.
/// Instead, when a Fact/Document matches, we follow edges to find the
/// real-world entities they reference (persons, orgs, locations, products, events).
pub async fn topic_map(
    State(state): State<AppState>,
    Json(req): Json<super::chat::TopicMapRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let hits = g.search_text(&req.topic, 30)
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if hits.is_empty() {
        return Ok(Json(serde_json::json!({
            "topic": req.topic,
            "error": format!("No entities found matching '{}'", req.topic),
            "clusters": [],
        })));
    }

    // Internal types that should be resolved to their referenced entities
    let internal_types: HashSet<&str> = ["Fact", "fact", "Document", "document", "Source", "source"].iter().copied().collect();

    // Collect real entities: either direct hits or resolved from internal nodes
    let mut seen: HashSet<String> = HashSet::new();
    let mut real_entities: Vec<(String, String, f32, u32)> = Vec::new(); // (label, type, confidence, edge_count)

    for hit in &hits {
        let nt = g.get_node_type(&hit.label).unwrap_or_else(|| "entity".to_string());

        if internal_types.contains(nt.as_str()) {
            // This is a Fact/Document/Source -- follow edges to find real entities
            let edges_out = g.edges_from(&hit.label).unwrap_or_default();
            let edges_in = g.edges_to(&hit.label).unwrap_or_default();

            for edge in edges_out.iter().chain(edges_in.iter()) {
                let neighbor = if edge.from == hit.label { &edge.to } else { &edge.from };
                let neighbor_type = g.get_node_type(neighbor).unwrap_or_else(|| "entity".to_string());

                // Only include real-world entity types
                if !internal_types.contains(neighbor_type.as_str()) && seen.insert(neighbor.clone()) {
                    let conf = g.node_confidence(neighbor).ok().flatten().unwrap_or(0.0);
                    let n_out = g.edges_from(neighbor).map(|e| e.len()).unwrap_or(0);
                    let n_in = g.edges_to(neighbor).map(|e| e.len()).unwrap_or(0);
                    real_entities.push((neighbor.clone(), neighbor_type, conf, (n_out + n_in) as u32));
                }
            }
        } else {
            // Direct real-world entity hit
            if seen.insert(hit.label.clone()) {
                let edges_out = g.edges_from(&hit.label).unwrap_or_default();
                let edges_in = g.edges_to(&hit.label).unwrap_or_default();
                real_entities.push((hit.label.clone(), nt, hit.confidence, (edges_out.len() + edges_in.len()) as u32));
            }
        }
    }

    if real_entities.is_empty() {
        return Ok(Json(serde_json::json!({
            "topic": req.topic,
            "error": format!("No real-world entities found for '{}'", req.topic),
            "clusters": [],
        })));
    }

    // Sort by edge count (most connected first) and limit
    real_entities.sort_by(|a, b| b.3.cmp(&a.3));
    real_entities.truncate(30);

    // Group by type
    let mut clusters: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut total_edges = 0u32;

    for (label, nt, conf, ec) in &real_entities {
        total_edges += ec;
        clusters.entry(nt.clone()).or_default().push(serde_json::json!({
            "label": label,
            "node_type": nt,
            "confidence": conf,
            "edge_count": ec,
        }));
    }

    // Sort clusters: most entities first
    let mut cluster_list: Vec<serde_json::Value> = clusters.into_iter().map(|(typ, entities)| {
        serde_json::json!({
            "type": typ,
            "count": entities.len(),
            "entities": entities,
        })
    }).collect();
    cluster_list.sort_by(|a, b| {
        let ca = a.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let cb = b.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        cb.cmp(&ca)
    });

    Ok(Json(serde_json::json!({
        "topic": req.topic,
        "clusters": cluster_list,
        "total_entities": real_entities.len(),
        "total_edges": total_edges,
    })))
}

/// POST /chat/graph_stats -- knowledge base health overview
pub async fn graph_stats(
    State(state): State<AppState>,
    Json(_req): Json<super::chat::GraphStatsRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_nodes = all.len();

    let mut by_type: HashMap<String, u32> = HashMap::new();
    let mut total_edges = 0u64;
    let mut conf_high = 0u32; let mut conf_med = 0u32; let mut conf_low = 0u32;

    for n in &all {
        let nt = n.node_type.as_deref().unwrap_or("unknown");
        *by_type.entry(nt.to_string()).or_default() += 1;
        let ec = n.edge_out_count as u64 + n.edge_in_count as u64;
        total_edges += ec;
        if n.confidence >= 0.70 { conf_high += 1; } else if n.confidence >= 0.40 { conf_med += 1; } else { conf_low += 1; }
    }
    total_edges /= 2; // edges counted from both sides

    let mut type_list: Vec<serde_json::Value> = by_type.into_iter()
        .map(|(t, c)| serde_json::json!({"type": t, "count": c}))
        .collect();
    type_list.sort_by(|a, b| {
        let ca = a.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let cb = b.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        cb.cmp(&ca)
    });

    Ok(Json(serde_json::json!({
        "total_nodes": total_nodes,
        "total_edges": total_edges,
        "nodes_by_type": type_list,
        "confidence_distribution": {
            "high": conf_high,
            "medium": conf_med,
            "low": conf_low,
        },
    })))
}
