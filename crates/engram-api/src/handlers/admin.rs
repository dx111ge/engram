use super::*;

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
    let compute = state.compute.read().unwrap().clone();
    Json(compute)
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
    drop(g);

    // Persist to config
    {
        let mut cfg = state.config.write().unwrap_or_else(|e| e.into_inner());
        cfg.quantization_enabled = Some(req.enabled);
        if let Some(ref path) = state.config_path {
            let _ = cfg.save(path);
        }
    }

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
    super::query::get_node(State(state), Path(req.label)).await
}

pub async fn delete_node_by_body(
    State(state): State<AppState>,
    Json(req): Json<LabelBody>,
) -> ApiResult<DeleteResponse> {
    super::query::delete_node(State(state), Path(req.label)).await
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
    let trust = req.trust.unwrap_or(0.5).clamp(0.0, 1.0);
    let mut nodes_imported: u32 = 0;
    let mut nodes_merged: u32 = 0;
    let mut edges_imported: u32 = 0;
    let mut edges_merged: u32 = 0;
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

    // First pass: create or merge nodes
    for item in &items {
        let label = extract_label(item);
        if label.is_empty() {
            continue;
        }

        let import_conf = item.get("engram:confidence")
            .or_else(|| item.get("confidence"))
            .and_then(|v| v.as_f64())
            .map(|c| c as f32);

        // Check if node already exists
        let existing = g.find_node_id(&label).unwrap_or(None);

        if let Some(_existing_id) = existing {
            // Node exists -- merge confidence using trust-weighted formula
            if let Some(c_import) = import_conf {
                if let Ok(Some(c_local)) = g.node_confidence(&label) {
                    let c_new = c_local + trust * (c_import - c_local);
                    let _ = g.set_node_confidence(&label, c_new);
                }
            }
            nodes_merged += 1;
        } else {
            // New node -- create with imported confidence
            let store_result = if let Some(c) = import_conf {
                g.store_with_confidence(&label, c, &prov)
            } else {
                g.store(&label, &prov)
            };
            match store_result {
                Ok(_) => { nodes_imported += 1; }
                Err(e) => { errors.push(format!("store {label}: {e}")); continue; }
            }
        }

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
                if let Some(s) = v.as_str() {
                    let prop_key = strip_prefix(k);
                    if !prop_key.is_empty() {
                        let _ = g.set_property(&label, &prop_key, s);
                    }
                }
            }
        }
    }

    // Second pass: create or merge edges
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
                let targets = if v.is_array() {
                    v.as_array().unwrap().clone()
                } else if v.is_object() {
                    vec![v.clone()]
                } else {
                    continue;
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

                        let c_import = target.get("engram:confidence")
                            .or_else(|| target.get("confidence"))
                            .and_then(|v| v.as_f64())
                            .map(|c| c as f32)
                            .unwrap_or(0.8);

                        // Check if edge already exists
                        match g.find_edge_slot(&from_label, &to_label, &rel) {
                            Ok(Some(slot)) => {
                                // Edge exists -- merge confidence
                                if let Ok(c_local) = g.edge_confidence(slot) {
                                    let c_new = c_local + trust * (c_import - c_local);
                                    let _ = g.update_edge_confidence(slot, c_new);
                                }
                                edges_merged += 1;
                            }
                            Ok(None) => {
                                // New edge -- create
                                match g.relate_with_confidence(&from_label, &to_label, &rel, c_import, &prov) {
                                    Ok(_) => { edges_imported += 1; }
                                    Err(e) => errors.push(format!("relate {from_label} -> {to_label}: {e}")),
                                }
                            }
                            Err(e) => errors.push(format!("find edge {from_label} -> {to_label}: {e}")),
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
        nodes_merged,
        edges_merged,
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

/// Normalize an LLM endpoint URL to produce a full `/chat/completions` URL.
///
/// Handles all common provider patterns:
///   `http://localhost:11434`                -> `http://localhost:11434/v1/chat/completions`
///   `http://localhost:11434/v1`             -> `http://localhost:11434/v1/chat/completions`
///   `http://localhost:11434/v1/chat/completions` -> as-is
///   `https://api.openai.com/v1`            -> `https://api.openai.com/v1/chat/completions`
///   `http://localhost:8000/v1`             -> `http://localhost:8000/v1/chat/completions`  (vLLM)
///   `http://localhost:1234/v1`             -> `http://localhost:1234/v1/chat/completions`  (LM Studio)
pub(crate) fn normalize_llm_endpoint(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');

    // Already a full chat completions URL
    if s.ends_with("/chat/completions") {
        return s.to_string();
    }

    // Strip /completions if user partially entered it
    let s = s.strip_suffix("/completions").unwrap_or(s);

    // Has a recognized API prefix -- just append /chat/completions
    if s.ends_with("/v1") || s.ends_with("/api") || s.ends_with("/v2") {
        return format!("{s}/chat/completions");
    }

    // Bare host+port (no meaningful path) -- add /v1/chat/completions
    let after_scheme = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else if let Some(rest) = s.strip_prefix("http://") {
        rest
    } else {
        return format!("{s}/chat/completions");
    };

    let path = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => "",
    };

    if path.is_empty() || path == "/" {
        format!("{s}/v1/chat/completions")
    } else {
        format!("{s}/chat/completions")
    }
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

// ── POST /admin/dedup-edges ──

pub async fn admin_dedup_edges(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let mut g = state.graph.write().map_err(|_| write_lock_err())?;
    let removed = g.dedup_edges();
    drop(g);
    if removed > 0 {
        state.mark_dirty();
    }
    Ok(Json(serde_json::json!({
        "duplicates_removed": removed,
    })))
}

// ── POST /admin/reset ──

pub async fn admin_reset(
    State(state): State<AppState>,
) -> ApiResult<ResetResponse> {
    let brain_path = {
        let g = state.graph.read().map_err(|_| read_lock_err())?;
        g.path().to_path_buf()
    };

    {
        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
        g.reset().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        g.checkpoint().map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let sidecars_cleaned = cleanup_sidecars(&brain_path);

    Ok(Json(ResetResponse {
        success: true,
        sidecars_cleaned,
    }))
}

fn cleanup_sidecars(brain_path: &std::path::Path) -> Vec<String> {
    let mut cleaned = Vec::new();
    let delete_extensions = [
        "props", "types", "vectors", "cooccur", "wal", "rules",
        "schedules", "peers", "audit", "assessments", "relgaz", "kge",
        "ledger",
    ];
    for ext in &delete_extensions {
        let sidecar = brain_path.with_extension(ext);
        if sidecar.exists() {
            if std::fs::remove_file(&sidecar).is_ok() {
                cleaned.push(ext.to_string());
            }
        }
    }
    cleaned
}
