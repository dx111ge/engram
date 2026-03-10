/// Natural language query interface — parse simple English into graph operations.
///
/// POST /ask  — "What does postgresql connect to?" → traverse query
/// POST /tell — "postgresql is a database" → store + relate
///
/// This is a rule-based parser, not an LLM. It handles common patterns:
///   - "What is X?" → node lookup
///   - "What does X relate to?" → outgoing edges
///   - "How are X and Y related?" → path between nodes
///   - "Find things like X" → similarity search
///   - "X is a Y" → store relationship
///   - "X causes Y" → create edge

use engram_core::graph::{Graph, Provenance};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
}

#[derive(Deserialize)]
pub struct TellRequest {
    pub statement: String,
    pub source: Option<String>,
}

#[derive(Serialize)]
pub struct AskResponse {
    pub interpretation: String,
    pub results: Vec<AskResult>,
}

#[derive(Serialize)]
pub struct TellResponse {
    pub interpretation: String,
    pub actions: Vec<String>,
}

#[derive(Serialize)]
pub struct AskResult {
    pub label: String,
    pub confidence: f32,
    pub relationship: Option<String>,
    pub detail: Option<String>,
}

/// Parse a natural language question and execute it against the graph.
pub fn handle_ask(graph: &Graph, question: &str) -> AskResponse {
    let q = question.trim().trim_end_matches('?').trim();
    let lower = q.to_lowercase();

    // "What is X?" / "Who is X?" — node lookup
    if lower.starts_with("what is ") || lower.starts_with("who is ") {
        let entity = &q[8..].trim();
        return ask_node_lookup(graph, entity);
    }

    // "What does X connect/relate/link to?" — outgoing edges
    if lower.starts_with("what does ") {
        // Extract from original case `q`, using known prefix/suffix lengths
        if let Some(entity) = extract_between_preserve_case(q, "what does ", " connect to")
            .or_else(|| extract_between_preserve_case(q, "what does ", " relate to"))
            .or_else(|| extract_between_preserve_case(q, "what does ", " link to"))
        {
            return ask_edges_from(graph, &entity);
        }
    }

    // "What connects to X?" / "What relates to X?" — incoming edges
    if lower.starts_with("what connects to ")
        || lower.starts_with("what relates to ")
        || lower.starts_with("what links to ")
    {
        let entity = q.split_once(" to ").map(|(_, e)| e.trim()).unwrap_or(q);
        return ask_edges_to(graph, entity);
    }

    // "How are X and Y related?" — path between
    if lower.starts_with("how are ") && lower.contains(" and ") {
        if let Some(pair) = extract_between_preserve_case(q, "how are ", " related") {
            if let Some((a, b)) = pair.split_once(" and ") {
                return ask_path(graph, a.trim(), b.trim());
            }
        }
    }

    // "Find things like X" / "Similar to X" — similarity search
    if lower.starts_with("find things like ")
        || lower.starts_with("similar to ")
        || lower.starts_with("find similar ")
    {
        let text = if lower.starts_with("find things like ") {
            &q[17..]
        } else if lower.starts_with("similar to ") {
            &q[11..]
        } else {
            &q[13..]
        };
        return ask_similar(graph, text.trim());
    }

    // "Search for X" / "Find X"
    if lower.starts_with("search for ") || lower.starts_with("find ") {
        let text = if lower.starts_with("search for ") {
            &q[11..]
        } else {
            &q[5..]
        };
        return ask_search(graph, text.trim());
    }

    // "List all X" / "Show all X" — search for type
    if lower.starts_with("list all ") || lower.starts_with("show all ") {
        let type_name = if lower.starts_with("list all ") {
            &q[9..]
        } else {
            &q[9..]
        };
        return ask_search_type(graph, type_name.trim());
    }

    // "What type is X?" / "What kind of X?" — node lookup focusing on is_a edges
    if lower.starts_with("what type is ") {
        let entity = &q[13..];
        return ask_type_of(graph, entity.trim());
    }
    if lower.starts_with("what kind of ") {
        let entity = &q[13..];
        return ask_type_of(graph, entity.trim());
    }

    // "Explain X" — full provenance / node lookup
    if lower.starts_with("explain ") {
        let entity = &q[8..];
        return ask_explain(graph, entity.trim());
    }

    // "What depends on X?" — incoming edges filtered by depends_on
    if lower.starts_with("what depends on ") {
        let entity = &q[16..];
        return ask_incoming_by_rel(graph, entity.trim(), "depends_on");
    }

    // Fallback: try LLM if configured, otherwise search
    #[cfg(feature = "llm")]
    {
        if let Some(resp) = llm_fallback_ask(graph, q) {
            return resp;
        }
    }

    ask_search(graph, q)
}

