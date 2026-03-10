/// MCP (Model Context Protocol) server — JSON-RPC over stdio.
///
/// Implements the MCP specification for native Claude/Cursor/IDE integration.
/// Tools: engram_store, engram_query, engram_ask, engram_tell, engram_prove, engram_explain, engram_search
/// Resources: engram://stats, engram://health

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

use engram_core::graph::Provenance;

use crate::state::AppState;

// ── JSON-RPC types ──

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i32, message: String) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

// ── MCP tool/resource definitions ──

fn tool_definitions() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "engram_store",
                "description": "Store a new fact or entity in the knowledge graph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Name/label of the entity" },
                        "type": { "type": "string", "description": "Entity type (person, server, concept, event, ...)" },
                        "properties": { "type": "object", "description": "Key-value properties", "additionalProperties": { "type": "string" } },
                        "source": { "type": "string", "description": "Where this knowledge comes from" },
                        "confidence": { "type": "number", "description": "How certain (0.0-1.0)" }
                    },
                    "required": ["entity"]
                }
            },
            {
                "name": "engram_relate",
                "description": "Create a relationship between two entities",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity" },
                        "to": { "type": "string", "description": "Target entity" },
                        "relationship": { "type": "string", "description": "Type of relationship (causes, is_a, part_of, ...)" },
                        "confidence": { "type": "number", "description": "Relationship confidence" }
                    },
                    "required": ["from", "to", "relationship"]
                }
            },
            {
                "name": "engram_query",
                "description": "Query the knowledge graph with traversal",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "start": { "type": "string", "description": "Starting entity" },
                        "relationship": { "type": "string", "description": "Filter by relationship type" },
                        "depth": { "type": "integer", "description": "Max traversal depth" },
                        "min_confidence": { "type": "number", "description": "Minimum confidence threshold" },
                        "direction": { "type": "string", "description": "Traversal direction: out, in, or both (default: both)" }
                    },
                    "required": ["start"]
                }
            },
            {
                "name": "engram_search",
                "description": "Full-text keyword search across all stored knowledge",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "description": "Max results to return" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "engram_prove",
                "description": "Find evidence for or against a hypothesis using backward chaining",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity" },
                        "relationship": { "type": "string", "description": "Relationship to prove" },
                        "to": { "type": "string", "description": "Target entity" }
                    },
                    "required": ["from", "relationship", "to"]
                }
            },
            {
                "name": "engram_explain",
                "description": "Explain how a fact was derived, its confidence, and provenance",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to explain" }
                    },
                    "required": ["entity"]
                }
            },
            {
                "name": "engram_gaps",
                "description": "List knowledge gaps (black areas) ranked by severity. Detects frontier nodes, structural holes, temporal gaps, confidence deserts, and coordinated clusters.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "min_severity": { "type": "number", "description": "Minimum severity (0.0-1.0, default: 0.3)" },
                        "limit": { "type": "integer", "description": "Max results (default: 20)" }
                    }
                }
            },
            {
                "name": "engram_frontier",
                "description": "List frontier nodes — entities at the edge of knowledge with few connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

fn resource_definitions() -> Value {
    serde_json::json!({
        "resources": [
            {
                "uri": "engram://stats",
                "name": "Graph Statistics",
                "description": "Node and edge counts",
                "mimeType": "application/json"
            },
            {
                "uri": "engram://health",
                "name": "System Health",
                "description": "Server status",
                "mimeType": "application/json"
            }
        ]
    })
}

// ── Tool execution ──

fn execute_tool(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        // Write operations — acquire write lock, mark dirty
        "engram_store" => {
            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let entity = args["entity"].as_str().ok_or("missing entity")?;
            let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("mcp");
            let prov = Provenance::user(source);

            let slot = if let Some(conf) = args.get("confidence").and_then(|v| v.as_f64()) {
                g.store_with_confidence(entity, conf as f32, &prov)
            } else {
                g.store(entity, &prov)
            }
            .map_err(|e| e.to_string())?;

            if let Some(t) = args.get("type").and_then(|v| v.as_str()) {
                let _ = g.set_node_type(entity, t);
            }

            if let Some(props) = args.get("properties").and_then(|v| v.as_object()) {
                for (k, v) in props {
                    if let Some(val) = v.as_str() {
                        let _ = g.set_property(entity, k, val);
                    }
                }
            }

            drop(g);
            state.mark_dirty();
            Ok(serde_json::json!({ "stored": entity, "slot": slot }))
        }

        "engram_relate" => {
            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let from = args["from"].as_str().ok_or("missing from")?;
            let to = args["to"].as_str().ok_or("missing to")?;
            let rel = args["relationship"].as_str().ok_or("missing relationship")?;
            let prov = Provenance::user("mcp");

            let slot = if let Some(conf) = args.get("confidence").and_then(|v| v.as_f64()) {
                g.relate_with_confidence(from, to, rel, conf as f32, &prov)
            } else {
                g.relate(from, to, rel, &prov)
            }
            .map_err(|e| e.to_string())?;

            drop(g);
            state.mark_dirty();
            Ok(serde_json::json!({ "from": from, "to": to, "relationship": rel, "edge_slot": slot }))
        }

        // Read operations — acquire read lock only
        "engram_query" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let start = args["start"].as_str().ok_or("missing start")?;
            let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as u32;
            let min_conf = args.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("both");

            let result = g.traverse_directed(start, depth, min_conf, direction).map_err(|e| e.to_string())?;

            let nodes: Vec<Value> = result.nodes.iter().filter_map(|&nid| {
                let node = g.get_node_by_id(nid).ok()??;
                if node.confidence >= min_conf {
                    Some(serde_json::json!({
                        "label": g.label_for_id(nid).unwrap_or_else(|_| node.label().to_string()),
                        "confidence": node.confidence,
                        "depth": result.depths.get(&nid)
                    }))
                } else {
                    None
                }
            }).collect();

            Ok(serde_json::json!({ "nodes": nodes, "count": nodes.len() }))
        }

        "engram_search" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let query = args["query"].as_str().ok_or("missing query")?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let results = g.search(query, limit).map_err(|e| e.to_string())?;

            let hits: Vec<Value> = results.into_iter().map(|r| {
                serde_json::json!({
                    "label": r.label,
                    "confidence": r.confidence,
                    "score": r.score
                })
            }).collect();

            Ok(serde_json::json!({ "results": hits, "total": hits.len() }))
        }

        "engram_prove" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let from = args["from"].as_str().ok_or("missing from")?;
            let rel = args["relationship"].as_str().ok_or("missing relationship")?;
            let to = args["to"].as_str().ok_or("missing to")?;

            let result = g.prove(from, to, rel, 5).map_err(|e| e.to_string())?;

            let chain: Vec<Value> = result.chain.iter().map(|step| {
                serde_json::json!({
                    "fact": step.fact,
                    "confidence": step.confidence,
                    "evidence": step.evidence,
                    "depth": step.depth
                })
            }).collect();

            Ok(serde_json::json!({
                "supported": result.supported,
                "confidence": result.confidence,
                "chain": chain
            }))
        }

        "engram_explain" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let entity = args["entity"].as_str().ok_or("missing entity")?;

            let confidence = g.get_node(entity).map_err(|e| e.to_string())?
                .ok_or_else(|| format!("node not found: {entity}"))?.confidence;

            let props = g.get_properties(entity).map_err(|e| e.to_string())?.unwrap_or_default();
            let edges = g.edges_from(entity).unwrap_or_default();
            let cooccurrences = g.cooccurrences_for(entity);

            Ok(serde_json::json!({
                "entity": entity,
                "confidence": confidence,
                "properties": props,
                "edges": edges.iter().map(|e| serde_json::json!({
                    "to": e.to,
                    "relationship": e.relationship,
                    "confidence": e.confidence
                })).collect::<Vec<_>>(),
                "cooccurrences": cooccurrences.iter().map(|(e, c)| serde_json::json!({
                    "entity": e,
                    "count": c
                })).collect::<Vec<_>>()
            }))
        }

        #[cfg(feature = "reason")]
        "engram_gaps" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let min_severity = args.get("min_severity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.3) as f32;
            let limit = args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;

            let config = engram_reason::DetectionConfig::default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64;

            let (gaps, report) = engram_reason::scan(&g, &config, now)
                .map_err(|e| e.to_string())?;

            let filtered: Vec<_> = gaps.into_iter()
                .filter(|gap| gap.severity >= min_severity)
                .take(limit)
                .collect();

            Ok(serde_json::json!({
                "gaps": filtered,
                "report": report
            }))
        }

        #[cfg(feature = "reason")]
        "engram_frontier" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let nodes = g.all_nodes().map_err(|e| e.to_string())?;
            let config = engram_reason::DetectionConfig::default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64;

            let mut gaps = engram_reason::frontier::detect_frontier_nodes(&nodes, &config, now);
            gaps.extend(engram_reason::frontier::detect_isolated_nodes(&nodes, now));
            engram_reason::scoring::rank_gaps(&mut gaps);

            Ok(serde_json::json!({ "frontier": gaps }))
        }

        _ => Err(format!("unknown tool: {name}")),
    }
}

