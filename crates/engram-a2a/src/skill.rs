/// Skill routing — maps A2A skill IDs to engram operations.
///
/// Each skill corresponds to a set of graph operations. The router
/// parses the incoming message, executes against the graph, and
/// returns artifacts.

use std::sync::{Arc, RwLock};

use engram_core::graph::{Graph, Provenance};

#[allow(unused_imports)]
use crate::task::{Artifact, MessagePart, TaskMessage, TaskRequest, TaskResponse};

/// Route a task to the appropriate skill handler.
pub fn route_task(
    request: &TaskRequest,
    graph: &Arc<RwLock<Graph>>,
) -> TaskResponse {
    let task_id = request.id.as_deref().unwrap_or("auto");
    let text = request.message.text();

    match request.skill_id.as_str() {
        "store-knowledge" => handle_store(task_id, &request.message, graph),
        "query-knowledge" => handle_query(task_id, &text, graph),
        "reason" => handle_reason(task_id, &text, graph),
        "learn" => handle_learn(task_id, &request.message, graph),
        "explain" => handle_explain(task_id, &text, graph),
        "analyze-gaps" => handle_analyze_gaps(task_id, &text, graph),
        "federated-search" => handle_federated_search(task_id, &text, graph),
        "suggest-investigations" => handle_suggest_investigations(task_id, &text, graph),
        "assess-knowledge" => handle_assess(task_id, &request.message, graph),
        other => TaskResponse::failed(task_id, &format!("Unknown skill: {other}")),
    }
}

/// Store knowledge — handles both text and structured input.
fn handle_store(task_id: &str, message: &TaskMessage, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let mut g = match graph.write() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph write lock poisoned"),
    };
    let prov = Provenance::user("a2a");
    let text = message.text();

    // Check for structured data input
    if let Some(data) = message.data() {
        return handle_store_structured(task_id, data, &mut g, &prov);
    }

    // Natural language store via the tell handler
    let result = engram_api::natural::handle_tell(&mut g, &text, Some("a2a"));
    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "store",
            "result": serde_json::to_value(&result).unwrap_or_default(),
        })),
    ])
}

fn handle_store_structured(
    task_id: &str,
    data: &serde_json::Value,
    g: &mut Graph,
    prov: &Provenance,
) -> TaskResponse {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("");
    if entity.is_empty() {
        return TaskResponse::failed(task_id, "missing 'entity' field in data");
    }

    let confidence = data.get("confidence").and_then(|v| v.as_f64()).map(|c| c as f32);

    let slot = if let Some(conf) = confidence {
        g.store_with_confidence(entity, conf, prov)
    } else {
        g.store(entity, prov)
    };

    match slot {
        Ok(slot) => TaskResponse::completed(task_id, vec![
            Artifact::json(serde_json::json!({
                "action": "store",
                "entity": entity,
                "slot": slot,
                "confidence": confidence.unwrap_or(0.5),
            })),
        ]),
        Err(e) => TaskResponse::failed(task_id, &e.to_string()),
    }
}

/// Query knowledge — natural language or structured.
fn handle_query(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    // Use the NL ask handler
    let result = engram_api::natural::handle_ask(&g, text);

    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "query",
            "query": text,
            "result": result,
        })),
    ])
}

/// Reason & prove — inference and proof.
fn handle_reason(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    // Try to extract "from" and "to" for prove, otherwise use search
    let lower = text.to_lowercase();
    if lower.contains(" related to ") || lower.contains(" connected to ") || lower.contains(" linked to ") {
        // Try to extract entities for proof
        let parts: Vec<&str> = if lower.contains(" related to ") {
            text.splitn(2, " related to ").collect()
        } else if lower.contains(" connected to ") {
            text.splitn(2, " connected to ").collect()
        } else {
            text.splitn(2, " linked to ").collect()
        };

        if parts.len() == 2 {
            let from = parts[0].trim_start_matches(|c: char| !c.is_alphanumeric()).trim();
            let to = parts[1].trim_end_matches('?').trim();
            let proof = g.prove(from, to, "is_a", 5);
            return TaskResponse::completed(task_id, vec![
                Artifact::json(serde_json::json!({
                    "action": "prove",
                    "from": from,
                    "to": to,
                    "proof": format!("{:?}", proof),
                })),
            ]);
        }
    }

    // Fallback: search
    let result = engram_api::natural::handle_ask(&g, text);
    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "reason",
            "query": text,
            "result": result,
        })),
    ])
}

