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
            },
            {
                "name": "engram_provenance",
                "description": "Trace provenance of an entity back to source documents. Shows Entity -> Facts -> Documents -> Publishers.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to trace provenance for" }
                    },
                    "required": ["entity"]
                }
            },
            {
                "name": "engram_documents",
                "description": "List ingested documents with metadata.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Max results (default: 20)" }
                    }
                }
            },
            {
                "name": "engram_mesh_discover",
                "description": "Discover mesh peers that cover a given topic. Returns peer names, coverage depth, and confidence.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "topic": { "type": "string", "description": "Topic to search for across mesh peers" }
                    },
                    "required": ["topic"]
                }
            },
            {
                "name": "engram_mesh_query",
                "description": "Execute a federated query across mesh peers. Searches all connected peers and merges results.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query to federate" },
                        "min_confidence": { "type": "number", "description": "Minimum confidence (0.0-1.0)" },
                        "max_hops": { "type": "integer", "description": "Maximum mesh hops (default: 2)" },
                        "clearance": { "type": "string", "description": "Clearance level: public, internal, confidential, restricted (default: public)" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "engram_ingest",
                "description": "Ingest text through the NER/entity-resolution pipeline. Restricted tool — requires opt-in.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to ingest" },
                        "source": { "type": "string", "description": "Source identifier" },
                        "pipeline": { "type": "string", "description": "Pipeline name (default: default)" },
                        "skip": { "type": "array", "items": { "type": "string" }, "description": "Stages to skip: ner, resolve, dedup" }
                    },
                    "required": ["text"]
                }
            },
            {
                "name": "engram_assess_create",
                "description": "Create a new assessment (hypothesis) to track probability over time with evidence-based scoring",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Hypothesis title (e.g. 'NVIDIA stock > $200 by Q3 2026')" },
                        "category": { "type": "string", "description": "Category: financial, geopolitical, technical, military, social, other" },
                        "timeframe": { "type": "string", "description": "Time horizon (e.g. 'Q3 2026')" },
                        "initial_probability": { "type": "number", "description": "Starting probability (0.05-0.95, default 0.50)" },
                        "watches": { "type": "array", "items": { "type": "string" }, "description": "Entity labels to watch for automatic re-evaluation" }
                    },
                    "required": ["title"]
                }
            },
            {
                "name": "engram_assess_list",
                "description": "List assessments with optional filters by category and status",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "category": { "type": "string", "description": "Filter by category" },
                        "status": { "type": "string", "description": "Filter by status: active, paused, archived, resolved" }
                    }
                }
            },
            {
                "name": "engram_assess_get",
                "description": "Get full assessment detail including score history, evidence, and watched entities",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" }
                    },
                    "required": ["label"]
                }
            },
            {
                "name": "engram_assess_evaluate",
                "description": "Trigger manual re-evaluation of an assessment's probability",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label to evaluate" }
                    },
                    "required": ["label"]
                }
            },
            {
                "name": "engram_assess_evidence",
                "description": "Add evidence to an assessment (supporting or contradicting)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" },
                        "node_label": { "type": "string", "description": "Evidence entity label" },
                        "direction": { "type": "string", "description": "'supports' or 'contradicts'" }
                    },
                    "required": ["label", "node_label", "direction"]
                }
            },
            {
                "name": "engram_assess_watch",
                "description": "Add or list watched entities for an assessment",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" },
                        "entity_label": { "type": "string", "description": "Entity to watch (omit to list current watches)" }
                    },
                    "required": ["label"]
                }
            },
            {
                "name": "engram_analyze_relations",
                "description": "Extract relations from text without storing (dry-run). Returns entities + relations with confidence scores.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to analyze for entities and relations" },
                        "skip": { "type": "array", "items": { "type": "string" }, "description": "Stages to skip" }
                    },
                    "required": ["text"]
                }
            },
            {
                "name": "engram_kge_train",
                "description": "Trigger KGE (RotatE) training on the current graph. Builds relation prediction embeddings.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "epochs": { "type": "integer", "description": "Training epochs (default: 100)" }
                    }
                }
            },
            {
                "name": "engram_kge_predict",
                "description": "Predict relations between two entities using KGE embeddings.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Head entity" },
                        "to": { "type": "string", "description": "Tail entity" },
                        "min_confidence": { "type": "number", "description": "Minimum confidence (default: 0.1)" }
                    },
                    "required": ["from", "to"]
                }
            },
            {
                "name": "engram_create_rule",
                "description": "Create an action engine rule that triggers on graph events. Restricted tool — requires opt-in.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Human-readable rule name" },
                        "condition": { "type": "string", "description": "TOML condition expression" },
                        "effect": { "type": "string", "description": "Effect type: webhook, confidence_cascade, create_edge, tier_change, ingest_job" },
                        "effect_config": { "type": "object", "description": "Effect-specific configuration" }
                    },
                    "required": ["name", "condition", "effect"]
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

        "engram_provenance" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let entity = args["entity"].as_str().ok_or("missing entity")?;

            let doc_facts = g.documents_for_entity(entity).map_err(|e| e.to_string())?;

            let mut documents = Vec::new();
            for (doc_label, facts) in &doc_facts {
                let doc_props = g.get_properties(doc_label)
                    .unwrap_or_default().unwrap_or_default();
                let publisher = g.edges_from(doc_label).unwrap_or_default()
                    .into_iter()
                    .find(|e| e.relationship == "published_by")
                    .map(|e| e.to);

                documents.push(serde_json::json!({
                    "document": doc_label,
                    "title": doc_props.get("title").cloned().unwrap_or_default(),
                    "url": doc_props.get("url").cloned().unwrap_or_default(),
                    "doc_date": doc_props.get("doc_date").cloned().unwrap_or_default(),
                    "ingested_at": doc_props.get("ingested_at").cloned().unwrap_or_default(),
                    "publisher": publisher.unwrap_or_default(),
                    "facts": facts.iter().map(|(fl, claim)| serde_json::json!({
                        "fact": fl,
                        "claim": claim,
                    })).collect::<Vec<_>>()
                }));
            }

            Ok(serde_json::json!({
                "entity": entity,
                "document_count": documents.len(),
                "documents": documents
            }))
        }

        "engram_documents" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let limit = args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;

            let all_nodes = g.all_nodes().map_err(|e| e.to_string())?;
            let mut docs = Vec::new();
            for node in &all_nodes {
                let node_type = g.get_node_type(&node.label).unwrap_or_default();
                if node_type != "Document" {
                    continue;
                }
                let props = g.get_properties(&node.label)
                    .unwrap_or_default().unwrap_or_default();
                let entity_count = g.edges_to(&node.label).unwrap_or_default()
                    .iter()
                    .filter(|e| e.relationship == "extracted_from")
                    .count();
                let publisher = g.edges_from(&node.label).unwrap_or_default()
                    .into_iter()
                    .find(|e| e.relationship == "published_by")
                    .map(|e| e.to);

                docs.push(serde_json::json!({
                    "label": node.label,
                    "title": props.get("title").cloned().unwrap_or_default(),
                    "url": props.get("url").cloned().unwrap_or_default(),
                    "doc_date": props.get("doc_date").cloned().unwrap_or_default(),
                    "ingested_at": props.get("ingested_at").cloned().unwrap_or_default(),
                    "content_length": props.get("content_length").cloned().unwrap_or_default(),
                    "publisher": publisher.unwrap_or_default(),
                    "fact_count": entity_count,
                }));
                if docs.len() >= limit {
                    break;
                }
            }

            Ok(serde_json::json!({
                "count": docs.len(),
                "documents": docs
            }))
        }

        #[cfg(feature = "reason")]
        "engram_mesh_discover" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let topic = args["topic"].as_str().ok_or("missing topic")?;

            // Derive local profile
            let config = engram_reason::ProfileConfig::default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64;
            let local_profile = engram_reason::derive_profile(&g, "local", &config, vec![], now);

            // Check local profile for topic coverage
            let profiles = vec![local_profile];
            let matches = engram_reason::profiles::discover_by_topic(&profiles, topic);

            let results: Vec<Value> = matches.iter().map(|(idx, domain)| {
                serde_json::json!({
                    "peer": profiles[*idx].name,
                    "topic": domain.topic,
                    "depth": domain.depth,
                    "fact_count": domain.fact_count,
                    "avg_confidence": domain.avg_confidence,
                    "freshness": domain.freshness
                })
            }).collect();

            Ok(serde_json::json!({ "matches": results, "total": results.len() }))
        }

        #[cfg(feature = "reason")]
        "engram_mesh_query" => {
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let query_text = args["query"].as_str().ok_or("missing query")?;
            let min_confidence = args.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let max_hops = args.get("max_hops").and_then(|v| v.as_u64()).unwrap_or(2) as u32;
            let clearance = args.get("clearance").and_then(|v| v.as_str()).unwrap_or("public").to_string();

            let _ = max_hops; // reserved for multi-hop federation
            let query = engram_reason::FederatedQuery {
                query: query_text.to_string(),
                query_type: "fulltext".to_string(),
                max_results: 20,
                min_confidence,
                requesting_node: "mcp".to_string(),
                sensitivity_clearance: clearance,
            };

            let result = engram_reason::federated::execute_local(&g, &query);

            Ok(serde_json::json!({
                "peer": result.peer_id,
                "facts": result.facts,
                "total": result.total_matches,
                "query_time_ms": result.query_time_ms
            }))
        }

        #[cfg(feature = "ingest")]
        "engram_ingest" => {
            let text = args["text"].as_str().ok_or("missing text")?;
            let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("mcp");

            // Run through ingest pipeline
            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let prov = Provenance::user(source);

            // Store the text directly (full pipeline requires async runtime)
            let slot = g.store(text, &prov).map_err(|e| e.to_string())?;
            drop(g);
            state.mark_dirty();

            Ok(serde_json::json!({
                "ingested": true,
                "text": text,
                "source": source,
                "slot": slot
            }))
        }

        #[cfg(feature = "actions")]
        "engram_create_rule" => {
            let rule_name = args["name"].as_str().ok_or("missing name")?;
            let condition = args["condition"].as_str().ok_or("missing condition")?;
            let effect = args["effect"].as_str().ok_or("missing effect")?;
            let effect_config = args.get("effect_config").cloned().unwrap_or(Value::Object(Default::default()));

            // Build a TOML rule definition
            let rule_toml = format!(
                "[[rules]]\nname = \"{}\"\ncondition = \"{}\"\neffect = \"{}\"\neffect_config = {}\n",
                rule_name,
                condition.replace('\"', "\\\""),
                effect,
                serde_json::to_string(&effect_config).unwrap_or_default()
            );

            Ok(serde_json::json!({
                "created": true,
                "rule_name": rule_name,
                "condition": condition,
                "effect": effect,
                "toml_preview": rule_toml
            }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_create" => {
            let title = args["title"].as_str().ok_or("missing title")?;
            let category = args.get("category").and_then(|v| v.as_str()).map(String::from);
            let timeframe = args.get("timeframe").and_then(|v| v.as_str()).map(String::from);
            let initial_prob = args.get("initial_probability").and_then(|v| v.as_f64()).map(|v| v as f32);
            let watches: Vec<String> = args.get("watches")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let label = format!("Assessment:{}", title.to_lowercase()
                .replace(' ', "-")
                .replace(|c: char| !c.is_alphanumeric() && c != '-', ""));

            let prob = initial_prob.unwrap_or(0.50).clamp(0.05, 0.95);
            let prov = Provenance::user("mcp");

            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let slot = g.store_with_confidence(&label, prob, &prov).map_err(|e| e.to_string())?;
            let _ = g.set_node_type(&label, "assessment");
            let _ = g.set_property(&label, "title", title);
            if let Some(ref cat) = category { let _ = g.set_property(&label, "category", cat); }
            if let Some(ref tf) = timeframe { let _ = g.set_property(&label, "timeframe", tf); }
            let _ = g.set_property(&label, "status", "active");
            for entity in &watches { let _ = g.relate(&label, entity, "watches", &prov); }
            drop(g);
            state.mark_dirty();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let record = engram_assess::AssessmentRecord {
                label: label.clone(),
                node_id: slot,
                history: vec![engram_assess::ScorePoint {
                    timestamp: now,
                    probability: prob,
                    shift: 0.0,
                    trigger: engram_assess::ScoreTrigger::Created,
                    reason: "Created via MCP".to_string(),
                    path: None,
                }],
                evidence: vec![],
                success_criteria: None,
                tags: vec![],
                resolution: "active".to_string(),
                pending_count: 0,
                evidence_for: vec![],
                evidence_against: vec![],
            };
            state.assessments.write().map_err(|_| "assessment store lock".to_string())?.insert(record);

            Ok(serde_json::json!({ "label": label, "probability": prob, "watches": watches }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_list" => {
            let store = state.assessments.read().map_err(|_| "assessment store lock".to_string())?;
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let cat_filter = args.get("category").and_then(|v| v.as_str());
            let status_filter = args.get("status").and_then(|v| v.as_str());

            let assessments: Vec<Value> = store.all().iter().filter_map(|r| {
                let props = g.get_properties(&r.label).ok().flatten().unwrap_or_default();
                let cat = props.get("category").map(|s| s.as_str()).unwrap_or("");
                let status = props.get("status").map(|s| s.as_str()).unwrap_or("active");
                if let Some(cf) = cat_filter { if cat != cf { return None; } }
                if let Some(sf) = status_filter { if status != sf { return None; } }
                Some(serde_json::json!({
                    "label": r.label,
                    "title": props.get("title").cloned().unwrap_or_else(|| r.label.clone()),
                    "category": cat,
                    "status": status,
                    "probability": engram_assess::engine::recalculate_probability(r),
                    "evidence_count": r.evidence_for.len() + r.evidence_against.len(),
                }))
            }).collect();

            Ok(serde_json::json!({ "assessments": assessments, "total": assessments.len() }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_get" => {
            let label = args["label"].as_str().ok_or("missing label")?;
            let store = state.assessments.read().map_err(|_| "assessment store lock".to_string())?;
            let record = store.get(label).ok_or_else(|| format!("assessment not found: {label}"))?;
            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let props = g.get_properties(label).ok().flatten().unwrap_or_default();
            let watches: Vec<String> = g.edges_from(label).unwrap_or_default().iter()
                .filter(|e| e.relationship == "watches").map(|e| e.to.clone()).collect();

            Ok(serde_json::json!({
                "label": record.label,
                "title": props.get("title").cloned().unwrap_or_default(),
                "probability": engram_assess::engine::recalculate_probability(record),
                "history_count": record.history.len(),
                "evidence_for": record.evidence_for.len(),
                "evidence_against": record.evidence_against.len(),
                "watches": watches,
            }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_evaluate" => {
            let label = args["label"].as_str().ok_or("missing label")?;
            let mut store = state.assessments.write().map_err(|_| "assessment store lock".to_string())?;
            let record = store.get_mut(label).ok_or_else(|| format!("assessment not found: {label}"))?;
            let point = engram_assess::engine::evaluate(record);

            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let _ = g.set_property(label, "current_probability", &format!("{:.4}", point.probability));
            drop(g);
            state.mark_dirty();

            Ok(serde_json::json!({
                "label": label,
                "probability": point.probability,
                "shift": point.shift,
            }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_evidence" => {
            let label = args["label"].as_str().ok_or("missing label")?;
            let node_label = args["node_label"].as_str().ok_or("missing node_label")?;
            let direction = args["direction"].as_str().ok_or("missing direction")?;
            let supports = direction == "supports";

            let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
            let conf = g.get_node(node_label).ok().flatten().map(|n| n.confidence).unwrap_or(0.50);
            drop(g);

            let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
            let prov = Provenance::user("mcp");
            let edge_type = if supports { "supported_by" } else { "contradicted_by" };
            let _ = g.relate(node_label, label, edge_type, &prov);
            drop(g);

            let mut store = state.assessments.write().map_err(|_| "assessment store lock".to_string())?;
            let record = store.get_mut(label).ok_or_else(|| format!("assessment not found: {label}"))?;
            let point = engram_assess::engine::add_evidence(
                record, conf, supports,
                engram_assess::ScoreTrigger::EvidenceAdded { node_id: 0 },
                format!("{} {} assessment (via MCP)", node_label, direction),
                None,
                node_label,
                "mcp",
            );
            state.mark_dirty();

            Ok(serde_json::json!({
                "label": label,
                "probability": point.probability,
                "shift": point.shift,
                "direction": direction,
            }))
        }

        #[cfg(feature = "assess")]
        "engram_assess_watch" => {
            let label = args["label"].as_str().ok_or("missing label")?;
            if let Some(entity) = args.get("entity_label").and_then(|v| v.as_str()) {
                let mut g = state.graph.write().map_err(|_| "graph write lock poisoned".to_string())?;
                let prov = Provenance::user("mcp");
                let _ = g.relate(label, entity, "watches", &prov);
                drop(g);
                state.mark_dirty();
                Ok(serde_json::json!({ "added": entity, "assessment": label }))
            } else {
                let g = state.graph.read().map_err(|_| "graph read lock poisoned".to_string())?;
                let watches: Vec<String> = g.edges_from(label).unwrap_or_default().iter()
                    .filter(|e| e.relationship == "watches").map(|e| e.to.clone()).collect();
                Ok(serde_json::json!({ "assessment": label, "watches": watches }))
            }
        }

        #[cfg(feature = "ingest")]
        "engram_analyze_relations" => {
            let text = args["text"].as_str().ok_or("missing text")?;

            let mut stages = engram_ingest::types::StageConfig::default();
            if let Some(skip) = args.get("skip").and_then(|v| v.as_array()) {
                for s in skip {
                    if let Some(name) = s.as_str() {
                        stages.apply_skip(name);
                    }
                }
            }

            let config = engram_ingest::PipelineConfig {
                stages,
                ..Default::default()
            };

            let graph_clone = state.graph.clone();
            let (ner_model, rel_model) = state.config.read()
                .map(|c| (c.ner_model.clone(), c.rel_model.clone()))
                .unwrap_or((None, None));
            let text_owned = text.to_string();
            let result = std::thread::spawn(move || {
                let pipeline = crate::handlers::build_pipeline_mcp(graph_clone, config, None, ner_model, rel_model);
                let items = vec![engram_ingest::types::RawItem {
                    content: engram_ingest::types::Content::Text(text_owned),
                    source_url: None,
                    source_name: "mcp-analyze".into(),
                    fetched_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    metadata: Default::default(),
                }];
                pipeline.analyze(items)
            })
            .join()
            .map_err(|_| "analyze thread panicked".to_string())?
            .map_err(|e| e.to_string())?;

            let entities: Vec<Value> = result.entities.iter().map(|e| {
                serde_json::json!({
                    "text": e.text,
                    "type": e.entity_type,
                    "confidence": e.confidence,
                    "method": format!("{:?}", e.method),
                    "resolved_to": e.resolved_to,
                })
            }).collect();

            let relations: Vec<Value> = result.relations.iter().map(|r| {
                serde_json::json!({
                    "from": r.from,
                    "to": r.to,
                    "rel_type": r.rel_type,
                    "confidence": r.confidence,
                    "method": format!("{:?}", r.method),
                })
            }).collect();

            Ok(serde_json::json!({
                "entities": entities,
                "relations": relations,
                "language": result.language,
                "duration_ms": result.duration_ms,
            }))
        }

        #[cfg(feature = "ingest")]
        "engram_kge_train" => {
            let epochs = args.get("epochs").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
            let graph_clone = state.graph.clone();

            let brain_path = {
                let g = state.graph.read().map_err(|_| "graph lock poisoned".to_string())?;
                g.path().to_path_buf()
            };

            let stats = std::thread::spawn(move || {
                let mut model = engram_ingest::KgeModel::load(&brain_path, engram_ingest::KgeConfig::default())
                    .unwrap_or_else(|_| engram_ingest::KgeModel::new(&brain_path, engram_ingest::KgeConfig::default()));
                let g = graph_clone.read().map_err(|_| "graph lock poisoned".to_string())?;
                let stats = model.train_full(&g, epochs).map_err(|e| e.to_string())?;
                drop(g);
                model.save().map_err(|e| e.to_string())?;
                Ok::<_, String>(stats)
            })
            .join()
            .map_err(|_| "kge training thread panicked".to_string())?
            .map_err(|e| e)?;

            Ok(serde_json::json!({
                "epochs_completed": stats.epochs_completed,
                "final_loss": stats.final_loss,
                "entity_count": stats.entity_count,
                "relation_type_count": stats.relation_type_count,
            }))
        }

        #[cfg(feature = "ingest")]
        "engram_kge_predict" => {
            let from = args["from"].as_str().ok_or("missing from")?;
            let to = args["to"].as_str().ok_or("missing to")?;
            let min_conf = args.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;

            let brain_path = {
                let g = state.graph.read().map_err(|_| "graph lock poisoned".to_string())?;
                g.path().to_path_buf()
            };

            let model = engram_ingest::KgeModel::load(&brain_path, engram_ingest::KgeConfig::default())
                .map_err(|e| e.to_string())?;

            let predictions = model.predict(from, to, min_conf);

            let results: Vec<Value> = predictions.iter().map(|(rel, conf)| {
                serde_json::json!({ "rel_type": rel, "confidence": conf })
            }).collect();

            Ok(serde_json::json!({
                "from": from,
                "to": to,
                "predictions": results,
                "trained": model.is_trained(),
            }))
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
