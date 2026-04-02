/// Chat temporal handlers: entity timeline, fact provenance, contradictions,
/// and situation-at-date reconstruction.

use axum::extract::State;
use axum::Json;
use std::collections::{HashMap, HashSet};

use crate::state::AppState;
use super::{api_err, read_lock_err, ApiResult};
use super::chat::{EntityTimelineRequest};

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

/// POST /chat/fact_provenance -- how information about an entity arrived and spread
pub async fn fact_provenance(
    State(state): State<AppState>,
    Json(req): Json<super::chat::FactProvenanceRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let mut edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    edges_out.extend(edges_in);

    if edges_out.is_empty() {
        return Ok(Json(serde_json::json!({
            "entity": req.entity,
            "error": format!("No information found for '{}'", req.entity),
            "sources": [],
            "timeline": [],
            "corroborations": [],
        })));
    }

    // Group edges by source (provenance) and build timeline sorted by valid_from
    let mut source_map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut timeline = Vec::new();

    for edge in &edges_out {
        let fact_label = format!("{} | {} | {}", edge.from, edge.relationship, edge.to);

        // Try to determine source from the relationship's provenance
        // For now, check if connected to Fact nodes or Documents
        let source = if edge.relationship == "extracted_from" || edge.relationship == "subject_of" {
            "LLM extraction".to_string()
        } else if edge.confidence >= 0.80 {
            "Knowledge Base (SPARQL)".to_string()
        } else if edge.confidence >= 0.50 {
            "Co-occurrence / GLiNER2".to_string()
        } else {
            "Low confidence".to_string()
        };

        let entry = serde_json::json!({
            "fact": fact_label,
            "from": edge.from,
            "to": edge.to,
            "relationship": edge.relationship,
            "confidence": edge.confidence,
            "valid_from": edge.valid_from,
            "source": source,
        });

        source_map.entry(source.clone()).or_default().push(entry.clone());
        timeline.push(entry);
    }

    // Sort timeline by valid_from (earliest first)
    timeline.sort_by(|a, b| {
        let da = a.get("valid_from").and_then(|v| v.as_str()).unwrap_or("9999");
        let db = b.get("valid_from").and_then(|v| v.as_str()).unwrap_or("9999");
        da.cmp(db)
    });
    timeline.truncate(20);

    // Build source summary
    let sources: Vec<serde_json::Value> = source_map.iter().map(|(src, facts)| {
        serde_json::json!({
            "source": src,
            "facts_count": facts.len(),
        })
    }).collect();

    // Detect corroborations: same relationship claimed by multiple confidence tiers
    let mut rel_sources: HashMap<String, HashSet<String>> = HashMap::new();
    for edge in &edges_out {
        let key = format!("{}-{}-{}", edge.from, edge.relationship, edge.to);
        let source = if edge.confidence >= 0.80 { "high" } else if edge.confidence >= 0.50 { "medium" } else { "low" };
        rel_sources.entry(key).or_default().insert(source.to_string());
    }

    let corroborations: Vec<serde_json::Value> = rel_sources.iter()
        .filter(|(_, tiers)| tiers.len() > 1)
        .map(|(claim, tiers)| {
            serde_json::json!({
                "claim": claim,
                "tiers": tiers.iter().collect::<Vec<_>>(),
                "count": tiers.len(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "total_facts": edges_out.len(),
        "sources": sources,
        "timeline": timeline,
        "corroborations": corroborations,
    })))
}

/// POST /chat/contradictions -- conflicting information about an entity
pub async fn contradictions(
    State(state): State<AppState>,
    Json(req): Json<super::chat::ContradictionsRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let mut edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    edges_out.extend(edges_in);

    if edges_out.is_empty() {
        return Ok(Json(serde_json::json!({
            "entity": req.entity,
            "error": format!("No information found for '{}'", req.entity),
            "conflicts": [],
            "low_confidence_facts": [],
        })));
    }

    // Detect property conflicts: same relationship type to different targets from same entity
    // Only flag as conflict when time periods OVERLAP. Non-overlapping = temporal succession.
    let mut rel_targets: HashMap<String, Vec<(String, f32, Option<String>, Option<String>)>> = HashMap::new();
    for edge in &edges_out {
        if edge.from == req.entity {
            rel_targets.entry(edge.relationship.clone())
                .or_default()
                .push((edge.to.clone(), edge.confidence, edge.valid_from.clone(), edge.valid_to.clone()));
        }
    }

    let mut conflicts = Vec::new();
    let mut successions = Vec::new();
    for (rel_type, targets) in &rel_targets {
        // Relationship types that should be unique at any point in time
        let is_unique_rel = matches!(rel_type.as_str(),
            "head_of_state" | "capital_of" | "president_of" | "ceo_of" | "headquartered_in"
            | "born_in" | "died_in" | "citizen_of" | "spouse_of"
        );

        if is_unique_rel && targets.len() > 1 {
            let mut sorted = targets.clone();
            sorted.sort_by(|a, b| {
                let da = a.2.as_deref().unwrap_or("0000");
                let db = b.2.as_deref().unwrap_or("0000");
                da.cmp(db)
            });

            for i in 0..sorted.len() {
                for j in (i+1)..sorted.len() {
                    if sorted[i].0 == sorted[j].0 { continue; } // same target, not a conflict

                    // Check temporal overlap: conflict only if periods overlap
                    let a_end = sorted[i].3.as_deref().unwrap_or("9999-12-31");
                    let b_start = sorted[j].2.as_deref().unwrap_or("0000-01-01");
                    let temporally_separate = a_end < b_start;

                    if temporally_separate {
                        // Temporal succession, not a conflict
                        successions.push(serde_json::json!({
                            "type": "temporal_succession",
                            "relationship": rel_type,
                            "earlier": {
                                "target": sorted[i].0,
                                "confidence": sorted[i].1,
                                "valid_from": sorted[i].2,
                                "valid_to": sorted[i].3,
                            },
                            "later": {
                                "target": sorted[j].0,
                                "confidence": sorted[j].1,
                                "valid_from": sorted[j].2,
                                "valid_to": sorted[j].3,
                            },
                        }));
                    } else {
                        // Overlapping or no temporal data = real conflict
                        conflicts.push(serde_json::json!({
                            "type": "property_conflict",
                            "relationship": rel_type,
                            "claim_a": {
                                "target": sorted[i].0,
                                "confidence": sorted[i].1,
                                "valid_from": sorted[i].2,
                                "valid_to": sorted[i].3,
                            },
                            "claim_b": {
                                "target": sorted[j].0,
                                "confidence": sorted[j].1,
                                "valid_from": sorted[j].2,
                                "valid_to": sorted[j].3,
                            },
                            "status": "unresolved",
                        }));
                    }
                }
            }
        }
    }
    conflicts.truncate(10);
    successions.truncate(10);

    // Find low confidence facts (potential inaccuracies)
    let mut low_confidence: Vec<serde_json::Value> = edges_out.iter()
        .filter(|e| e.confidence < 0.30)
        .map(|e| serde_json::json!({
            "fact": format!("{} | {} | {}", e.from, e.relationship, e.to),
            "confidence": e.confidence,
            "valid_from": e.valid_from,
        }))
        .collect();
    low_confidence.truncate(10);

    // Find debunked/pending facts (Fact nodes with status != confirmed)
    let mut debunked = Vec::new();
    for edge in &edges_out {
        let other = if edge.from == req.entity { &edge.to } else { &edge.from };
        let nt_val = g.get_property(other, "node_type").ok().flatten().or_else(|| g.get_node_type(other));
        if let Some(nt) = nt_val {
            if nt == "Fact" {
                if let Ok(Some(status)) = g.get_property(other, "status") {
                    if status == "debunked" || status == "disputed" {
                        debunked.push(serde_json::json!({
                            "fact": other,
                            "status": status,
                            "confidence": edge.confidence,
                        }));
                    }
                }
            }
        }
    }
    debunked.truncate(10);

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "conflicts": conflicts,
        "conflict_count": conflicts.len(),
        "successions": successions,
        "succession_count": successions.len(),
        "low_confidence_facts": low_confidence,
        "debunked_facts": debunked,
    })))
}