/// Learn & correct — reinforce, correct, decay.
fn handle_learn(task_id: &str, message: &TaskMessage, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let mut g = match graph.write() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph write lock poisoned"),
    };

    let text = message.text();
    let lower = text.to_lowercase();
    let prov = Provenance::user("a2a");

    if lower.starts_with("confirm ") || lower.starts_with("reinforce ") {
        let entity = text.splitn(2, ' ').nth(1).unwrap_or("").trim();
        match g.reinforce_confirm(entity, &prov) {
            Ok(updated) => {
                let new_conf = g.get_node(entity).ok().flatten().map(|n| n.confidence).unwrap_or(0.0);
                return TaskResponse::completed(task_id, vec![
                    Artifact::json(serde_json::json!({
                        "action": "reinforce",
                        "entity": entity,
                        "updated": updated,
                        "confidence": new_conf,
                    })),
                ]);
            }
            Err(e) => return TaskResponse::failed(task_id, &e.to_string()),
        }
    }

    if lower.starts_with("correct ") || lower.contains(" was wrong") {
        let entity = if lower.starts_with("correct ") {
            text.splitn(2, ' ').nth(1).unwrap_or("").trim()
        } else {
            text.split(" was wrong").next().unwrap_or("").trim()
        };
        match g.correct(entity, &prov, 3) {
            Ok(Some(correction)) => {
                return TaskResponse::completed(task_id, vec![
                    Artifact::json(serde_json::json!({
                        "action": "correct",
                        "entity": entity,
                        "corrected_slot": correction.corrected_slot,
                        "propagated_count": correction.propagated.len(),
                    })),
                ]);
            }
            Ok(None) => {
                return TaskResponse::completed(task_id, vec![
                    Artifact::json(serde_json::json!({
                        "action": "correct",
                        "entity": entity,
                        "result": "entity not found",
                    })),
                ]);
            }
            Err(e) => return TaskResponse::failed(task_id, &e.to_string()),
        }
    }

    if lower.starts_with("forget ") || lower.starts_with("decay ") {
        match g.apply_decay() {
            Ok(count) => {
                return TaskResponse::completed(task_id, vec![
                    Artifact::json(serde_json::json!({
                        "action": "decay",
                        "nodes_decayed": count,
                    })),
                ]);
            }
            Err(e) => return TaskResponse::failed(task_id, &e.to_string()),
        }
    }

    // Fallback: store via tell
    let result = engram_api::natural::handle_tell(&mut g, &text, Some("a2a"));
    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "learn",
            "result": serde_json::to_value(&result).unwrap_or_default(),
        })),
    ])
}

/// Explain provenance.
fn handle_explain(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    // Extract entity name from questions like "How do we know about X?"
    let lower = text.to_lowercase();
    let entity = if lower.contains("about ") {
        text.split("about ").last().unwrap_or(text).trim_end_matches('?').trim()
    } else if lower.contains("for ") {
        text.split("for ").last().unwrap_or(text).trim_end_matches('?').trim()
    } else {
        text.trim_end_matches('?').trim()
    };

    let node = g.get_node(entity);
    match node {
        Ok(Some(n)) => {
            let props = g.get_properties(entity).unwrap_or(None).unwrap_or_default();
            let edges_from = g.edges_from(entity).unwrap_or_default();
            let edges_to = g.edges_to(entity).unwrap_or_default();
            let cooccurrences = g.cooccurrences_for(entity);
            TaskResponse::completed(task_id, vec![
                Artifact::json(serde_json::json!({
                    "action": "explain",
                    "entity": entity,
                    "confidence": n.confidence,
                    "properties": props,
                    "edges_from": edges_from.iter().map(|e| serde_json::json!({
                        "to": e.to, "relationship": e.relationship, "confidence": e.confidence
                    })).collect::<Vec<_>>(),
                    "edges_to": edges_to.iter().map(|e| serde_json::json!({
                        "from": e.from, "relationship": e.relationship, "confidence": e.confidence
                    })).collect::<Vec<_>>(),
                    "cooccurrences": cooccurrences.iter().map(|(e, c)| serde_json::json!({
                        "entity": e, "count": c
                    })).collect::<Vec<_>>(),
                })),
            ])
        }
        Ok(None) => TaskResponse::failed(task_id, &format!("entity not found: {entity}")),
        Err(e) => TaskResponse::failed(task_id, &e.to_string()),
    }
}