/// Parse a natural language statement and execute it against the graph.
///
/// Entity names preserve their original casing from the input. Graph lookups
/// are case-insensitive, so "PostgreSQL", "postgresql", and "Postgresql"
/// all resolve to the same node.
pub fn handle_tell(graph: &mut Graph, statement: &str, source: Option<&str>) -> TellResponse {
    let s = statement.trim().trim_end_matches('.').trim();
    let lower = s.to_lowercase();
    let prov = Provenance::user(source.unwrap_or("natural"));
    let mut actions = Vec::new();

    // "X is a Y" / "X is Y" — store + type or relationship
    // Match on lowered string, but extract entities from original to preserve casing
    if let Some((subj_orig, pred_orig)) = split_is_a_preserve_case(s, &lower) {
        let _ = graph.store(&subj_orig, &prov);
        actions.push(format!("stored entity: {subj_orig}"));

        let _ = graph.store(&pred_orig, &prov);
        actions.push(format!("stored entity: {pred_orig}"));

        let _ = graph.relate(&subj_orig, &pred_orig, "is_a", &prov);
        actions.push(format!("{subj_orig} -[is_a]-> {pred_orig}"));

        return TellResponse {
            interpretation: format!("{subj_orig} is a type of {pred_orig}"),
            actions,
        };
    }

    // "X causes Y" / "X connects to Y" / "X depends on Y" — relationship patterns
    let relationship_patterns = [
        // Multi-word patterns first (longer patterns before shorter to avoid partial matches)
        ("inherits from", "inherits_from"),
        ("communicates with", "communicates_with"),
        ("deployed on", "deployed_on"),
        ("deployed to", "deployed_on"),
        ("configured by", "configured_by"),
        ("reports to", "reports_to"),
        ("located in", "located_in"),
        ("connects to", "connects_to"),
        ("depends on", "depends_on"),
        ("runs on", "runs_on"),
        ("belongs to", "belongs_to"),
        ("relates to", "relates_to"),
        ("part of", "part_of"),
        ("version of", "version_of"),
        // Single-word patterns
        ("causes", "causes"),
        ("created", "creates"),
        ("creates", "creates"),
        ("manages", "manages"),
        ("owns", "owns"),
        ("implemented", "implements"),
        ("implements", "implements"),
        ("extends", "extends"),
        ("replaced", "replaces"),
        ("replaces", "replaces"),
        ("supports", "supports"),
        ("blocked", "blocks"),
        ("blocks", "blocks"),
        ("enables", "enables"),
        ("triggered", "triggers"),
        ("triggers", "triggers"),
        ("monitors", "monitors"),
        ("stores", "stores_data_in"),
        ("uses", "uses"),
        ("has", "has"),
        ("contains", "contains"),
        ("affects", "affects"),
        ("hosts", "hosts"),
        ("requires", "requires"),
        ("produces", "produces"),
        ("consumes", "consumes"),
    ];

    for (pattern, rel_type) in &relationship_patterns {
        if let Some(pos) = lower.find(pattern) {
            let subj = s[..pos].trim();
            let obj = s[pos + pattern.len()..].trim();
            if !subj.is_empty() && !obj.is_empty() {
                let _ = graph.store(subj, &prov);
                let _ = graph.store(obj, &prov);
                let _ = graph.relate(subj, obj, rel_type, &prov);
                actions.push(format!("stored: {subj}"));
                actions.push(format!("stored: {obj}"));
                actions.push(format!("{subj} -[{rel_type}]-> {obj}"));

                return TellResponse {
                    interpretation: format!("{subj} {pattern} {obj}"),
                    actions,
                };
            }
        }
    }

    // "X has property Y = Z" — property setting
    if let Some(has_pos) = lower.find(" has ") {
        let entity = s[..has_pos].trim();
        let rest = &lower[has_pos + 5..];
        if let Some((key, value)) = rest.split_once(" = ") {
            let _ = graph.store(entity, &prov);
            let _ = graph.set_property(entity, key.trim(), value.trim());
            actions.push(format!("set {entity}.{} = {}", key.trim(), value.trim()));

            return TellResponse {
                interpretation: format!("set property on {entity}"),
                actions,
            };
        }
    }

    // Fallback: try LLM if configured, otherwise store as fact
    #[cfg(feature = "llm")]
    {
        if let Some(resp) = llm_fallback_tell(graph, s, &prov) {
            return resp;
        }
    }

    let _ = graph.store(s, &prov);
    actions.push(format!("stored fact: {s}"));

    TellResponse {
        interpretation: "stored as fact".to_string(),
        actions,
    }
}

