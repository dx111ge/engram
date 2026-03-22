//! Context retrieval: extract entities from user message and fetch graph context.

use crate::api::ApiClient;
use crate::components::chat_types::ContextItem;

/// Extract simple keywords/entities from a user message for context lookup.
/// Uses basic heuristics: capitalized words, quoted strings, multi-word proper nouns.
pub fn extract_keywords(text: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    // Extract quoted strings first
    let mut remaining = text;
    while let Some(start) = remaining.find('"') {
        remaining = &remaining[start + 1..];
        if let Some(end) = remaining.find('"') {
            let quoted = &remaining[..end];
            if !quoted.is_empty() && quoted.len() < 80 {
                keywords.push(quoted.to_string());
            }
            remaining = &remaining[end + 1..];
        } else {
            break;
        }
    }

    // Extract capitalized words and multi-word proper nouns
    let stop_words: &[&str] = &[
        "I", "The", "This", "That", "What", "When", "Where", "Who", "How", "Why",
        "Is", "Are", "Was", "Were", "Do", "Does", "Did", "Can", "Could", "Would",
        "Should", "Will", "Have", "Has", "Had", "Not", "But", "And", "Or", "If",
        "About", "From", "With", "Tell", "Find", "Show", "Get", "Give", "Make",
        "Know", "Think", "Between", "Compare", "What's", "Let",
    ];

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let word = words[i].trim_matches(|c: char| !c.is_alphanumeric());
        if word.is_empty() {
            i += 1;
            continue;
        }

        let first_char = word.chars().next().unwrap_or('a');
        if first_char.is_uppercase() && !stop_words.contains(&word) {
            // Try to collect multi-word proper noun
            let mut proper_noun = word.to_string();
            let mut j = i + 1;
            while j < words.len() {
                let next = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                let next_first = next.chars().next().unwrap_or('a');
                if next_first.is_uppercase() && !stop_words.contains(&next) {
                    proper_noun.push(' ');
                    proper_noun.push_str(next);
                    j += 1;
                } else {
                    break;
                }
            }
            if !keywords.contains(&proper_noun) {
                keywords.push(proper_noun);
            }
            i = j;
        } else {
            i += 1;
        }
    }

    keywords
}

/// Retrieve context from the graph for the given keywords.
/// Returns a list of matching entities with their types and confidence.
pub async fn retrieve_context(
    api: &ApiClient,
    keywords: &[String],
) -> Vec<ContextItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for keyword in keywords.iter().take(5) {
        // Try search for each keyword
        let body = serde_json::json!({
            "query": keyword,
            "limit": 3,
        });

        if let Ok(text) = api.post_text("/search", &body).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(results) = parsed.get("results").and_then(|v| v.as_array()) {
                    for hit in results {
                        let label = hit.get("label").and_then(|v| v.as_str()).unwrap_or_default();
                        if label.is_empty() || !seen.insert(label.to_string()) {
                            continue;
                        }
                        let node_type = hit.get("node_type").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let confidence = hit.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        items.push(ContextItem {
                            label: label.to_string(),
                            node_type,
                            confidence,
                            edge_count: None,
                        });
                    }
                }
            }
        }
    }

    items
}

/// Build the context block for the system prompt.
pub fn format_context_block(
    items: &[ContextItem],
    page: &str,
    selected_node: &Option<String>,
    current_assessment: &Option<String>,
    node_count: u64,
    edge_count: u64,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "Graph contains {} entities, {} relations.",
        node_count, edge_count
    ));
    parts.push(format!("User is on the {} page.", page_display_name(page)));

    if let Some(node) = selected_node {
        parts.push(format!("Selected entity in graph view: {}", node));
    }

    if let Some(assessment) = current_assessment {
        parts.push(format!("Currently viewing assessment: {}", assessment));
    }

    if !items.is_empty() {
        parts.push("Retrieved context:".to_string());
        for item in items {
            let typ = item.node_type.as_deref().unwrap_or("entity");
            parts.push(format!(
                "  - {} ({}, confidence: {:.0}%)",
                item.label, typ, item.confidence * 100.0
            ));
        }
    }

    parts.join("\n")
}

/// Display-friendly page name.
fn page_display_name(path: &str) -> &str {
    match path {
        "/" => "Home",
        "/graph" => "Explore",
        "/insights" => "Insights",
        "/search" => "Search",
        "/ingest" => "Ingest",
        "/sources" => "Sources",
        "/gaps" => "Knowledge Gaps",
        "/actions" => "Actions",
        "/mesh" => "Mesh",
        _ if path.starts_with("/node/") => "Entity Detail",
        _ => path,
    }
}

/// Build the full system prompt with context injection.
pub fn build_system_prompt(context_block: &str, persona: &str) -> String {
    format!(
        "{persona}\n\n\
         CONTEXT (auto-injected):\n\
         {context_block}\n\n\
         BEHAVIOR:\n\
         - Always cite confidence levels when reporting facts\n\
         - Flag low-confidence (<40%) data explicitly\n\
         - After answering, suggest 2-3 follow-up questions or actions\n\
         - When temporal data exists, distinguish current vs historical\n\
         - Never invent facts -- if you don't know, say so and suggest an investigation\n\
         - When calling tools with entity names, use the EXACT labels from previous tool results. Never paraphrase, reformat, or change casing of entity names (e.g., use 'Russia Ukraine war' not 'Russia-Ukraine War')"
    )
}

/// Default persona for the intelligence analyst.
pub const DEFAULT_PERSONA: &str =
    "You are an intelligence analyst assistant for the engram knowledge graph. \
     Use the available tools to store, query, search, and reason about knowledge. \
     Be concise and precise. When you find information, summarize it clearly. \
     Always distinguish current from historical information using temporal bounds. \
     When referencing entities in tool calls, always copy names exactly as they appear in results.";
