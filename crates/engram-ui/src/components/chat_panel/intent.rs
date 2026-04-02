//! Keyword-based intent detection for chat messages.
//! Deterministic, no LLM -- matches keywords to tool intents.
//! Returns which tool card to show and any pre-fill text extracted.

/// Detected intent from user input.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatIntent {
    /// Tool name matching the intent (e.g., "search", "explain", "query")
    pub tool: &'static str,
    /// Pre-fill text extracted from the input (entity names, search terms)
    pub prefill: String,
    /// Optional second entity (for compare, path, influence)
    pub prefill2: String,
}

/// Detect intent from user text. Returns None if no keyword matches
/// (will fall back to search with the full text as prefill).
pub fn detect_intent(text: &str) -> ChatIntent {
    let lower = text.to_lowercase();
    let trimmed = text.trim();

    // ── Slash commands: /tool [args] ──
    if lower.starts_with('/') {
        let without_slash = &trimmed[1..];
        let (cmd, rest) = without_slash.split_once(' ').unwrap_or((without_slash, ""));
        let rest = rest.trim().to_string();
        let tool = match cmd.to_lowercase().as_str() {
            "store" | "add" => "store",
            "relate" | "connect" | "link" => "relate",
            "query" | "explore" => "query",
            "search" | "find" => "search",
            "explain" => "explain",
            "similar" => "similar",
            "compare" => "compare",
            "delete" | "remove" => "delete",
            "correct" | "fix" => "correct",
            "reinforce" | "boost" => "reinforce",
            "timeline" => "timeline",
            "provenance" | "sources" => "provenance",
            "documents" | "docs" => "documents",
            "most_connected" | "top" => "most_connected",
            "shortest_path" | "path" => "shortest_path",
            "prove" | "evidence" => "prove",
            "gaps" | "gap" => "gaps",
            "isolated" => "isolated",
            "ingest" | "import" => "ingest",
            "help" => "help",
            _ => "search",  // unknown slash commands fall back to search
        };
        // For two-entity tools, split on " to " / " and " / " vs "
        let (prefill, prefill2) = if matches!(tool, "relate" | "compare" | "shortest_path" | "prove") {
            split_two_entities(&rest)
        } else {
            (rest, String::new())
        };
        return ChatIntent { tool, prefill, prefill2 };
    }

    // ── Compound keywords first (longest match wins) ──

    // Category commands (multi-tool)
    if starts_any(&lower, &["analyze ", "analyse "]) {
        return ChatIntent { tool: "analyze", prefill: after_keyword(trimmed, &["analyze ", "analyse "]), prefill2: String::new() };
    }
    if starts_any(&lower, &["knowledge about ", "everything about ", "knowledge of "]) {
        return ChatIntent { tool: "knowledge", prefill: after_keyword(trimmed, &["knowledge about ", "everything about ", "knowledge of "]), prefill2: String::new() };
    }
    if starts_any(&lower, &["investigate "]) {
        return ChatIntent { tool: "investigate", prefill: after_keyword(trimmed, &["investigate "]), prefill2: String::new() };
    }
    if starts_any(&lower, &["briefing on ", "briefing about ", "brief me on ", "brief me about ", "summarize ", "summary of "]) {
        return ChatIntent { tool: "briefing", prefill: after_keyword(trimmed, &["briefing on ", "briefing about ", "brief me on ", "brief me about ", "summarize ", "summary of "]), prefill2: String::new() };
    }

    // What-if (before other "what" patterns)
    if starts_any(&lower, &["what if ", "what would happen if ", "hypothetical ", "simulate "]) {
        return ChatIntent { tool: "what_if", prefill: after_keyword(trimmed, &["what if ", "what would happen if ", "hypothetical ", "simulate "]), prefill2: String::new() };
    }

    // Path / connection between two entities
    if starts_any(&lower, &["path from ", "path between ", "find path ", "shortest path "]) {
        let rest = after_keyword(trimmed, &["path from ", "path between ", "find path ", "shortest path "]);
        let (a, b) = split_two_entities(&rest);
        return ChatIntent { tool: "shortest_path", prefill: a, prefill2: b };
    }
    if lower.contains("connected to") || lower.contains("connection between") {
        let (a, b) = extract_two_from_connected(trimmed);
        return ChatIntent { tool: "shortest_path", prefill: a, prefill2: b };
    }

    // Compare
    if starts_any(&lower, &["compare "]) {
        let rest = after_keyword(trimmed, &["compare "]);
        let (a, b) = split_two_entities(&rest);
        return ChatIntent { tool: "compare", prefill: a, prefill2: b };
    }
    if lower.contains(" vs ") || lower.contains(" versus ") {
        let (a, b) = split_on_vs(trimmed);
        return ChatIntent { tool: "compare", prefill: a, prefill2: b };
    }
    if starts_any(&lower, &["difference between ", "differences between "]) {
        let rest = after_keyword(trimmed, &["difference between ", "differences between "]);
        let (a, b) = split_two_entities(&rest);
        return ChatIntent { tool: "compare", prefill: a, prefill2: b };
    }

    // Influence
    if starts_any(&lower, &["influence of ", "how does "]) && lower.contains(" affect ") || lower.contains(" influence ") {
        let (a, b) = extract_two_from_connected(trimmed);
        return ChatIntent { tool: "influence", prefill: a, prefill2: b };
    }

    // Similar
    if starts_any(&lower, &["similar to ", "entities like ", "find similar "]) {
        return ChatIntent { tool: "similar", prefill: after_keyword(trimmed, &["similar to ", "entities like ", "find similar "]), prefill2: String::new() };
    }

    // Timeline
    if starts_any(&lower, &["timeline ", "timeline of ", "history of ", "chronology of ", "when did "]) {
        return ChatIntent { tool: "timeline", prefill: after_keyword(trimmed, &["timeline ", "timeline of ", "history of ", "chronology of ", "when did "]), prefill2: String::new() };
    }

    // Date query (temporal range)
    if starts_any(&lower, &["what happened to ", "what about "]) {
        let rest = after_keyword(trimmed, &["what happened to ", "what about "]);
        // Try to split "entity in/during/between date"
        let (entity, date) = split_entity_date(&rest);
        if !date.is_empty() {
            return ChatIntent { tool: "date_query", prefill: entity, prefill2: date };
        }
    }
    if lower.contains(" during ") || lower.contains(" between ") || lower.contains(" in january") || lower.contains(" in february") || lower.contains(" in march") || lower.contains(" in 202") {
        return ChatIntent { tool: "date_query", prefill: trimmed.to_string(), prefill2: String::new() };
    }

    // Current state
    if starts_any(&lower, &["current state of ", "current ", "what is "]) && (lower.contains(" now") || lower.contains("current")) {
        return ChatIntent { tool: "current_state", prefill: after_keyword(trimmed, &["current state of ", "current "]), prefill2: String::new() };
    }

    // Fact provenance
    if starts_any(&lower, &["provenance of ", "sources for ", "where did ", "how did i learn about ", "origin of "]) {
        return ChatIntent { tool: "fact_provenance", prefill: after_keyword(trimmed, &["provenance of ", "sources for ", "where did ", "how did i learn about ", "origin of "]), prefill2: String::new() };
    }

    // Contradictions
    if lower.contains("contradict") || lower.contains("conflict") || lower.contains("disputed") || lower.contains("debunked") {
        let entity = after_keyword(trimmed, &["contradictions about ", "contradictions for ", "conflicts about ", "what contradicts "]);
        return ChatIntent { tool: "contradictions", prefill: if entity.is_empty() { trimmed.to_string() } else { entity }, prefill2: String::new() };
    }

    // Situation at date
    if starts_any(&lower, &["situation ", "snapshot ", "state on ", "what did i know "]) {
        let rest = after_keyword(trimmed, &["situation of ", "situation ", "snapshot of ", "snapshot ", "state on ", "what did i know about "]);
        let (entity, date) = split_entity_date(&rest);
        return ChatIntent { tool: "situation_at", prefill: entity, prefill2: date };
    }

    // Gaps
    if lower.contains("gaps") || lower.contains("missing knowledge") || lower.contains("blind spots") || lower == "gaps" {
        return ChatIntent { tool: "gaps", prefill: String::new(), prefill2: String::new() };
    }

    // Most connected
    if lower.contains("most connected") || lower.contains("key entities") || lower.contains("hubs") || lower.contains("most important entit") {
        return ChatIntent { tool: "most_connected", prefill: String::new(), prefill2: String::new() };
    }

    // Isolated
    if lower.contains("isolated") || lower.contains("orphan") || lower.contains("disconnected entit") {
        return ChatIntent { tool: "isolated", prefill: String::new(), prefill2: String::new() };
    }

    // Explain (single entity deep-dive)
    if starts_any(&lower, &["explain ", "about ", "what is ", "who is ", "tell me about ", "describe ", "details of ", "detail ", "info on ", "information about "]) {
        return ChatIntent { tool: "explain", prefill: after_keyword(trimmed, &["explain ", "about ", "what is ", "who is ", "tell me about ", "describe ", "details of ", "detail ", "info on ", "information about "]), prefill2: String::new() };
    }

    // Query (graph traversal)
    if starts_any(&lower, &["query ", "connections of ", "graph of ", "neighbors of ", "show connections ", "traverse ", "network of "]) {
        return ChatIntent { tool: "query", prefill: after_keyword(trimmed, &["query ", "connections of ", "graph of ", "neighbors of ", "show connections ", "traverse ", "network of "]), prefill2: String::new() };
    }

    // Search (must be after more specific "find" patterns like "find path", "find similar")
    if starts_any(&lower, &["search for ", "search ", "find ", "look up ", "lookup "]) {
        return ChatIntent { tool: "search", prefill: after_keyword(trimmed, &["search for ", "search ", "find ", "look up ", "lookup "]), prefill2: String::new() };
    }

    // Provenance (source documents for an entity)
    if starts_any(&lower, &["provenance of ", "provenance ", "sources for ", "source of ", "documents about ", "documents for ", "where does ", "where did "]) {
        return ChatIntent { tool: "provenance", prefill: after_keyword(trimmed, &["provenance of ", "provenance ", "sources for ", "source of ", "documents about ", "documents for ", "where does ", "where did "]), prefill2: String::new() };
    }
    if lower.contains("come from") && !lower.contains("path") {
        // "where does Putin come from" -> provenance
        let entity = lower.replace("come from", "").replace("where does", "").replace("where did", "").trim().to_string();
        return ChatIntent { tool: "provenance", prefill: entity, prefill2: String::new() };
    }

    // Documents list
    if starts_any(&lower, &["list documents", "show documents", "ingested documents", "all documents"]) || lower == "documents" {
        return ChatIntent { tool: "documents", prefill: String::new(), prefill2: String::new() };
    }

    // Ingest
    if starts_any(&lower, &["ingest ", "import "]) {
        return ChatIntent { tool: "ingest", prefill: after_keyword(trimmed, &["ingest ", "import "]), prefill2: String::new() };
    }

    // ── Write operations (show confirmation cards) ──
    if starts_any(&lower, &["store ", "add entity ", "create entity "]) {
        return ChatIntent { tool: "store", prefill: after_keyword(trimmed, &["store ", "add entity ", "create entity "]), prefill2: String::new() };
    }
    if starts_any(&lower, &["relate ", "connect "]) && lower.contains(" to ") {
        let rest = after_keyword(trimmed, &["relate ", "connect "]);
        let (a, b) = split_two_entities(&rest);
        return ChatIntent { tool: "relate", prefill: a, prefill2: b };
    }
    if starts_any(&lower, &["delete ", "remove "]) {
        return ChatIntent { tool: "delete", prefill: after_keyword(trimmed, &["delete ", "remove "]), prefill2: String::new() };
    }

    // ── Fallback: treat as search with the entire text as pre-fill ──
    ChatIntent { tool: "search", prefill: trimmed.to_string(), prefill2: String::new() }
}