fn read_resource(state: &AppState, uri: &str) -> Result<Value, String> {
    let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;

    match uri {
        "engram://stats" => {
            let (nodes, edges) = g.stats();
            Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
        }
        "engram://health" => {
            Ok(serde_json::json!({ "status": "ok" }))
        }
        _ => Err(format!("unknown resource: {uri}")),
    }
}

// ── MCP stdio server loop ──

/// Run the MCP server over stdio (blocking).
pub fn run_stdio(state: AppState) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(
                    Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                );
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(Value::Null);

        let resp = match req.method.as_str() {
            "initialize" => {
                JsonRpcResponse::success(id, serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {},
                        "resources": {}
                    },
                    "serverInfo": {
                        "name": "engram",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }))
            }

            "notifications/initialized" => continue, // no response needed

            "tools/list" => {
                JsonRpcResponse::success(id, tool_definitions())
            }

            "tools/call" => {
                let params = req.params.unwrap_or(Value::Null);
                let tool_name = params["name"].as_str().unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(Value::Object(Default::default()));

                match execute_tool(&state, tool_name, &arguments) {
                    Ok(result) => JsonRpcResponse::success(id, serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                        }]
                    })),
                    Err(e) => JsonRpcResponse::error(id, -32603, e),
                }
            }

            "resources/list" => {
                JsonRpcResponse::success(id, resource_definitions())
            }

            "resources/read" => {
                let params = req.params.unwrap_or(Value::Null);
                let uri = params["uri"].as_str().unwrap_or("");

                match read_resource(&state, uri) {
                    Ok(result) => JsonRpcResponse::success(id, serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": serde_json::to_string(&result).unwrap_or_default()
                        }]
                    })),
                    Err(e) => JsonRpcResponse::error(id, -32603, e),
                }
            }

            _ => JsonRpcResponse::error(id, -32601, format!("method not found: {}", req.method)),
        };

        let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
        let _ = stdout.flush();
    }
}