// ── Query helpers ──

fn ask_node_lookup(graph: &Graph, entity: &str) -> AskResponse {
    match graph.get_node(entity) {
        Ok(Some(node)) => {
            let confidence = node.confidence;
            let mut results = vec![AskResult {
                label: entity.to_string(),
                confidence,
                relationship: None,
                detail: None,
            }];

            // Include properties as details
            if let Ok(Some(props)) = graph.get_properties(entity) {
                for (k, v) in &props {
                    results.push(AskResult {
                        label: entity.to_string(),
                        confidence,
                        relationship: None,
                        detail: Some(format!("{k}: {v}")),
                    });
                }
            }

            // Include outgoing edges
            if let Ok(edges) = graph.edges_from(entity) {
                for e in edges {
                    results.push(AskResult {
                        label: e.to,
                        confidence: e.confidence,
                        relationship: Some(e.relationship),
                        detail: None,
                    });
                }
            }

            AskResponse {
                interpretation: format!("lookup: {entity}"),
                results,
            }
        }
        _ => AskResponse {
            interpretation: format!("node not found: {entity}"),
            results: Vec::new(),
        },
    }
}

fn ask_edges_from(graph: &Graph, entity: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .edges_from(entity)
        .unwrap_or_default()
        .into_iter()
        .map(|e| AskResult {
            label: e.to,
            confidence: e.confidence,
            relationship: Some(e.relationship),
            detail: None,
        })
        .collect();

    AskResponse {
        interpretation: format!("outgoing edges from: {entity}"),
        results,
    }
}

fn ask_edges_to(graph: &Graph, entity: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .edges_to(entity)
        .unwrap_or_default()
        .into_iter()
        .map(|e| AskResult {
            label: e.from,
            confidence: e.confidence,
            relationship: Some(e.relationship),
            detail: None,
        })
        .collect();

    AskResponse {
        interpretation: format!("incoming edges to: {entity}"),
        results,
    }
}

fn ask_path(graph: &Graph, a: &str, b: &str) -> AskResponse {
    // Graph lookups are case-insensitive, pass names directly
    // Check outgoing from a
    let mut results = Vec::new();
    if let Ok(edges) = graph.edges_from(a) {
        for e in &edges {
            if e.to.eq_ignore_ascii_case(b) {
                results.push(AskResult {
                    label: format!("{} -[{}]-> {}", e.from, e.relationship, e.to),
                    confidence: e.confidence,
                    relationship: Some(e.relationship.clone()),
                    detail: None,
                });
            }
        }
    }

    // Check outgoing from b
    if let Ok(edges) = graph.edges_from(b) {
        for e in &edges {
            if e.to.eq_ignore_ascii_case(a) {
                results.push(AskResult {
                    label: format!("{} -[{}]-> {}", e.from, e.relationship, e.to),
                    confidence: e.confidence,
                    relationship: Some(e.relationship.clone()),
                    detail: None,
                });
            }
        }
    }

    if results.is_empty() {
        // Try traversal from a to see if b is reachable
        if let Ok(traversal) = graph.traverse(a, 3, 0.0) {
            if let Ok(Some(b_id)) = graph.find_node_id(b) {
                if traversal.nodes.contains(&b_id) {
                    let depth = traversal.depths.get(&b_id).copied().unwrap_or(0);
                    results.push(AskResult {
                        label: format!("{a} is {depth} hops from {b}"),
                        confidence: 0.5,
                        relationship: None,
                        detail: Some(format!("reachable at depth {depth}")),
                    });
                }
            }
        }
    }

    AskResponse {
        interpretation: format!("path between {a} and {b}"),
        results,
    }
}