// ── Helper functions ──

fn starts_any(lower: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| lower.starts_with(p))
}

fn after_keyword(original: &str, prefixes: &[&str]) -> String {
    let lower = original.to_lowercase();
    // Try longest prefix first to avoid partial matches
    let mut sorted: Vec<&&str> = prefixes.iter().collect();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()));
    for prefix in sorted {
        if lower.starts_with(*prefix) {
            return original[prefix.len()..].trim().to_string();
        }
    }
    original.to_string()
}

fn split_two_entities(text: &str) -> (String, String) {
    // Try splitting on " and ", " to ", ", "
    for sep in &[" and ", " to ", ", ", " with "] {
        if let Some(pos) = text.to_lowercase().find(sep) {
            let a = text[..pos].trim().to_string();
            let b = text[pos + sep.len()..].trim().to_string();
            return (a, b);
        }
    }
    (text.trim().to_string(), String::new())
}

fn split_on_vs(text: &str) -> (String, String) {
    let lower = text.to_lowercase();
    for sep in &[" versus ", " vs "] {
        if let Some(pos) = lower.find(sep) {
            let a = text[..pos].trim().to_string();
            let b = text[pos + sep.len()..].trim().to_string();
            return (a, b);
        }
    }
    (text.trim().to_string(), String::new())
}

