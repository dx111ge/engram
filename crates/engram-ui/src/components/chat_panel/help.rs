//! /help command handler -- local help text generation, no LLM needed.

/// All slash commands available for autocomplete.
/// Each entry: (command, icon class, short description).
pub const SLASH_COMMANDS: &[(&str, &str, &str)] = &[
    ("/help", "fa-solid fa-circle-question", "Show available commands"),
    ("/help knowledge", "fa-solid fa-database", "Knowledge tools"),
    ("/help analysis", "fa-solid fa-chart-line", "Analysis tools"),
    ("/help temporal", "fa-solid fa-clock", "Temporal tools"),
    ("/help investigation", "fa-solid fa-magnifying-glass", "Investigation tools"),
    ("/help assessment", "fa-solid fa-scale-balanced", "Assessment tools"),
    ("/help reasoning", "fa-solid fa-code-branch", "Reasoning tools"),
    ("/help actions", "fa-solid fa-gears", "Action tools"),
    ("/help reporting", "fa-solid fa-file-lines", "Reporting tools"),
];

/// Generate a help response for the given input.
/// Returns the text content for a ChatMessage with role Assistant.
pub fn generate_help_response(input: &str) -> String {
    let arg = input.trim().strip_prefix("/help").unwrap_or("").trim().to_lowercase();

    match arg.as_str() {
        "" => help_overview(),
        "knowledge" => help_knowledge(),
        "analysis" => help_analysis(),
        "temporal" => help_temporal(),
        "investigation" => help_investigation(),
        "assessment" => help_assessment(),
        "reasoning" => help_reasoning(),
        "actions" => help_actions(),
        "reporting" => help_reporting(),
        other => format!(
            "Unknown help category: \"{}\"\n\n\
             Available categories: knowledge, analysis, temporal, investigation,\n\
             assessment, reasoning, actions, reporting\n\n\
             Type /help for the full overview.",
            other
        ),
    }
}

fn help_overview() -> String {
    [
        "Knowledge Chat -- Available Commands",
        "",
        "  Knowledge (10)     query, search, explain, store, relate...",
        "  Analysis (4)       compare, shortest path, most connected, isolated",
        "  Temporal (3)       timeline, date queries, current state",
        "  Investigation (5)  ingest, analyze, investigate, changes, watch",
        "  Assessment (6)     hypotheses, evidence, evaluate",
        "  Reasoning (2)      what-if, influence paths",
        "  Actions (4)        rules, inference, scheduling",
        "  Reporting (3)      briefings, export, entity timeline",
        "",
        "Type /help <category> for details.",
        "Just ask naturally -- no commands needed!",
    ].join("\n")
}

fn help_knowledge() -> String {
    "\
Knowledge Tools:\n\
\n\
  query       Traverse graph from entity\n\
              \"Show me connections of Putin\"\n\
\n\
  search      Full-text keyword search\n\
              \"Search for Ukraine\"\n\
\n\
  explain     Provenance, confidence, edges\n\
              \"Explain Lavrov\"\n\
\n\
  similar     Semantic similarity search\n\
              \"Find entities similar to NATO\"\n\
\n\
  store       Add new entity [write]\n\
              \"Store that Berlin is a city\"\n\
\n\
  relate      Create relationship [write]\n\
              \"Connect Putin to Russia as president\"\n\
\n\
  reinforce   Boost confidence [write]\n\
              \"Reinforce the claim about sanctions\"\n\
\n\
  correct     Mark as wrong [write]\n\
              \"Correct the outdated entry for...\"\n\
\n\
  delete      Soft-delete [write]\n\
              \"Delete the duplicate entry for...\"\n\
\n\
  prove       Find evidence for a claim\n\
              \"Prove that X is connected to Y\""
        .to_string()
}