/// Federated search — search across local graph with mesh-aware ACL filtering.
fn handle_federated_search(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    let query = engram_reason::FederatedQuery {
        query: text.to_string(),
        query_type: "fulltext".to_string(),
        max_results: 20,
        min_confidence: 0.0,
        requesting_node: "a2a".to_string(),
        sensitivity_clearance: "public".to_string(),
    };

    let result = engram_reason::federated::execute_local(&g, &query);

    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "federated_search",
            "query": text,
            "peer": result.peer_id,
            "facts": result.facts,
            "total": result.total_matches,
        })),
    ])
}

/// Suggest investigations — uses gap detection + LLM suggestions.
fn handle_suggest_investigations(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    let config = engram_reason::DetectionConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    match engram_reason::scan(&g, &config, now) {
        Ok((gaps, _report)) => {
            let min_severity = if text.contains("critical") { 0.7 } else { 0.3 };
            let filtered: Vec<_> = gaps.into_iter()
                .filter(|gap| gap.severity >= min_severity)
                .take(10)
                .collect();

            // Generate mechanical query suggestions
            let suggestions: Vec<serde_json::Value> = filtered.into_iter().map(|mut gap| {
                engram_reason::queries::generate_queries(&mut gap, &g);
                serde_json::json!({
                    "gap": gap.entities.first().unwrap_or(&String::new()),
                    "kind": format!("{:?}", gap.kind),
                    "severity": gap.severity,
                    "suggested_queries": gap.suggested_queries,
                })
            }).collect();

            // Also include LLM suggestion config info
            let llm_config = engram_reason::LlmSuggestionConfig::from_env();
            let llm_available = llm_config.api_key.is_some();

            TaskResponse::completed(task_id, vec![
                Artifact::json(serde_json::json!({
                    "action": "suggest_investigations",
                    "investigations": suggestions,
                    "total_gaps": suggestions.len(),
                    "llm_suggestions_available": llm_available,
                })),
            ])
        }
        Err(e) => TaskResponse::failed(task_id, &e.to_string()),
    }
}

/// Assessment & hypothesis tracking — create, evaluate, and query assessments.
fn handle_assess(task_id: &str, message: &TaskMessage, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let text = message.text();
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    let lower = text.to_lowercase();

    // Try to find assessment nodes via search
    let query = if lower.contains("assess") || lower.contains("predict") || lower.contains("hypothesis") {
        "Assessment:"
    } else {
        &text
    };

    let results = g.search(query, 20).unwrap_or_default();
    let assessments: Vec<_> = results.iter()
        .filter(|r| r.label.starts_with("Assessment:"))
        .collect();

    if lower.starts_with("create") || lower.starts_with("new") {
        return TaskResponse::completed(task_id, vec![
            Artifact::json(serde_json::json!({
                "action": "assess_create",
                "hint": "Use POST /assessments to create. Provide title, watches, and initial_probability.",
                "query": text,
            })),
        ]);
    }

    TaskResponse::completed(task_id, vec![
        Artifact::json(serde_json::json!({
            "action": "assess_list",
            "query": text,
            "assessments": assessments.iter().map(|r| serde_json::json!({
                "label": r.label,
                "confidence": r.confidence,
                "score": r.score,
            })).collect::<Vec<_>>(),
            "total": assessments.len(),
        })),
    ])
}