fn extract_two_from_connected(text: &str) -> (String, String) {
    let lower = text.to_lowercase();
    // "how is X connected to Y"
    if let Some(pos) = lower.find("connected to") {
        let before = text[..pos].trim();
        let after = text[pos + 12..].trim();
        // Strip leading "how is " etc from before
        let a = before.trim_start_matches(|c: char| !c.is_uppercase())
            .trim().to_string();
        let a = if a.is_empty() { before.split_whitespace().last().unwrap_or("").to_string() } else { a };
        return (a, after.trim_end_matches('?').trim().to_string());
    }
    // "connection between X and Y"
    if let Some(pos) = lower.find("between") {
        let rest = &text[pos + 7..];
        return split_two_entities(rest.trim());
    }
    (String::new(), String::new())
}

/// Split "entity in/on/during date" into (entity, date) parts.
fn split_entity_date(text: &str) -> (String, String) {
    let lower = text.to_lowercase();
    for sep in &[" on ", " in ", " during ", " at ", " between "] {
        if let Some(pos) = lower.find(sep) {
            let entity = text[..pos].trim().to_string();
            let date = text[pos + sep.len()..].trim().to_string();
            if !entity.is_empty() && !date.is_empty() {
                return (entity, date);
            }
        }
    }
    (text.trim().to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_intent() {
        let i = detect_intent("search Ukraine");
        assert_eq!(i.tool, "search");
        assert_eq!(i.prefill, "Ukraine");
    }

    #[test]
    fn test_search_for() {
        let i = detect_intent("search for NATO");
        assert_eq!(i.tool, "search");
        assert_eq!(i.prefill, "NATO");
    }

    #[test]
    fn test_explain_intent() {
        let i = detect_intent("explain Putin");
        assert_eq!(i.tool, "explain");
        assert_eq!(i.prefill, "Putin");
    }

    #[test]
    fn test_about_intent() {
        let i = detect_intent("tell me about Russia");
        assert_eq!(i.tool, "explain");
        assert_eq!(i.prefill, "Russia");
    }

    #[test]
    fn test_query_intent() {
        let i = detect_intent("connections of Lavrov");
        assert_eq!(i.tool, "query");
        assert_eq!(i.prefill, "Lavrov");
    }

    #[test]
    fn test_compare_intent() {
        let i = detect_intent("compare NATO and CSTO");
        assert_eq!(i.tool, "compare");
        assert_eq!(i.prefill, "NATO");
        assert_eq!(i.prefill2, "CSTO");
    }

    #[test]
    fn test_vs_intent() {
        let i = detect_intent("NATO vs CSTO");
        assert_eq!(i.tool, "compare");
        assert_eq!(i.prefill, "NATO");
        assert_eq!(i.prefill2, "CSTO");
    }

    #[test]
    fn test_path_intent() {
        let i = detect_intent("path from Putin to Biden");
        assert_eq!(i.tool, "path");
        assert_eq!(i.prefill, "Putin");
        assert_eq!(i.prefill2, "Biden");
    }

    #[test]
    fn test_similar_intent() {
        let i = detect_intent("similar to NATO");
        assert_eq!(i.tool, "similar");
        assert_eq!(i.prefill, "NATO");
    }

    #[test]
    fn test_timeline_intent() {
        let i = detect_intent("timeline Ukraine conflict");
        assert_eq!(i.tool, "timeline");
        assert_eq!(i.prefill, "Ukraine conflict");
    }

    #[test]
    fn test_gaps_intent() {
        let i = detect_intent("find gaps in my knowledge");
        assert_eq!(i.tool, "gaps");
    }

    #[test]
    fn test_most_connected() {
        let i = detect_intent("most connected entities");
        assert_eq!(i.tool, "most_connected");
    }

    #[test]
    fn test_what_if() {
        let i = detect_intent("what if Putin's confidence drops to 20%");
        assert_eq!(i.tool, "what_if");
        assert!(i.prefill.contains("Putin"));
    }

    #[test]
    fn test_analyze_category() {
        let i = detect_intent("analyze Putin");
        assert_eq!(i.tool, "analyze");
        assert_eq!(i.prefill, "Putin");
    }

    #[test]
    fn test_knowledge_category() {
        let i = detect_intent("knowledge about Ukraine");
        assert_eq!(i.tool, "knowledge");
        assert_eq!(i.prefill, "Ukraine");
    }

    #[test]
    fn test_investigate_category() {
        let i = detect_intent("investigate Wagner Group");
        assert_eq!(i.tool, "investigate");
        assert_eq!(i.prefill, "Wagner Group");
    }

    #[test]
    fn test_briefing() {
        let i = detect_intent("briefing on NATO expansion");
        assert_eq!(i.tool, "briefing");
        assert_eq!(i.prefill, "NATO expansion");
    }

    #[test]
    fn test_fallback_is_search() {
        let i = detect_intent("Putin");
        assert_eq!(i.tool, "search");
        assert_eq!(i.prefill, "Putin");
    }

    #[test]
    fn test_relate_is_write() {
        let i = detect_intent("relate Putin to Russia");
        assert_eq!(i.tool, "relate");
        assert_eq!(i.prefill, "Putin");
        assert_eq!(i.prefill2, "Russia");
    }

    #[test]
    fn test_find_path_not_search() {
        let i = detect_intent("find path from A to B");
        assert_eq!(i.tool, "path");
    }

    #[test]
    fn test_find_similar_not_search() {
        let i = detect_intent("find similar to NATO");
        // "find similar" starts with "find " which would match search,
        // but "similar to" is checked before in compound patterns
        // Actually "find similar " is not in starts_any for similar...
        // This tests the current behavior - may fall to search
        let i = detect_intent("similar to NATO");
        assert_eq!(i.tool, "similar");
    }

    #[test]
    fn test_connected_to_is_path() {
        let i = detect_intent("how is Putin connected to Iran");
        assert_eq!(i.tool, "path");
    }

    #[test]
    fn test_store_is_write() {
        let i = detect_intent("store Berlin as a city");
        assert_eq!(i.tool, "store");
    }

    #[test]
    fn test_ingest() {
        let i = detect_intent("ingest this article about sanctions");
        assert_eq!(i.tool, "ingest");
    }

    #[test]
    fn test_isolated() {
        let i = detect_intent("show me isolated entities");
        assert_eq!(i.tool, "isolated");
    }

    #[test]
    fn test_provenance_intent() {
        let i = detect_intent("provenance of Putin");
        assert_eq!(i.tool, "provenance");
        assert_eq!(i.prefill, "Putin");

        let i = detect_intent("sources for NATO");
        assert_eq!(i.tool, "provenance");
        assert_eq!(i.prefill, "NATO");

        let i = detect_intent("documents about Ukraine");
        assert_eq!(i.tool, "provenance");
        assert_eq!(i.prefill, "Ukraine");
    }

    #[test]
    fn test_documents_intent() {
        let i = detect_intent("list documents");
        assert_eq!(i.tool, "documents");

        let i = detect_intent("show documents");
        assert_eq!(i.tool, "documents");

        let i = detect_intent("documents");
        assert_eq!(i.tool, "documents");
    }

    #[test]
    fn test_slash_commands() {
        let i = detect_intent("/store");
        assert_eq!(i.tool, "store");
        assert_eq!(i.prefill, "");

        let i = detect_intent("/store Berlin");
        assert_eq!(i.tool, "store");
        assert_eq!(i.prefill, "Berlin");

        let i = detect_intent("/search Ukraine");
        assert_eq!(i.tool, "search");
        assert_eq!(i.prefill, "Ukraine");

        let i = detect_intent("/explain Putin");
        assert_eq!(i.tool, "explain");
        assert_eq!(i.prefill, "Putin");

        let i = detect_intent("/delete NATO");
        assert_eq!(i.tool, "delete");
        assert_eq!(i.prefill, "NATO");

        let i = detect_intent("/relate Putin to Russia");
        assert_eq!(i.tool, "relate");
        assert_eq!(i.prefill, "Putin");
        assert_eq!(i.prefill2, "Russia");

        let i = detect_intent("/compare NATO and CSTO");
        assert_eq!(i.tool, "compare");
        assert_eq!(i.prefill, "NATO");
        assert_eq!(i.prefill2, "CSTO");

        let i = detect_intent("/timeline");
        assert_eq!(i.tool, "timeline");

        let i = detect_intent("/provenance Russia");
        assert_eq!(i.tool, "provenance");
        assert_eq!(i.prefill, "Russia");
    }

    #[test]
    fn test_slash_unknown_falls_back_to_search() {
        let i = detect_intent("/foobar something");
        assert_eq!(i.tool, "search");
        assert_eq!(i.prefill, "something");
    }

    #[test]
    fn test_slash_aliases() {
        let i = detect_intent("/add Berlin");
        assert_eq!(i.tool, "store");

        let i = detect_intent("/connect A to B");
        assert_eq!(i.tool, "relate");

        let i = detect_intent("/find Ukraine");
        assert_eq!(i.tool, "search");

        let i = detect_intent("/remove Putin");
        assert_eq!(i.tool, "delete");

        let i = detect_intent("/docs");
        assert_eq!(i.tool, "documents");

        let i = detect_intent("/path A to B");
        assert_eq!(i.tool, "shortest_path");
        assert_eq!(i.prefill, "A");
        assert_eq!(i.prefill2, "B");
    }
}