fn help_analysis() -> String {
    "\
Analysis Tools:\n\
\n\
  compare         Compare two entities side-by-side\n\
                  \"Compare NATO and CSTO\"\n\
\n\
  shortest_path   Find shortest path between entities\n\
                  \"Shortest path from Putin to Biden\"\n\
\n\
  most_connected  Find the most connected entities\n\
                  \"What are the most connected nodes?\"\n\
\n\
  isolated        Find entities with no or few connections\n\
                  \"Show me isolated entities\""
        .to_string()
}

fn help_temporal() -> String {
    "\
Temporal Tools:\n\
\n\
  timeline      Show entity changes over time\n\
                \"Timeline of Ukraine conflict\"\n\
\n\
  date_query    Query knowledge at a specific date\n\
                \"What did I know about sanctions in January?\"\n\
\n\
  current       Show current state vs historical\n\
                \"Current confidence for all entities about Iran\""
        .to_string()
}

fn help_investigation() -> String {
    "\
Investigation Tools:\n\
\n\
  ingest        Extract entities from text [write]\n\
                \"Ingest this article about...\"\n\
\n\
  analyze       Deep analysis of entity neighborhood\n\
                \"Analyze the network around Lavrov\"\n\
\n\
  investigate   Full investigation with depth control [write]\n\
                \"Investigate Wagner Group deeply\"\n\
\n\
  changes       Show recent graph changes\n\
                \"What changed in the last 24 hours?\"\n\
\n\
  watch         Mark entity for monitoring [write]\n\
                \"Watch Prigozhin for changes\""
        .to_string()
}

fn help_assessment() -> String {
    "\
Assessment Tools:\n\
\n\
  assess_create     Create new hypothesis [write]\n\
                    \"Create assessment: Will sanctions hold?\"\n\
\n\
  assess_evidence   Add evidence to assessment [write]\n\
                    \"Add supporting evidence from...\"\n\
\n\
  assess_evaluate   Evaluate current hypothesis confidence\n\
                    \"Evaluate my assessment on...\"\n\
\n\
  assess_list       List all active assessments\n\
                    \"Show my assessments\"\n\
\n\
  assess_detail     Show assessment with all evidence\n\
                    \"Detail on assessment about sanctions\"\n\
\n\
  assess_compare    Compare competing hypotheses\n\
                    \"Compare assessments about leadership change\""
        .to_string()
}

fn help_reasoning() -> String {
    "\
Reasoning Tools:\n\
\n\
  what_if       Simulate confidence changes and cascading effects\n\
                \"What if Putin's health confidence drops to 20%?\"\n\
\n\
  influence     Find influence paths between entities\n\
                \"How does China influence the Iran deal?\""
        .to_string()
}

fn help_actions() -> String {
    "\
Action Tools:\n\
\n\
  rule_create   Create inference rule [write]\n\
                \"Create rule: if sanctions then economic impact\"\n\
\n\
  rule_list     List all active rules\n\
                \"Show my rules\"\n\
\n\
  rule_fire     Manually trigger rule evaluation\n\
                \"Fire rules on recent changes\"\n\
\n\
  schedule      View or create scheduled tasks\n\
                \"What's scheduled for today?\""
        .to_string()
}

fn help_reporting() -> String {
    "\
Reporting Tools:\n\
\n\
  briefing        Generate intelligence briefing\n\
                  \"Brief me on the current state of...\"\n\
\n\
  export          Export entities or subgraph\n\
                  \"Export all entities about Ukraine\"\n\
\n\
  entity_timeline Show full history of an entity\n\
                  \"Show the complete timeline for Wagner Group\""
        .to_string()
}

/// Filter slash commands by a partial query string (without leading `/`).
/// Returns matching (command, icon, description) tuples.
pub fn filter_commands(query: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let query_lower = query.to_lowercase();
    SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _, _)| {
            let cmd_body = &cmd[1..]; // strip leading /
            cmd_body.to_lowercase().contains(&query_lower) || query_lower.is_empty()
        })
        .copied()
        .collect()
}
