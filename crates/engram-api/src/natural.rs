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

    // Fallback: treat the whole thing as a search query
    ask_search(graph, q)
}

/// Parse a natural language statement and execute it against the graph.
pub fn handle_tell(graph: &mut Graph, statement: &str, source: Option<&str>) -> TellResponse {
    let s = statement.trim().trim_end_matches('.').trim();
    let lower = s.to_lowercase();
    let prov = Provenance::user(source.unwrap_or("natural"));
    let mut actions = Vec::new();

    // "X is a Y" / "X is Y" — store + type or relationship
    if let Some((subject, predicate)) = split_is_a(&lower) {
        // Store subject if not exists
        let _ = graph.store(&capitalize_first(subject), &prov);
        actions.push(format!("stored entity: {subject}"));

        // Store object if not exists
        let _ = graph.store(&capitalize_first(predicate), &prov);
        actions.push(format!("stored entity: {predicate}"));

        // Create is_a relationship
        let _ = graph.relate(
            &capitalize_first(subject),
            &capitalize_first(predicate),
            "is_a",
            &prov,
        );
        actions.push(format!("{subject} -[is_a]-> {predicate}"));

        return TellResponse {
            interpretation: format!("{subject} is a type of {predicate}"),
            actions,
        };
    }

    // "X causes Y" / "X connects to Y" / "X depends on Y" — relationship patterns
    let relationship_patterns = [
        ("causes", "causes"),
        ("connects to", "connects_to"),
        ("depends on", "depends_on"),
        ("runs on", "runs_on"),
        ("uses", "uses"),
        ("has", "has"),
        ("contains", "contains"),
        ("belongs to", "belongs_to"),
        ("affects", "affects"),
        ("relates to", "relates_to"),
        ("hosts", "hosts"),
        ("requires", "requires"),
        ("produces", "produces"),
        ("consumes", "consumes"),
    ];

    for (pattern, rel_type) in &relationship_patterns {
        if let Some((subject, object)) = lower.split_once(pattern) {
            let subject = subject.trim();
            let object = object.trim();
            if !subject.is_empty() && !object.is_empty() {
                let subj = capitalize_first(subject);
                let obj = capitalize_first(object);

                let _ = graph.store(&subj, &prov);
                let _ = graph.store(&obj, &prov);
                let _ = graph.relate(&subj, &obj, rel_type, &prov);
                actions.push(format!("stored: {subj}"));
                actions.push(format!("stored: {obj}"));
                actions.push(format!("{subj} -[{rel_type}]-> {obj}"));

                return TellResponse {
                    interpretation: format!("{subject} {pattern} {object}"),
                    actions,
                };
            }
        }
    }

    // "X has property Y = Z" / "X.Y = Z" — property setting
    if let Some((entity, rest)) = lower.split_once(" has ") {
        if let Some((key, value)) = rest.split_once(" = ") {
            let entity = capitalize_first(entity.trim());
            let _ = graph.store(&entity, &prov);
            let _ = graph.set_property(&entity, key.trim(), value.trim());
            actions.push(format!("set {entity}.{} = {}", key.trim(), value.trim()));

            return TellResponse {
                interpretation: format!("set property on {entity}"),
                actions,
            };
        }
    }

    // Fallback: store as a node with the full statement as label
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
    // Try to prove a connection in either direction
    let a_cap = capitalize_first(a);
    let b_cap = capitalize_first(b);

    // Check outgoing from a
    let mut results = Vec::new();
    if let Ok(edges) = graph.edges_from(&a_cap) {
        for e in &edges {
            if e.to.to_lowercase() == b.to_lowercase() {
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
    if let Ok(edges) = graph.edges_from(&b_cap) {
        for e in &edges {
            if e.to.to_lowercase() == a.to_lowercase() {
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
        if let Ok(traversal) = graph.traverse(&a_cap, 3, 0.0) {
            if let Ok(Some(b_id)) = graph.find_node_id(&b_cap) {
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

fn split_is_a(s: &str) -> Option<(&str, &str)> {
    // "X is a Y" or "X is an Y"
    if let Some((subj, rest)) = s.split_once(" is a ") {
        return Some((subj.trim(), rest.trim()));
    }
    if let Some((subj, rest)) = s.split_once(" is an ") {
        return Some((subj.trim(), rest.trim()));
    }
    None
}

fn capitalize_first(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap().to_uppercase().to_string();
    first + chars.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
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
