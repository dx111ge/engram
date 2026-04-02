/// Chat tool handlers: temporal queries and changes.
///
/// Types used by both chat.rs and chat_analysis.rs are defined here.
/// Analysis/comparison handlers are in chat_analysis.rs.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::state::AppState;
use super::{api_err, read_lock_err, write_lock_err, ApiResult};

// ── Request types (shared with chat_analysis) ──

#[derive(Deserialize)]
pub struct TemporalQueryRequest {
    pub entity: String,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub relationship: Option<String>,
}

#[derive(Deserialize)]
pub struct TimelineRequest {
    pub entity: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct CurrentStateRequest {
    pub entity: String,
    pub depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct CompareRequest {
    pub entity_a: String,
    pub entity_b: String,
    pub aspects: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct ShortestPathRequest {
    pub from: String,
    pub to: String,
    pub max_depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct MostConnectedRequest {
    pub limit: Option<usize>,
    pub node_type: Option<String>,
}

#[derive(Deserialize)]
pub struct IsolatedRequest {
    pub max_edges: Option<u32>,
    pub node_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ChangesRequest {
    pub since: String,
    pub entity: Option<String>,
}

#[derive(Deserialize)]
pub struct WhatIfRequest {
    pub entity: String,
    pub new_confidence: Option<f64>,
    pub depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct InfluencePathRequest {
    pub from: String,
    pub to: String,
    pub max_depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct BriefingRequest {
    pub topic: String,
    pub depth: Option<String>,
    pub format: Option<String>,
}

#[derive(Deserialize)]
pub struct ExportSubgraphRequest {
    pub entity: String,
    pub depth: Option<u32>,
    pub format: Option<String>,
}

#[derive(Deserialize)]
pub struct EntityTimelineRequest {
    pub entity: String,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
}

#[derive(Deserialize)]
pub struct FactProvenanceRequest {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct ContradictionsRequest {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct SituationAtRequest {
    pub entity: String,
    pub date: String,
}

#[derive(Deserialize)]
pub struct NetworkAnalysisRequest {
    pub entity: String,
    pub depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct Entity360Request {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct EntityGapsRequest {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct DossierRequest {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct TopicMapRequest {
    pub topic: String,
}

#[derive(Deserialize)]
pub struct GraphStatsRequest {}

#[derive(Deserialize)]
pub struct WatchRequest {
    pub entity: String,
}

#[derive(Deserialize)]
pub struct ScheduleRequest {
    pub action: String,
    pub entity: Option<String>,
    pub interval: Option<String>,
}

// ── Response types ──

#[derive(Serialize)]
pub struct TemporalEdge {
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
}

#[derive(Serialize)]
pub struct CompareResponse {
    pub entity_a: EntitySummary,
    pub entity_b: EntitySummary,
    pub shared_neighbors: Vec<String>,
    pub unique_to_a: Vec<String>,
    pub unique_to_b: Vec<String>,
}

#[derive(Serialize)]
pub struct EntitySummary {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    pub confidence: f32,
    pub edge_count: u32,
    pub properties: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct PathResponse {
    pub found: bool,
    pub path: Vec<PathStep>,
    pub length: u32,
}

#[derive(Serialize)]
pub struct PathStep {
    pub entity: String,
    pub relationship: String,
    pub direction: String,
}

#[derive(Serialize)]
pub struct ConnectedNode {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    pub confidence: f32,
    pub edge_count: u32,
}

#[derive(Serialize)]
pub struct WhatIfResponse {
    pub entity: String,
    pub current_confidence: f32,
    pub simulated_confidence: f32,
    pub affected: Vec<AffectedEntity>,
}

#[derive(Serialize)]
pub struct AffectedEntity {
    pub label: String,
    pub relationship: String,
    pub current_confidence: f32,
    pub impact: String,
}

// ── Temporal Handlers ──

/// POST /chat/temporal_query -- query edges by time range
pub async fn temporal_query(
    State(state): State<AppState>,
    Json(req): Json<TemporalQueryRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let mut all_edges = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all_edges.extend(edges_in);

    let from_ts = req.from_date.as_deref().and_then(parse_date_opt);
    let to_ts = req.to_date.as_deref().and_then(parse_date_opt);

    let mut results = Vec::new();
    for edge in all_edges {
        if let Some(ref rel) = req.relationship {
            if &edge.relationship != rel { continue; }
        }
        if let Some(from) = from_ts {
            if let Some(ref vt) = edge.valid_to {
                if let Some(edge_end) = parse_date_opt(vt) {
                    if edge_end < from { continue; }
                }
            }
        }
        if let Some(to) = to_ts {
            if let Some(ref vf) = edge.valid_from {
                if let Some(edge_start) = parse_date_opt(vf) {
                    if edge_start > to { continue; }
                }
            }
        }
        results.push(TemporalEdge {
            from: edge.from, to: edge.to, relationship: edge.relationship,
            confidence: edge.confidence, valid_from: edge.valid_from, valid_to: edge.valid_to,
        });
    }

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "from_date": req.from_date,
        "to_date": req.to_date,
        "events": results,
        "event_count": results.len(),
    })))
}

/// POST /chat/timeline -- chronological events for entity
pub async fn timeline(
    State(state): State<AppState>,
    Json(req): Json<TimelineRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(20);

    let mut all_edges = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all_edges.extend(edges_in);

    all_edges.sort_by(|a, b| {
        a.valid_from.as_deref().unwrap_or("9999").cmp(b.valid_from.as_deref().unwrap_or("9999"))
    });

    let results: Vec<TemporalEdge> = all_edges.into_iter().take(limit).map(|e| {
        TemporalEdge {
            from: e.from, to: e.to, relationship: e.relationship,
            confidence: e.confidence, valid_from: e.valid_from, valid_to: e.valid_to,
        }
    }).collect();

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "events": results,
        "event_count": results.len(),
    })))
}

/// POST /chat/current_state -- only non-expired relations
pub async fn current_state(
    State(state): State<AppState>,
    Json(req): Json<CurrentStateRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let now = format_days_since_epoch(days);

    let mut all_edges = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all_edges.extend(edges_in);

    let mut current: Vec<TemporalEdge> = Vec::new();
    let mut expired: Vec<TemporalEdge> = Vec::new();
    for e in all_edges {
        let te = TemporalEdge {
            from: e.from, to: e.to, relationship: e.relationship,
            confidence: e.confidence, valid_from: e.valid_from, valid_to: e.valid_to,
        };
        match &te.valid_to {
            Some(vt) if vt.as_str() < now.as_str() => expired.push(te),
            _ => current.push(te),
        }
    }

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "as_of": now,
        "current": current,
        "expired": expired,
        "current_count": current.len(),
        "expired_count": expired.len(),
    })))
}

/// POST /chat/changes -- what changed since a timestamp
pub async fn changes(
    State(state): State<AppState>,
    Json(req): Json<ChangesRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let since_ts = parse_date_opt(&req.since).unwrap_or(0);
    let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let changed: Vec<serde_json::Value> = all.into_iter()
        .filter(|n| {
            let updated = n.updated_at > 0 && n.updated_at >= since_ts;
            let created = n.created_at > 0 && n.created_at >= since_ts;
            let matches = req.entity.as_ref().map_or(true, |e| n.label.contains(e.as_str()));
            (updated || created) && matches
        })
        .take(50)
        .map(|n| serde_json::json!({
            "label": n.label, "node_type": n.node_type, "confidence": n.confidence,
            "created_at": n.created_at, "updated_at": n.updated_at,
        }))
        .collect();

    Ok(Json(serde_json::json!({ "since": req.since, "changes": changed, "total": changed.len() })))
}

/// POST /chat/watch -- mark entity as watched
pub async fn watch(
    State(state): State<AppState>,
    Json(req): Json<WatchRequest>,
) -> ApiResult<serde_json::Value> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let exists = g.set_property(&req.entity, "_watched", "true")
        .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !exists {
        return Err(api_err(axum::http::StatusCode::NOT_FOUND, format!("entity '{}' not found", req.entity)));
    }
    state.mark_dirty();
    Ok(Json(serde_json::json!({ "entity": req.entity, "watched": true })))
}

/// POST /chat/schedule -- create or list schedules
pub async fn schedule(
    State(state): State<AppState>,
    Json(req): Json<ScheduleRequest>,
) -> ApiResult<serde_json::Value> {
    match req.action.as_str() {
        "create" => {
            let entity = req.entity.as_deref()
                .ok_or_else(|| api_err(axum::http::StatusCode::BAD_REQUEST, "entity required for create"))?;
            let interval = req.interval.as_deref().unwrap_or("daily");
            let mut g = state.graph.write().map_err(|_| write_lock_err())?;
            let exists = g.set_property(entity, "_schedule", interval)
                .map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !exists {
                return Err(api_err(axum::http::StatusCode::NOT_FOUND, format!("entity '{}' not found", entity)));
            }
            let _ = g.set_property(entity, "_watched", "true");
            state.mark_dirty();
            Ok(Json(serde_json::json!({ "entity": entity, "interval": interval, "scheduled": true })))
        }
        "list" => {
            let g = state.graph.read().map_err(|_| read_lock_err())?;
            let all = g.all_nodes().map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let scheduled: Vec<serde_json::Value> = all.iter()
                .filter(|n| n.properties.get("_schedule").is_some())
                .map(|n| serde_json::json!({
                    "entity": n.label,
                    "interval": n.properties.get("_schedule").cloned().unwrap_or_default(),
                    "node_type": n.node_type,
                }))
                .collect();
            let total = scheduled.len();
            Ok(Json(serde_json::json!({ "schedules": scheduled, "total": total })))
        }
        other => Err(api_err(axum::http::StatusCode::BAD_REQUEST, format!("unknown action: {other}"))),
    }
}

// ── Date helpers (no chrono dependency) ──

/// Parse YYYY-MM-DD to approximate unix timestamp (midnight UTC).
pub(crate) fn parse_date_opt(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 { return None; }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    let days = (y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400
        + [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][(m - 1) as usize]
        + d - 1
        + if m > 2 && (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) { 1 } else { 0 };
    Some(days * 86400)
}

fn format_days_since_epoch(total_days: u64) -> String {
    let mut y = 1970i64;
    let mut remaining = total_days as i64;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, remaining + 1)
}