fn ask_similar(graph: &Graph, text: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .search_text(text, 10)
        .unwrap_or_default()
        .into_iter()
        .map(|r| AskResult {
            label: r.label,
            confidence: r.confidence,
            relationship: None,
            detail: Some(format!("score: {:.3}", r.score)),
        })
        .collect();

    AskResponse {
        interpretation: format!("similarity search: {text}"),
        results,
    }
}

fn ask_search(graph: &Graph, query: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .search_text(query, 10)
        .unwrap_or_default()
        .into_iter()
        .map(|r| AskResult {
            label: r.label,
            confidence: r.confidence,
            relationship: None,
            detail: Some(format!("score: {:.3}", r.score)),
        })
        .collect();

    AskResponse {
        interpretation: format!("search: {query}"),
        results,
    }
}

/// Search for nodes that have an is_a edge to the given type.
fn ask_search_type(graph: &Graph, type_name: &str) -> AskResponse {
    // Find all nodes that point to type_name via is_a
    let results: Vec<AskResult> = graph
        .edges_to(type_name)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.relationship == "is_a")
        .map(|e| AskResult {
            label: e.from,
            confidence: e.confidence,
            relationship: Some("is_a".to_string()),
            detail: Some(format!("is a {type_name}")),
        })
        .collect();

    AskResponse {
        interpretation: format!("list all: {type_name}"),
        results,
    }
}

/// Look up what type an entity is (is_a edges).
fn ask_type_of(graph: &Graph, entity: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .edges_from(entity)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.relationship == "is_a")
        .map(|e| AskResult {
            label: e.to.clone(),
            confidence: e.confidence,
            relationship: Some("is_a".to_string()),
            detail: Some(format!("{entity} is a {}", e.to)),
        })
        .collect();

    AskResponse {
        interpretation: format!("type of: {entity}"),
        results,
    }
}

/// Explain an entity — full node lookup with properties and all edges (both directions).
fn ask_explain(graph: &Graph, entity: &str) -> AskResponse {
    let mut results = Vec::new();

    if let Ok(Some(node)) = graph.get_node(entity) {
        results.push(AskResult {
            label: entity.to_string(),
            confidence: node.confidence,
            relationship: None,
            detail: None,
        });

        // Include properties
        if let Ok(Some(props)) = graph.get_properties(entity) {
            for (k, v) in &props {
                results.push(AskResult {
                    label: entity.to_string(),
                    confidence: node.confidence,
                    relationship: None,
                    detail: Some(format!("{k}: {v}")),
                });
            }
        }

        // Include outgoing edges
        if let Ok(edges) = graph.edges_from(entity) {
            for e in edges {
                results.push(AskResult {
                    label: e.to,
                    confidence: e.confidence,
                    relationship: Some(e.relationship),
                    detail: Some("outgoing".to_string()),
                });
            }
        }

        // Include incoming edges
        if let Ok(edges) = graph.edges_to(entity) {
            for e in edges {
                results.push(AskResult {
                    label: e.from,
                    confidence: e.confidence,
                    relationship: Some(e.relationship),
                    detail: Some("incoming".to_string()),
                });
            }
        }
    }

    AskResponse {
        interpretation: format!("explain: {entity}"),
        results,
    }
}

/// Query incoming edges filtered by a specific relationship type.
fn ask_incoming_by_rel(graph: &Graph, entity: &str, rel: &str) -> AskResponse {
    let results: Vec<AskResult> = graph
        .edges_to(entity)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.relationship == rel)
        .map(|e| AskResult {
            label: e.from,
            confidence: e.confidence,
            relationship: Some(e.relationship),
            detail: None,
        })
        .collect();

    AskResponse {
        interpretation: format!("incoming {rel} to: {entity}"),
        results,
    }
}

