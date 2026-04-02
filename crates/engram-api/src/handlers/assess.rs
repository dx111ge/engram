use super::*;

// ── Assessment endpoints ─────────────────────────────────────────────

// POST /assessments -- Create assessment
#[cfg(feature = "assess")]
pub async fn create_assessment(
    State(state): State<AppState>,
    Json(req): Json<engram_assess::CreateAssessmentRequest>,
) -> ApiResult<serde_json::Value> {
    let label = format!("Assessment:{}", req.title.to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', ""));

    let initial_prob = req.initial_probability.unwrap_or(0.50).clamp(0.05, 0.95);
    let prov = provenance(&None);

    // Create graph node
    let node_id = {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let slot = g.store_with_confidence(&label, initial_prob, &prov)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let _ = g.set_node_type(&label, "assessment");
        let _ = g.set_property(&label, "title", &req.title);
        if let Some(ref cat) = req.category { let _ = g.set_property(&label, "category", cat); }
        if let Some(ref desc) = req.description { let _ = g.set_property(&label, "description", desc); }
        if let Some(ref tf) = req.timeframe { let _ = g.set_property(&label, "timeframe", tf); }
        let _ = g.set_property(&label, "status", "active");
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", initial_prob));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = g.set_property(&label, "last_evaluated", &now.to_string());

        // Create watch edges
        for entity in &req.watches {
            let _ = g.relate(&label, entity, "watches", &prov);
        }

        slot
    };

    state.mark_dirty();

    // Create sidecar record
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let record = engram_assess::AssessmentRecord {
            label: label.clone(),
            node_id,
            history: vec![engram_assess::ScorePoint {
                timestamp: now,
                probability: initial_prob,
                shift: 0.0,
                trigger: engram_assess::ScoreTrigger::Created,
                reason: "Initial assessment created".to_string(),
                path: None,
            }],
            evidence: vec![],
            success_criteria: req.success_criteria.clone(),
            tags: req.tags.clone(),
            resolution: "active".to_string(),
            pending_count: 0,
            evidence_for: vec![],
            evidence_against: vec![],
        };

        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        store.insert(record);
    }

    Ok(Json(serde_json::json!({
        "label": label,
        "node_id": node_id,
        "probability": initial_prob,
        "status": "active",
        "watches": req.watches,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn create_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments -- List assessments
#[cfg(feature = "assess")]
pub async fn list_assessments(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;

    let category_filter = params.get("category").filter(|s| !s.is_empty());
    let status_filter = params.get("status").filter(|s| !s.is_empty());

    let mut assessments = Vec::new();
    for record in store.all() {
        let props = g.get_properties(&record.label).ok().flatten().unwrap_or_default();
        let category = props.get("category").cloned().unwrap_or_default();
        let status = props.get("status").cloned().unwrap_or_else(|| "active".to_string());

        if let Some(cf) = category_filter {
            if &category != cf { continue; }
        }
        if let Some(sf) = status_filter {
            if &status != sf { continue; }
        }

        let last_shift = record.history.last().map(|p| p.shift).unwrap_or(0.0);
        let last_evaluated: i64 = props.get("last_evaluated")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let watch_count = g.edges_from(&record.label)
            .unwrap_or_default()
            .iter()
            .filter(|e| e.relationship == "watches")
            .count();

        // Stale detection: not evaluated in 7+ days
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let seven_days: i64 = 7 * 24 * 3600;
        let stale = last_evaluated == 0 || (now_secs - last_evaluated) > seven_days;

        assessments.push(serde_json::json!({
            "label": record.label,
            "title": props.get("title").cloned().unwrap_or_else(|| record.label.clone()),
            "category": category,
            "status": status,
            "description": props.get("description").cloned().unwrap_or_default(),
            "timeframe": props.get("timeframe").cloned().unwrap_or_default(),
            "current_probability": engram_assess::engine::recalculate_probability(record),
            "last_evaluated": last_evaluated,
            "evidence_count": record.evidence_for.len() + record.evidence_against.len(),
            "watch_count": watch_count,
            "last_shift": last_shift,
            "pending_count": record.pending_count,
            "stale": stale,
            "resolution": record.resolution,
            "tags": record.tags,
        }));
    }

    Ok(Json(serde_json::json!({ "assessments": assessments })))
}

#[cfg(not(feature = "assess"))]
pub async fn list_assessments() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments/:label -- Full detail
#[cfg(feature = "assess")]
pub async fn get_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;

    let record = store.get(&label)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

    let props = g.get_properties(&label).ok().flatten().unwrap_or_default();

    let watches: Vec<String> = g.edges_from(&label)
        .unwrap_or_default()
        .iter()
        .filter(|e| e.relationship == "watches")
        .map(|e| e.to.clone())
        .collect();

    let evidence_for: Vec<serde_json::Value> = record.evidence_for.iter().enumerate().map(|(i, &c)| {
        serde_json::json!({ "node_label": format!("evidence_{}", i), "confidence": c })
    }).collect();

    let evidence_against: Vec<serde_json::Value> = record.evidence_against.iter().enumerate().map(|(i, &c)| {
        serde_json::json!({ "node_label": format!("evidence_{}", i), "confidence": c })
    }).collect();

    let last_evaluated: i64 = props.get("last_evaluated")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let seven_days: i64 = 7 * 24 * 3600;
    let stale = last_evaluated == 0 || (now_secs - last_evaluated) > seven_days;

    Ok(Json(serde_json::json!({
        "label": record.label,
        "title": props.get("title").cloned().unwrap_or_else(|| record.label.clone()),
        "category": props.get("category").cloned().unwrap_or_default(),
        "status": props.get("status").cloned().unwrap_or_else(|| "active".to_string()),
        "description": props.get("description").cloned().unwrap_or_default(),
        "timeframe": props.get("timeframe").cloned().unwrap_or_default(),
        "current_probability": engram_assess::engine::recalculate_probability(record),
        "last_evaluated": last_evaluated,
        "history": record.history,
        "evidence_for": evidence_for,
        "evidence_against": evidence_against,
        "watches": watches,
        "pending_count": record.pending_count,
        "stale": stale,
        "resolution": record.resolution,
        "tags": record.tags,
        "success_criteria": record.success_criteria,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn get_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label
#[cfg(feature = "assess")]
pub async fn delete_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    // Remove from graph
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = provenance(&None);
        let _ = g.delete(&label, &prov);
    }
    state.mark_dirty();

    // Remove from sidecar
    {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        store.remove(&label);
    }

    Ok(Json(serde_json::json!({ "deleted": label })))
}

#[cfg(not(feature = "assess"))]
pub async fn delete_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// PATCH /assessments/:label
#[cfg(feature = "assess")]
pub async fn update_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::UpdateAssessmentRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        if let Some(ref title) = req.title { let _ = g.set_property(&label, "title", title); }
        if let Some(ref desc) = req.description { let _ = g.set_property(&label, "description", desc); }
        if let Some(ref cat) = req.category { let _ = g.set_property(&label, "category", cat); }
        if let Some(ref status) = req.status { let _ = g.set_property(&label, "status", status); }
        if let Some(ref tf) = req.timeframe { let _ = g.set_property(&label, "timeframe", tf); }
    }

    // Manual probability override
    if let Some(prob) = req.probability {
        let prob = prob.clamp(0.05, 0.95);
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        if let Some(record) = store.get_mut(&label) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let old_prob = record.history.last().map(|p| p.probability).unwrap_or(0.50);
            record.history.push(engram_assess::ScorePoint {
                timestamp: now,
                probability: prob,
                shift: prob - old_prob,
                trigger: engram_assess::ScoreTrigger::Manual,
                reason: "Manual probability adjustment".to_string(),
                path: None,
            });
        }

        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", prob));
    }

    state.mark_dirty();
    Ok(Json(serde_json::json!({ "updated": label })))
}

#[cfg(not(feature = "assess"))]
pub async fn update_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/evaluate
#[cfg(feature = "assess")]
pub async fn evaluate_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    let point = {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        let record = store.get_mut(&label)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;
        engram_assess::engine::evaluate(record)
    };

    // Update graph property
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", point.probability));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = g.set_property(&label, "last_evaluated", &now.to_string());
    }
    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "label": label,
        "old_probability": point.probability - point.shift,
        "new_probability": point.probability,
        "shift": point.shift,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn evaluate_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/evidence
#[cfg(feature = "assess")]
pub async fn add_assessment_evidence(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::AddEvidenceRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let supports = req.direction == "supports";

    // Get evidence node confidence
    let evidence_conf = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        req.confidence.unwrap_or_else(|| {
            g.get_node(&req.node_label).ok().flatten().map(|n| n.confidence).unwrap_or(0.50)
        })
    };

    // Create evidence edge in graph
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let prov = provenance(&None);
        let edge_type = if supports { "supported_by" } else { "contradicted_by" };
        let _ = g.relate(&req.node_label, &label, edge_type, &prov);
    }

    // Update sidecar
    let point = {
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        let record = store.get_mut(&label)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

        let node_id = 0; // We don't have the node_id easily here
        engram_assess::engine::add_evidence(
            record,
            evidence_conf,
            supports,
            engram_assess::ScoreTrigger::EvidenceAdded { node_id },
            format!("Evidence '{}' {} assessment", req.node_label, if supports { "supports" } else { "contradicts" }),
            None,
            &req.node_label,
            "user",
        )
    };

    // Update graph property
    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        let _ = g.set_property(&label, "current_probability", &format!("{:.4}", point.probability));
    }
    state.mark_dirty();

    Ok(Json(serde_json::json!({
        "added": true,
        "direction": req.direction,
        "new_probability": point.probability,
        "shift": point.shift,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn add_assessment_evidence() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label/evidence/:id
#[cfg(feature = "assess")]
pub async fn remove_assessment_evidence(
    State(state): State<AppState>,
    Path((label, evidence_label)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let evidence_label = urlencoding::decode(&evidence_label).unwrap_or_default().to_string();

    // Remove the evidence edge (supported_by or contradicted_by)
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);

    let mut removed = false;
    let mut was_supporting = true;

    // Try supported_by first
    if g.delete_edge(&evidence_label, &label, "supported_by", &prov).is_ok() {
        removed = true;
        was_supporting = true;
    }
    // Try contradicted_by
    if !removed {
        if g.delete_edge(&evidence_label, &label, "contradicted_by", &prov).is_ok() {
            removed = true;
            was_supporting = false;
        }
    }
    drop(g);

    if removed {
        // Update evidence arrays in sidecar
        let mut store = state.assessments.write().map_err(|_| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
        })?;
        if let Some(record) = store.get_mut(&label) {
            if was_supporting {
                // Remove last evidence_for entry (best effort)
                record.evidence_for.pop();
            } else {
                record.evidence_against.pop();
            }
        }
        state.mark_dirty();
    }

    Ok(Json(serde_json::json!({ "removed": removed, "evidence": evidence_label })))
}

#[cfg(not(feature = "assess"))]
pub async fn remove_assessment_evidence() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments/:label/history
#[cfg(feature = "assess")]
pub async fn assessment_history(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;
    let record = store.get(&label)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label}")))?;

    Ok(Json(serde_json::json!({ "label": label, "history": record.history })))
}

#[cfg(not(feature = "assess"))]
pub async fn assessment_history() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/watch
#[cfg(feature = "assess")]
pub async fn add_assessment_watch(
    State(state): State<AppState>,
    Path(label): Path<String>,
    Json(req): Json<engram_assess::AddWatchRequest>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);
    let _ = g.relate(&label, &req.entity_label, "watches", &prov);
    drop(g);
    state.mark_dirty();

    Ok(Json(serde_json::json!({ "added": true, "entity": req.entity_label })))
}

#[cfg(not(feature = "assess"))]
pub async fn add_assessment_watch() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// DELETE /assessments/:label/watch/:entity
#[cfg(feature = "assess")]
pub async fn remove_assessment_watch(
    State(state): State<AppState>,
    Path((label, entity)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();
    let entity = urlencoding::decode(&entity).unwrap_or_default().to_string();

    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let prov = provenance(&None);
    let removed = g.delete_edge(&label, &entity, "watches", &prov).is_ok();
    drop(g);

    if removed {
        state.mark_dirty();
    }

    Ok(Json(serde_json::json!({ "removed": removed, "entity": entity })))
}

#[cfg(not(feature = "assess"))]
pub async fn remove_assessment_watch() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// GET /assessments/compare/{label_a}/{label_b} -- Side-by-side comparison
#[cfg(feature = "assess")]
pub async fn compare_assessments(
    State(state): State<AppState>,
    Path((label_a, label_b)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let label_a = urlencoding::decode(&label_a).unwrap_or_default().to_string();
    let label_b = urlencoding::decode(&label_b).unwrap_or_default().to_string();

    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let store = state.assessments.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "assessment store lock poisoned")
    })?;

    let record_a = store.get(&label_a)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label_a}")))?;
    let record_b = store.get(&label_b)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("assessment not found: {label_b}")))?;

    let props_a = g.get_properties(&label_a).ok().flatten().unwrap_or_default();
    let props_b = g.get_properties(&label_b).ok().flatten().unwrap_or_default();

    let watches_a: Vec<String> = g.edges_from(&label_a).unwrap_or_default().iter()
        .filter(|e| e.relationship == "watches").map(|e| e.to.clone()).collect();
    let watches_b: Vec<String> = g.edges_from(&label_b).unwrap_or_default().iter()
        .filter(|e| e.relationship == "watches").map(|e| e.to.clone()).collect();

    let shared_watches: Vec<String> = watches_a.iter()
        .filter(|w| watches_b.contains(w)).cloned().collect();

    let build_side = |record: &engram_assess::AssessmentRecord, props: &std::collections::HashMap<String, String>, watches: &[String]| {
        serde_json::json!({
            "label": record.label,
            "title": props.get("title").cloned().unwrap_or_else(|| record.label.clone()),
            "category": props.get("category").cloned().unwrap_or_default(),
            "status": props.get("status").cloned().unwrap_or_else(|| "active".to_string()),
            "description": props.get("description").cloned().unwrap_or_default(),
            "current_probability": engram_assess::engine::recalculate_probability(record),
            "evidence_for_count": record.evidence_for.len(),
            "evidence_against_count": record.evidence_against.len(),
            "history_length": record.history.len(),
            "last_shift": record.history.last().map(|p| p.shift).unwrap_or(0.0),
            "watches": watches,
        })
    };

    Ok(Json(serde_json::json!({
        "assessment_a": build_side(record_a, &props_a, &watches_a),
        "assessment_b": build_side(record_b, &props_b, &watches_b),
        "shared_watches": shared_watches,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn compare_assessments() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/suggest-watches -- LLM + graph search for relevant entities
#[cfg(feature = "assess")]
pub async fn suggest_watches(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let hypothesis = req.get("hypothesis").and_then(|v| v.as_str()).unwrap_or("");
    if hypothesis.is_empty() {
        return Ok(Json(serde_json::json!({ "suggestions": [], "error": "No hypothesis text provided" })));
    }

    let g = state.graph.read().map_err(|_| read_lock_err())?;

    // Strategy 1: Extract words from hypothesis and search graph
    let words: Vec<&str> = hypothesis.split_whitespace()
        .filter(|w| w.len() > 2)
        .filter(|w| !matches!(w.to_lowercase().as_str(),
            "the" | "and" | "will" | "that" | "this" | "with" | "for" | "from" | "are" | "was"
            | "were" | "been" | "have" | "has" | "had" | "not" | "but" | "what" | "how"
            | "can" | "could" | "would" | "should" | "may" | "might" | "shall" | "does"
            | "did" | "its" | "his" | "her" | "our" | "their" | "about" | "into" | "over"
            | "than" | "then" | "when" | "where" | "which" | "who" | "whom" | "why"
            | "all" | "any" | "each" | "every" | "more" | "most" | "other" | "some" | "such"
            | "very" | "also" | "just" | "only" | "still" | "already" | "between" | "through"
        ))
        .collect();

    let mut found_entities: std::collections::HashMap<String, (String, f32, u32)> = std::collections::HashMap::new();

    // Search each significant word
    for word in &words {
        if let Ok(hits) = g.search_text(word, 5) {
            for hit in hits {
                let nt = g.get_node_type(&hit.label).unwrap_or_else(|| "entity".to_string());
                // Skip internal types
                if matches!(nt.as_str(), "Fact" | "fact" | "Document" | "document" | "Source" | "source") {
                    continue;
                }
                let edges_out = g.edges_from(&hit.label).map(|e| e.len()).unwrap_or(0);
                let edges_in = g.edges_to(&hit.label).map(|e| e.len()).unwrap_or(0);
                let ec = (edges_out + edges_in) as u32;
                found_entities.entry(hit.label.clone())
                    .or_insert((nt, hit.confidence, ec));
            }
        }
    }

    // Also try full-phrase search
    if let Ok(hits) = g.search_text(hypothesis, 10) {
        for hit in hits {
            let nt = g.get_node_type(&hit.label).unwrap_or_else(|| "entity".to_string());
            if matches!(nt.as_str(), "Fact" | "fact" | "Document" | "document" | "Source" | "source") {
                continue;
            }
            let edges_out = g.edges_from(&hit.label).map(|e| e.len()).unwrap_or(0);
            let edges_in = g.edges_to(&hit.label).map(|e| e.len()).unwrap_or(0);
            let ec = (edges_out + edges_in) as u32;
            found_entities.entry(hit.label.clone())
                .or_insert((nt, hit.confidence, ec));
        }
    }

    // Sort by edge count (most connected = most relevant)
    let mut suggestions: Vec<serde_json::Value> = found_entities.into_iter()
        .map(|(label, (nt, conf, ec))| {
            serde_json::json!({
                "label": label,
                "node_type": nt,
                "confidence": conf,
                "edge_count": ec,
            })
        })
        .collect();
    suggestions.sort_by(|a, b| {
        let ea = a.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let eb = b.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        eb.cmp(&ea)
    });
    suggestions.truncate(20);

    Ok(Json(serde_json::json!({
        "hypothesis": hypothesis,
        "suggestions": suggestions,
        "search_terms": words,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn suggest_watches() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}

// POST /assessments/:label/suggest-watches -- Suggest new watches for an existing assessment
#[cfg(feature = "assess")]
pub async fn suggest_watches_for_assessment(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> ApiResult<serde_json::Value> {
    let label = urlencoding::decode(&label).unwrap_or_default().to_string();

    let g = state.graph.read().map_err(|_| read_lock_err())?;

    // Get existing watches so we can exclude them
    let existing_watches: std::collections::HashSet<String> = g.edges_from(&label)
        .unwrap_or_default()
        .iter()
        .filter(|e| e.relationship == "watches")
        .map(|e| e.to.clone())
        .collect();

    // Get hypothesis text from graph properties
    let props = g.get_properties(&label).ok().flatten().unwrap_or_default();
    let hypothesis = props.get("title").cloned().unwrap_or_default();
    let description = props.get("description").cloned().unwrap_or_default();
    let search_text = format!("{} {}", hypothesis, description);

    if search_text.trim().is_empty() {
        return Ok(Json(serde_json::json!({ "suggestions": [], "error": "No hypothesis text available" })));
    }

    // Same search logic as global suggest_watches but excluding already-watched entities
    let words: Vec<&str> = search_text.split_whitespace()
        .filter(|w| w.len() > 2)
        .filter(|w| !matches!(w.to_lowercase().as_str(),
            "the" | "and" | "will" | "that" | "this" | "with" | "for" | "from" | "are" | "was"
            | "were" | "been" | "have" | "has" | "had" | "not" | "but" | "what" | "how"
            | "can" | "could" | "would" | "should" | "may" | "might" | "shall" | "does"
            | "did" | "its" | "his" | "her" | "our" | "their" | "about" | "into" | "over"
            | "than" | "then" | "when" | "where" | "which" | "who" | "whom" | "why"
            | "all" | "any" | "each" | "every" | "more" | "most" | "other" | "some" | "such"
            | "very" | "also" | "just" | "only" | "still" | "already" | "between" | "through"
        ))
        .collect();

    let mut found_entities: std::collections::HashMap<String, (String, f32, u32)> = std::collections::HashMap::new();

    for word in &words {
        if let Ok(hits) = g.search_text(word, 5) {
            for hit in hits {
                if existing_watches.contains(&hit.label) || hit.label == label {
                    continue;
                }
                let nt = g.get_node_type(&hit.label).unwrap_or_else(|| "entity".to_string());
                if matches!(nt.as_str(), "Fact" | "fact" | "Document" | "document" | "Source" | "source" | "assessment") {
                    continue;
                }
                let edges_out = g.edges_from(&hit.label).map(|e| e.len()).unwrap_or(0);
                let edges_in = g.edges_to(&hit.label).map(|e| e.len()).unwrap_or(0);
                let ec = (edges_out + edges_in) as u32;
                found_entities.entry(hit.label.clone())
                    .or_insert((nt, hit.confidence, ec));
            }
        }
    }

    // Also try full-phrase search
    if let Ok(hits) = g.search_text(&search_text, 10) {
        for hit in hits {
            if existing_watches.contains(&hit.label) || hit.label == label {
                continue;
            }
            let nt = g.get_node_type(&hit.label).unwrap_or_else(|| "entity".to_string());
            if matches!(nt.as_str(), "Fact" | "fact" | "Document" | "document" | "Source" | "source" | "assessment") {
                continue;
            }
            let edges_out = g.edges_from(&hit.label).map(|e| e.len()).unwrap_or(0);
            let edges_in = g.edges_to(&hit.label).map(|e| e.len()).unwrap_or(0);
            let ec = (edges_out + edges_in) as u32;
            found_entities.entry(hit.label.clone())
                .or_insert((nt, hit.confidence, ec));
        }
    }

    let mut suggestions: Vec<serde_json::Value> = found_entities.into_iter()
        .map(|(lbl, (nt, conf, ec))| {
            serde_json::json!({
                "label": lbl,
                "node_type": nt,
                "confidence": conf,
                "edge_count": ec,
            })
        })
        .collect();
    suggestions.sort_by(|a, b| {
        let ea = a.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let eb = b.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        eb.cmp(&ea)
    });
    suggestions.truncate(20);

    Ok(Json(serde_json::json!({
        "label": label,
        "existing_watches": existing_watches.into_iter().collect::<Vec<_>>(),
        "suggestions": suggestions,
    })))
}

#[cfg(not(feature = "assess"))]
pub async fn suggest_watches_for_assessment() -> impl axum::response::IntoResponse {
    feature_not_enabled("assess")
}