/// Analyze knowledge gaps — uses engram-reason to detect black areas.
fn handle_analyze_gaps(task_id: &str, text: &str, graph: &Arc<RwLock<Graph>>) -> TaskResponse {
    let g = match graph.read() {
        Ok(g) => g,
        Err(_) => return TaskResponse::failed(task_id, "graph read lock poisoned"),
    };

    let config = engram_reason::DetectionConfig::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;

    match engram_reason::scan(&g, &config, now) {
        Ok((gaps, report)) => {
            let min_severity = if text.contains("critical") { 0.7 } else { 0.3 };
            let filtered: Vec<_> = gaps.into_iter()
                .filter(|gap| gap.severity >= min_severity)
                .take(20)
                .collect();

            TaskResponse::completed(task_id, vec![
                Artifact::json(serde_json::json!({
                    "action": "analyze_gaps",
                    "gaps": filtered,
                    "report": report,
                })),
            ])
        }
        Err(e) => TaskResponse::failed(task_id, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> Arc<RwLock<Graph>> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain");
        let g = Graph::create(&path).unwrap();
        // Leak tempdir to keep it alive
        std::mem::forget(dir);
        Arc::new(RwLock::new(g))
    }

    #[test]
    fn store_text_skill() {
        let graph = make_graph();
        let req = TaskRequest {
            id: Some("t1".to_string()),
            skill_id: "store-knowledge".to_string(),
            message: TaskMessage::user_text("Rust is a systems language"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
    }

    #[test]
    fn store_structured_skill() {
        let graph = make_graph();
        let req = TaskRequest {
            id: Some("t2".to_string()),
            skill_id: "store-knowledge".to_string(),
            message: TaskMessage {
                role: "user".to_string(),
                parts: vec![
                    MessagePart::Data {
                        data: serde_json::json!({
                            "entity": "PostgreSQL",
                            "confidence": 0.9
                        }),
                    },
                ],
            },
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
        let art = &resp.artifacts.unwrap()[0];
        assert_eq!(art.data["entity"], "PostgreSQL");
    }

    #[test]
    fn query_skill() {
        let graph = make_graph();
        // Store something first
        graph.write().unwrap().store("Rust", &Provenance::user("test")).unwrap();

        let req = TaskRequest {
            id: Some("t3".to_string()),
            skill_id: "query-knowledge".to_string(),
            message: TaskMessage::user_text("What is Rust?"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
    }

    #[test]
    fn unknown_skill() {
        let graph = make_graph();
        let req = TaskRequest {
            id: Some("t4".to_string()),
            skill_id: "unknown-skill".to_string(),
            message: TaskMessage::user_text("test"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Failed);
    }

    #[test]
    fn explain_skill() {
        let graph = make_graph();
        graph.write().unwrap().store("Rust", &Provenance::user("test")).unwrap();

        let req = TaskRequest {
            id: Some("t5".to_string()),
            skill_id: "explain".to_string(),
            message: TaskMessage::user_text("How do we know about Rust?"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
    }

    #[test]
    fn learn_reinforce() {
        let graph = make_graph();
        graph.write().unwrap().store("Rust", &Provenance::user("test")).unwrap();

        let req = TaskRequest {
            id: Some("t6".to_string()),
            skill_id: "learn".to_string(),
            message: TaskMessage::user_text("Confirm Rust"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
        assert!(resp.artifacts.unwrap()[0].data["action"] == "reinforce");
    }

    #[test]
    fn reason_skill() {
        let graph = make_graph();
        graph.write().unwrap().store("A", &Provenance::user("test")).unwrap();
        graph.write().unwrap().store("B", &Provenance::user("test")).unwrap();

        let req = TaskRequest {
            id: Some("t7".to_string()),
            skill_id: "reason".to_string(),
            message: TaskMessage::user_text("Is A related to B?"),
            metadata: None,
            push_notification_url: None,
        };
        let resp = route_task(&req, &graph);
        assert_eq!(resp.status.state, crate::task::TaskState::Completed);
    }
}