// ── LLM fallback (behind "llm" feature) ──

#[cfg(feature = "llm")]
mod llm {
    use super::*;
    use std::env;
    use std::time::Duration;

    #[derive(Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<ChatMessage>,
        temperature: f32,
        max_tokens: u32,
    }

    #[derive(Serialize, Deserialize)]
    struct ChatMessage {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<ChatChoice>,
    }

    #[derive(Deserialize)]
    struct ChatChoice {
        message: ChatMessage,
    }

    fn llm_client() -> Option<(reqwest::blocking::Client, String, String)> {
        let endpoint = env::var("ENGRAM_LLM_ENDPOINT").ok()?;
        let model =
            env::var("ENGRAM_LLM_MODEL").unwrap_or_else(|_| "gpt-3.5-turbo".to_string());

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;

        Some((client, endpoint, model))
    }

    fn call_llm(system_prompt: &str, user_input: &str) -> Option<String> {
        let (client, endpoint, model) = llm_client()?;
        let api_key = env::var("ENGRAM_LLM_API_KEY").unwrap_or_default();

        let url = if endpoint.ends_with("/chat/completions") {
            endpoint.clone()
        } else {
            format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'))
        };

        let body = ChatRequest {
            model,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_input.to_string(),
                },
            ],
            temperature: 0.0,
            max_tokens: 256,
        };

        let mut req = client.post(&url).json(&body);
        if !api_key.is_empty() {
            req = req.bearer_auth(&api_key);
        }

        let resp = req.send().ok()?;
        let chat_resp: ChatResponse = resp.json().ok()?;
        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
    }

    const ASK_SYSTEM_PROMPT: &str = r#"You parse natural language questions about a knowledge graph into structured operations.
Available operations:
- NODE_LOOKUP <entity> — look up a node
- EDGES_FROM <entity> — outgoing edges
- EDGES_TO <entity> — incoming edges
- PATH <entity_a> <entity_b> — path between two nodes
- SEARCH <query> — text search

Respond with EXACTLY one line: OPERATION arg1 [arg2]
No explanation. No extra text."#;

    const TELL_SYSTEM_PROMPT: &str = r#"Extract entities and a relationship from a statement about a knowledge graph.
Respond with EXACTLY one line in the format: SUBJECT|RELATIONSHIP|OBJECT
- SUBJECT and OBJECT are entity names (preserve original case)
- RELATIONSHIP is a snake_case relationship type (e.g., depends_on, uses, is_a)
No explanation. No extra text."#;

    pub fn llm_fallback_ask(graph: &Graph, question: &str) -> Option<AskResponse> {
        let response = call_llm(ASK_SYSTEM_PROMPT, question)?;
        let line = response.trim();

        if line.starts_with("NODE_LOOKUP ") {
            let entity = line.strip_prefix("NODE_LOOKUP ")?.trim();
            return Some(ask_node_lookup(graph, entity));
        }
        if line.starts_with("EDGES_FROM ") {
            let entity = line.strip_prefix("EDGES_FROM ")?.trim();
            return Some(ask_edges_from(graph, entity));
        }
        if line.starts_with("EDGES_TO ") {
            let entity = line.strip_prefix("EDGES_TO ")?.trim();
            return Some(ask_edges_to(graph, entity));
        }
        if line.starts_with("PATH ") {
            let rest = line.strip_prefix("PATH ")?.trim();
            let (a, b) = rest.split_once(' ')?;
            return Some(ask_path(graph, a.trim(), b.trim()));
        }
        if line.starts_with("SEARCH ") {
            let query = line.strip_prefix("SEARCH ")?.trim();
            return Some(ask_search(graph, query));
        }

        None // Unrecognized LLM output, fall through to default
    }

    pub fn llm_fallback_tell(
        graph: &mut Graph,
        statement: &str,
        prov: &Provenance,
    ) -> Option<TellResponse> {
        let response = call_llm(TELL_SYSTEM_PROMPT, statement)?;
        let line = response.trim();

        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() != 3 {
            return None;
        }

        let subj = parts[0].trim();
        let rel = parts[1].trim();
        let obj = parts[2].trim();

        if subj.is_empty() || rel.is_empty() || obj.is_empty() {
            return None;
        }

        let mut actions = Vec::new();
        let _ = graph.store(subj, prov);
        let _ = graph.store(obj, prov);
        let _ = graph.relate(subj, obj, rel, prov);
        actions.push(format!("stored: {subj}"));
        actions.push(format!("stored: {obj}"));
        actions.push(format!("{subj} -[{rel}]-> {obj}"));

        Some(TellResponse {
            interpretation: format!("LLM parsed: {subj} {rel} {obj}"),
            actions,
        })
    }
}