/// POST /chat/situation_at -- reconstruct knowledge state at a specific date
pub async fn situation_at(
    State(state): State<AppState>,
    Json(req): Json<super::chat::SituationAtRequest>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;

    let mut edges_out = g.edges_from(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let edges_in = g.edges_to(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    edges_out.extend(edges_in);

    if edges_out.is_empty() {
        return Ok(Json(serde_json::json!({
            "entity": req.entity,
            "date": req.date,
            "error": format!("No information found for '{}'", req.entity),
            "active_edges": [],
        })));
    }

    // Filter edges that were valid at the target date
    let active: Vec<serde_json::Value> = edges_out.iter()
        .filter(|e| {
            // Edge must have started before or on target date
            if let Some(ref vf) = e.valid_from {
                if vf.as_str() > req.date.as_str() { return false; }
            }
            // Edge must not have ended before target date
            if let Some(ref vt) = e.valid_to {
                if vt.as_str() < req.date.as_str() { return false; }
            }
            true
        })
        .map(|e| serde_json::json!({
            "from": e.from, "to": e.to, "relationship": e.relationship,
            "confidence": e.confidence, "valid_from": e.valid_from, "valid_to": e.valid_to,
        }))
        .collect();

    let conf = g.node_confidence(&req.entity).map_err(|e| api_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.unwrap_or(0.0);
    let nt = g.get_node_type(&req.entity);

    Ok(Json(serde_json::json!({
        "entity": req.entity,
        "date": req.date,
        "node_type": nt,
        "confidence": conf,
        "active_edges": active,
        "edge_count": active.len(),
        "total_edges": edges_out.len(),
    })))
}