#[cfg(feature = "llm")]
use llm::{llm_fallback_ask, llm_fallback_tell};

// ── String helpers ──

/// Extract text between prefix and suffix, preserving original case.
/// Uses prefix/suffix lengths to index into the original string.
fn extract_between_preserve_case(original: &str, prefix: &str, suffix: &str) -> Option<String> {
    let lower = original.to_lowercase();
    let after = lower.strip_prefix(prefix)?;
    let idx = after.find(suffix)?;
    // Extract from original using same byte positions
    let start = prefix.len();
    let end = start + idx;
    Some(original[start..end].trim().to_string())
}

/// Split "X is a Y" pattern, matching on `lower` but extracting from `original`.
/// Returns (subject, predicate) with original casing preserved.
fn split_is_a_preserve_case<'a>(original: &'a str, lower: &str) -> Option<(String, String)> {
    for pattern in [" is a ", " is an "] {
        if let Some(pos) = lower.find(pattern) {
            let subj = original[..pos].trim();
            let pred = original[pos + pattern.len()..].trim();
            if !subj.is_empty() && !pred.is_empty() {
                return Some((subj.to_string(), pred.to_string()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_graph() -> (TempDir, Graph) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let g = Graph::create(&path).unwrap();
        (dir, g)
    }

    #[test]
    fn ask_what_is() {
        let (_dir, mut g) = test_graph();
        let prov = Provenance::user("test");
        g.store("postgresql", &prov).unwrap();
        g.set_property("postgresql", "version", "16").unwrap();

        let resp = handle_ask(&g, "What is postgresql?");
        assert_eq!(resp.interpretation, "lookup: postgresql");
        assert!(!resp.results.is_empty());
        assert_eq!(resp.results[0].label, "postgresql");
    }

    #[test]
    fn ask_edges() {
        let (_dir, mut g) = test_graph();
        let prov = Provenance::user("test");
        g.store("A", &prov).unwrap();
        g.store("B", &prov).unwrap();
        g.relate("A", "B", "causes", &prov).unwrap();

        let resp = handle_ask(&g, "What does A connect to?");
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].label, "B");
    }

    #[test]
    fn ask_search_fallback() {
        let (_dir, mut g) = test_graph();
        let prov = Provenance::user("test");
        g.store("kubernetes", &prov).unwrap();

        let resp = handle_ask(&g, "kubernetes");
        assert!(resp.interpretation.starts_with("search:"));
    }

    #[test]
    fn tell_is_a() {
        let (_dir, mut g) = test_graph();
        let resp = handle_tell(&mut g, "postgresql is a database", None);
        assert!(resp.interpretation.contains("type of"));
        assert!(resp.actions.len() >= 3);

        // Verify the relationship was created
        let edges = g.edges_from("Postgresql").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relationship, "is_a");
    }

    #[test]
    fn tell_relationship() {
        let (_dir, mut g) = test_graph();
        let resp = handle_tell(&mut g, "redis depends on network", None);
        assert!(resp.interpretation.contains("depends on"));

        let edges = g.edges_from("Redis").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relationship, "depends_on");
    }

    #[test]
    fn tell_fallback_stores_fact() {
        let (_dir, mut g) = test_graph();
        let resp = handle_tell(&mut g, "something happened today", None);
        assert_eq!(resp.interpretation, "stored as fact");
    }
}
