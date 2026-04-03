/// Agent persona generation, system prompts, turn execution, and tool dispatch
/// for the multi-agent debate panel.

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content};
use super::research::execute_web_search;

// ── Persona auto-generation ─────────────────────────────────────────────

const COGNITIVE_STYLES: &[CognitiveStyle] = &[
    CognitiveStyle::PatternSeeking,
    CognitiveStyle::Skeptical,
    CognitiveStyle::Contrarian,
    CognitiveStyle::DevilsAdvocate,
    CognitiveStyle::Empirical,
    CognitiveStyle::Intuitive,
    CognitiveStyle::Systemic,
];

const SOURCE_OPTIONS: &[SourceAccess] = &[
    SourceAccess::Comprehensive,
    SourceAccess::Comprehensive,
    SourceAccess::BriefingFocused,
    SourceAccess::Contrarian,
];

const COLORS: &[&str] = &[
    "#e74c3c", "#3498db", "#2ecc71", "#f39c12", "#9b59b6",
    "#1abc9c", "#e67e22", "#34495e",
];

const ICONS: &[&str] = &[
    "fa-user-secret", "fa-microscope", "fa-hat-wizard", "fa-balance-scale",
    "fa-flask", "fa-brain", "fa-network-wired", "fa-eye",
];

/// Deterministic slot assignment for agent diversity.
pub fn assign_agent_slots(count: u8) -> Vec<DebateAgent> {
    let n = count.max(2).min(8) as usize;
    let mut agents = Vec::with_capacity(n);

    for i in 0..n {
        let rigor = if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
        let evidence_threshold = rigor * 0.5 + 0.1;
        let source = SOURCE_OPTIONS[i % SOURCE_OPTIONS.len()].clone();
        let style = COGNITIVE_STYLES[i % COGNITIVE_STYLES.len()].clone();
        // First and last agents are neutral; middle ones get biases (filled by LLM)
        let is_neutral = i == 0 || (n > 2 && i == n - 1);

        agents.push(DebateAgent {
            id: format!("agent-{}", i + 1),
            name: String::new(),
            persona_description: String::new(),
            rigor_level: rigor,
            source_access: source,
            evidence_threshold,
            cognitive_style: style,
            bias: AgentBias {
                label: if is_neutral { "Neutral analyst".into() } else { String::new() },
                description: if is_neutral { "Follow evidence wherever it leads".into() } else { String::new() },
                is_neutral,
            },
            icon: ICONS[i % ICONS.len()].into(),
            color: COLORS[i % COLORS.len()].into(),
        });
    }
    agents
}

/// Build the LLM prompt to generate persona details + biases for the assigned slots.
pub fn build_persona_generation_prompt(topic: &str, agents: &[DebateAgent], mode: &DebateMode, mode_input: Option<&str>) -> serde_json::Value {
    let mode_rules = super::modes::persona_rules(mode, mode_input);
    let mut agent_descriptions = String::new();
    for a in agents {
        agent_descriptions.push_str(&format!(
            "Agent {}: rigor={:.1} ({}), source_access={}, cognitive_style={}, neutral={}\n",
            a.id,
            a.rigor_level,
            if a.rigor_level < 0.3 { "conspiracy-leaning, low standards" }
            else if a.rigor_level < 0.7 { "moderate rigor" }
            else { "strict evidence-only" },
            a.source_access,
            a.cognitive_style,
            a.bias.is_neutral,
        ));
    }

    let system_prompt = format!(
        r##"You are generating a diverse panel of {} participants for a structured analysis session.
Each participant has pre-assigned characteristics (rigor level, cognitive style).
Your job: generate a name, background, role assignment, and visual identity for each.

{}

Topic: "{}"

Pre-assigned slots:
{}

RULES:
- Agents marked neutral=true MUST have bias_label "Neutral analyst" and bias_description "Follow evidence wherever it leads"
- Agents marked neutral=false MUST have a specific bias/agenda relevant to the topic
  The bias represents what stakeholder group they fight for (e.g., "Industry lobby", "Civil liberties advocate", "Military-industrial complex")
- Low-rigor agents (rigor < 0.3) should have colorful, extreme personas (conspiracy theorist, social media "researcher", tabloid journalist)
- High-rigor agents (rigor > 0.7) should be serious professionals (forensic analyst, academic researcher, veteran intelligence officer)
- icon must be a Font Awesome class (fa-user-secret, fa-microscope, fa-hat-wizard, fa-balance-scale, fa-flask, fa-brain, fa-network-wired, fa-eye, fa-shield-alt, fa-search, fa-gavel, fa-globe-americas, fa-chess)
- color must be a distinct CSS hex color

Return ONLY a JSON array (no markdown, no commentary):
[
  {{
    "id": "agent-1",
    "name": "Dr. Example",
    "persona_description": "1-2 sentences about background and argumentative style",
    "bias_label": "Neutral analyst or specific bias",
    "bias_description": "what they fight for",
    "icon": "fa-xxx",
    "color": "#hexcolor"
  }}
]"##,
        agents.len(), mode_rules, topic, agent_descriptions
    );

    serde_json::json!({
        "messages": [
            { "role": "system", "content": system_prompt }
        ],
        "temperature": 0.9,
        "max_tokens": 2048
    })
}

// ── Tool definitions for agents ─────────────────────────────────────────

/// Build the tool definitions available to a debate agent based on source access.
pub fn tools_for_agent(agent: &DebateAgent) -> serde_json::Value {
    let mut tools = vec![
        tool_def("engram_search", "Search the knowledge graph for entities and relationships matching a query", serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 10)" }
            },
            "required": ["query"]
        })),
        tool_def("engram_query", "Traverse the graph from a start entity to find connected entities and relationships", serde_json::json!({
            "type": "object",
            "properties": {
                "start": { "type": "string", "description": "Starting entity label" },
                "depth": { "type": "integer", "description": "Traversal depth (default 2)" }
            },
            "required": ["start"]
        })),
        tool_def("engram_explain", "Get detailed explanation of an entity including provenance and confidence", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Entity label to explain" }
            },
            "required": ["entity"]
        })),
        tool_def("engram_what_if", "Simulate confidence cascade: what happens if an entity's confidence changes", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Entity to simulate" },
                "new_confidence": { "type": "number", "description": "Hypothetical new confidence (0-1)" }
            },
            "required": ["entity", "new_confidence"]
        })),
        tool_def("engram_contradictions", "Find contradicting facts in the knowledge graph", serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Optional entity to focus on" }
            }
        })),
    ];

    // Add web search for GraphAndWeb and WebOnly agents
    if matches!(agent.source_access, SourceAccess::GraphAndWeb | SourceAccess::WebOnly) {
        tools.push(tool_def("engram_investigate", "Search the web for information on a topic and optionally ingest results", serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Web search query" }
            },
            "required": ["query"]
        })));
    }

    serde_json::json!(tools)
}

fn tool_def(name: &str, description: &str, parameters: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

// ── Tool execution ──────────────────────────────────────────────────────

/// Execute a tool call against the local graph (server-side, no HTTP round-trip).
/// Uses the label-based Graph API (search_text, edges_from, node_confidence, etc.).
async fn execute_tool(state: &AppState, tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    match tool_name {
        "engram_search" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            match g.search_text(query, limit) {
                Ok(results) => {
                    let items: Vec<serde_json::Value> = results.iter().map(|r| {
                        serde_json::json!({
                            "label": r.label,
                            "confidence": r.confidence,
                            "score": r.score
                        })
                    }).collect();
                    serde_json::json!({"results": items})
                }
                Err(e) => serde_json::json!({"error": e.to_string()}),
            }
        }
        "engram_query" => {
            let start = args.get("start").and_then(|s| s.as_str()).unwrap_or("");
            let depth = args.get("depth").and_then(|d| d.as_u64()).unwrap_or(2) as usize;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            // BFS traversal using label-based API
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            let mut results = Vec::new();
            queue.push_back((start.to_string(), 0usize));
            visited.insert(start.to_string());
            while let Some((label, d)) = queue.pop_front() {
                if d > depth { continue; }
                let conf = g.node_confidence(&label).unwrap_or(None).unwrap_or(0.0);
                let nt = g.get_node_type(&label).unwrap_or_else(|| "entity".into());
                let edges = g.edges_from(&label).unwrap_or_default();
                let mut edge_items = Vec::new();
                for e in edges.iter().take(20) {
                    if !visited.contains(&e.to) && d + 1 <= depth {
                        visited.insert(e.to.clone());
                        queue.push_back((e.to.clone(), d + 1));
                    }
                    edge_items.push(serde_json::json!({
                        "rel": e.relationship,
                        "target": e.to,
                        "confidence": e.confidence
                    }));
                }
                results.push(serde_json::json!({
                    "label": label,
                    "type": nt,
                    "confidence": conf,
                    "depth": d,
                    "edges": edge_items
                }));
            }
            serde_json::json!({"traversal": results})
        }
        "engram_explain" => {
            let entity = args.get("entity").and_then(|e| e.as_str()).unwrap_or("");
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            let conf = g.node_confidence(entity).unwrap_or(None);
            match conf {
                Some(c) => {
                    let nt = g.get_node_type(entity).unwrap_or_else(|| "entity".into());
                    let edges = g.edges_from(entity).unwrap_or_default();
                    let edge_items: Vec<serde_json::Value> = edges.iter().take(20).map(|e| {
                        serde_json::json!({
                            "rel": e.relationship,
                            "target": e.to,
                            "confidence": e.confidence
                        })
                    }).collect();
                    serde_json::json!({
                        "label": entity,
                        "type": nt,
                        "confidence": c,
                        "edge_count": edges.len(),
                        "edges": edge_items
                    })
                }
                None => serde_json::json!({"error": format!("entity '{}' not found", entity)}),
            }
        }
        "engram_what_if" => {
            let entity = args.get("entity").and_then(|e| e.as_str()).unwrap_or("");
            let new_conf = args.get("new_confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            match g.node_confidence(entity).unwrap_or(None) {
                Some(current_conf) => {
                    let delta = new_conf - current_conf;
                    let edges = g.edges_from(entity).unwrap_or_default();
                    let affected: Vec<serde_json::Value> = edges.iter().take(30).filter_map(|e| {
                        let neighbor_conf = g.node_confidence(&e.to).unwrap_or(None).unwrap_or(0.5);
                        let propagated = delta * e.confidence * 0.5;
                        Some(serde_json::json!({
                            "entity": e.to,
                            "current_confidence": neighbor_conf,
                            "impact": propagated,
                            "simulated": (neighbor_conf + propagated).clamp(0.05, 0.95)
                        }))
                    }).collect();
                    serde_json::json!({
                        "entity": entity,
                        "current_confidence": current_conf,
                        "simulated_confidence": new_conf,
                        "affected": affected
                    })
                }
                None => serde_json::json!({"error": format!("entity '{}' not found", entity)}),
            }
        }
        "engram_contradictions" => {
            let entity = args.get("entity").and_then(|e| e.as_str());
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => return serde_json::json!({"error": "graph lock failed"}),
            };
            let mut contradictions = Vec::new();
            if let Some(label) = entity {
                let edges = g.edges_from(label).unwrap_or_default();
                for e in edges.iter().take(50) {
                    if e.confidence < 0.3 {
                        contradictions.push(serde_json::json!({
                            "from": label,
                            "rel": e.relationship,
                            "to": e.to,
                            "confidence": e.confidence,
                            "issue": "low confidence suggests disputed or uncertain"
                        }));
                    }
                }
            }
            serde_json::json!({"contradictions": contradictions})
        }
        "engram_investigate" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            serde_json::json!({
                "note": "Web search capability. Results would come from configured web search provider.",
                "query": query,
                "status": "web_search_placeholder"
            })
        }
        _ => serde_json::json!({"error": format!("unknown tool: {}", tool_name)}),
    }
}

// ── Agent system prompt ─────────────────────────────────────────────────

/// Build the system prompt for an agent's turn.
fn build_agent_system_prompt(
    agent: &DebateAgent,
    topic: &str,
    round: usize,
    previous_turns: &[DebateTurn],
    agents: &[DebateAgent],
    user_injection: Option<&str>,
    gap_research: &[GapResearch],
    briefing_summary: &str,
    mode: &DebateMode,
    mode_input: Option<&str>,
) -> String {
    let mut prompt = format!(
        "You are {}, {}.\n\n",
        agent.name, agent.persona_description
    );

    // Include the briefing (factual foundation for all agents)
    if !briefing_summary.is_empty() {
        prompt.push_str(&format!("{}\n\n", briefing_summary));
        prompt.push_str("The above briefing contains verified facts. Use them as the foundation for your analysis.\n\n");
    }

    // Rigor instructions
    prompt.push_str("Your analysis characteristics:\n");
    prompt.push_str(&format!("- Rigor level: {:.0}% ({})\n",
        agent.rigor_level * 100.0,
        if agent.rigor_level < 0.3 { "you follow hunches, rumors, and unverified claims freely" }
        else if agent.rigor_level < 0.7 { "you balance evidence with reasonable inference" }
        else { "you ONLY cite verified, high-confidence evidence; reject all speculation" }
    ));
    prompt.push_str(&format!("- Evidence standard: only cite facts with confidence >= {:.2}\n", agent.evidence_threshold));
    prompt.push_str(&format!("- Cognitive style: {}\n", agent.cognitive_style));
    prompt.push_str(&format!("- Source access: {}\n\n", agent.source_access));

    // Bias instructions
    if agent.bias.is_neutral {
        prompt.push_str("You are an unbiased analyst. Follow the evidence wherever it leads, regardless of political or institutional implications.\n\n");
    } else {
        prompt.push_str(&format!(
            "You represent the **{}** perspective. Your role is to argue for: {}. You may use evidence selectively to support your position. You are not seeking truth -- you are advocating.\n\n",
            agent.bias.label, agent.bias.description
        ));
    }

    // Mode-specific instructions
    let mode_addition = super::modes::agent_prompt_addition(mode, mode_input);
    if !mode_addition.is_empty() {
        prompt.push_str(&mode_addition);
    }

    prompt.push_str(&format!("You are participating in Round {} of a structured analysis on: \"{}\"\n\n", round + 1, topic));

    // Previous round context
    if !previous_turns.is_empty() {
        prompt.push_str("Previous round positions:\n");
        for turn in previous_turns {
            let agent_name = agents.iter().find(|a| a.id == turn.agent_id)
                .map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            prompt.push_str(&format!("- {} said: \"{}\" (confidence: {:.2})\n",
                agent_name, truncate(&turn.position, 300), turn.confidence));
        }
        prompt.push('\n');

        // Evolution instructions (only after round 1)
        prompt.push_str(
            "IMPORTANT -- POSITION EVOLUTION:\n\
             This is NOT round 1. You have heard other agents' arguments. You MUST evolve your thinking:\n\
             - If another agent presented compelling evidence you hadn't considered, ACKNOWLEDGE it and adjust your position.\n\
             - If your confidence should change based on what you heard, change it. Don't stubbornly hold the same number.\n\
             - You CAN concede specific points while maintaining your overall stance.\n\
             - You CAN shift your position entirely if the evidence warrants it.\n\
             - Real analysts update their views. Stubbornly repeating the same position without engaging with new evidence is a sign of bias, not strength.\n"
        );
        if !agent.bias.is_neutral {
            prompt.push_str(
                "- Even as an advocate, you should acknowledge when opposing evidence is strong. \
                 Credibility comes from honest engagement, not blind advocacy.\n"
            );
        }
        prompt.push('\n');
    }

    // User injection
    if let Some(injection) = user_injection {
        prompt.push_str(&format!("The moderator has asked: \"{}\"\nYou MUST address this in your response.\n\n", injection));
    }

    // Gap research from previous round
    if !gap_research.is_empty() {
        prompt.push_str("NEW EVIDENCE discovered since last round (from targeted gap-closing research):\n");
        for gr in gap_research {
            if !gr.findings.is_empty() {
                prompt.push_str(&format!("  Gap investigated: \"{}\"\n", gr.gap_query));
                for finding in &gr.findings {
                    prompt.push_str(&format!("    - {}\n", finding));
                }
            }
        }
        prompt.push_str("\nYou MUST incorporate this new evidence into your analysis. If it changes your position, say so.\n\n");
    }

    prompt.push_str(
        "Use the provided tools to gather evidence from the knowledge graph before forming your position.\n\
         After gathering evidence, state your position clearly.\n\n\
         At the END of your response, include these EXACT lines:\n\
         CONFIDENCE: X.XX\n\
         AGREES_WITH: [comma-separated agent names, or \"none\"]\n\
         DISAGREES_WITH: [comma-separated agent names, or \"none\"]\n"
    );

    // Evolution metadata (only after round 1)
    if !previous_turns.is_empty() {
        prompt.push_str(
            "POSITION_SHIFT: [1 sentence: what changed in your thinking since last round, or \"No change\" if nothing did]\n\
             CONCESSIONS: [comma-separated specific points you now agree with from other agents, or \"none\"]\n"
        );
    }

    prompt
}

// ── Helpers ─────────────────────────────────────────────────────────────

pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

/// Parsed metadata from an agent's turn response.
pub struct TurnMetadata {
    pub confidence: f32,
    pub agrees_with: Vec<String>,
    pub disagrees_with: Vec<String>,
    pub position_shift: String,
    pub concessions: Vec<String>,
}

/// Parse confidence, agrees/disagrees, position_shift, concessions from agent response.
pub fn parse_turn_metadata(text: &str, agents: &[DebateAgent]) -> TurnMetadata {
    let mut confidence = 0.5;
    let mut agrees = Vec::new();
    let mut disagrees = Vec::new();
    let mut position_shift = String::new();
    let mut concessions = Vec::new();

    for line in text.lines().rev().take(15) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("CONFIDENCE:") {
            if let Ok(c) = rest.trim().parse::<f32>() {
                confidence = c.clamp(0.0, 1.0);
            }
        } else if let Some(rest) = line.strip_prefix("AGREES_WITH:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                agrees = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        } else if let Some(rest) = line.strip_prefix("DISAGREES_WITH:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                disagrees = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        } else if let Some(rest) = line.strip_prefix("POSITION_SHIFT:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "no change" && !cleaned.is_empty() {
                position_shift = cleaned.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("CONCESSIONS:") {
            let cleaned = rest.trim().trim_start_matches('[').trim_end_matches(']');
            if cleaned.to_lowercase() != "none" {
                concessions = cleaned.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        }
    }

    // Map agent names to IDs
    let name_to_id = |name: &str| -> String {
        agents.iter()
            .find(|a| a.name.eq_ignore_ascii_case(name) || a.id == name)
            .map(|a| a.id.clone())
            .unwrap_or_else(|| name.to_string())
    };
    agrees = agrees.iter().map(|n| name_to_id(n)).collect();
    disagrees = disagrees.iter().map(|n| name_to_id(n)).collect();

    TurnMetadata { confidence, agrees_with: agrees, disagrees_with: disagrees, position_shift, concessions }
}

/// Strip metadata lines from the position text.
/// Removes CONFIDENCE/AGREES_WITH/etc. lines from ANYWHERE in the text (not just the end).
/// Also strips raw `<tool_call>` XML blocks that some models output.
pub fn strip_metadata_lines(text: &str) -> String {
    let metadata_prefixes = ["CONFIDENCE:", "AGREES_WITH:", "DISAGREES_WITH:", "POSITION_SHIFT:", "CONCESSIONS:", "POSITION:"];
    let lines: Vec<&str> = text.lines().collect();
    let mut filtered = Vec::new();
    let mut in_tool_call = false;
    for line in &lines {
        let trimmed = line.trim();
        // Skip metadata lines
        if metadata_prefixes.iter().any(|p| trimmed.starts_with(p)) {
            continue;
        }
        // Skip <tool_call> blocks
        if trimmed.starts_with("<tool_call>") {
            in_tool_call = true;
            continue;
        }
        if trimmed.starts_with("</tool_call>") {
            in_tool_call = false;
            continue;
        }
        if in_tool_call {
            continue;
        }
        filtered.push(*line);
    }
    // Trim leading/trailing blank lines
    let result = filtered.join("\n");
    result.trim().to_string()
}

// ── Agent turn execution ────────────────────────────────────────────────

/// Execute a single agent's turn using research-then-respond pattern.
/// Phase 1: Automatically gather evidence from graph + web based on source_access.
/// Phase 2: Feed research results to LLM, get position (no function calling, just text).
pub async fn execute_agent_turn(
    state: &AppState,
    agent: &DebateAgent,
    topic: &str,
    round: usize,
    previous_turns: &[DebateTurn],
    all_agents: &[DebateAgent],
    user_injection: Option<&str>,
    gap_research: &[GapResearch],
    briefing_summary: &str,
    mode: &DebateMode,
    mode_input: Option<&str>,
    tx: &tokio::sync::broadcast::Sender<String>,
) -> Result<DebateTurn, String> {
    let mut all_tool_invocations = Vec::new();
    let mut all_evidence = Vec::new();
    let mut research_summary = String::new();

    // ── Phase 1: Automated research ──

    // Extract key terms from topic for searching
    let search_terms = vec![
        topic.to_string(),
        // Also search for individual key entities in the topic
    ];

    // Graph search (all agents have full access now; briefing is the primary source)
    {
        for term in &search_terms {
            let _ = tx.send(format!("event: tool_call\ndata: {}\n\n", serde_json::json!({
                "agent_id": agent.id, "tool_name": "engram_search", "args": {"query": term}
            })));

            let result = execute_tool(state, "engram_search", &serde_json::json!({"query": term, "limit": 10})).await;

            // Extract evidence
            if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
                if !results.is_empty() {
                    research_summary.push_str(&format!("\n[Graph search for \"{}\"]:\n", term));
                    for r in results.iter().take(8) {
                        let label = r.get("label").and_then(|l| l.as_str()).unwrap_or("");
                        let conf = r.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
                        let score = r.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                        if !label.is_empty() {
                            research_summary.push_str(&format!("  - {} (confidence: {:.2}, relevance: {:.2})\n", label, conf, score));
                            all_evidence.push(TurnEvidence {
                                entity: label.to_string(),
                                confidence: conf,
                                source: "graph".into(),
                                supporting: true,
                            });
                        } else if !label.is_empty() {
                            research_summary.push_str(&format!("  - {} (confidence: {:.2}, relevance: {:.2})\n", label, conf, score));
                            all_evidence.push(TurnEvidence {
                                entity: label.to_string(),
                                confidence: conf,
                                source: "graph".into(),
                                supporting: true,
                            });
                        }
                    }
                    all_tool_invocations.push(ToolInvocation {
                        tool_name: "engram_search".into(),
                        arguments: serde_json::json!({"query": term}),
                        result_summary: format!("{} results", results.len()),
                    });
                }
            }

            // Also traverse from top results
            if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
                for r in results.iter().take(3) {
                    let label = r.get("label").and_then(|l| l.as_str()).unwrap_or("");
                    if !label.is_empty() {
                        let query_result = execute_tool(state, "engram_query", &serde_json::json!({"start": label, "depth": 1})).await;
                        if let Some(traversal) = query_result.get("traversal").and_then(|t| t.as_array()) {
                            for node in traversal.iter().take(5) {
                                let edges = node.get("edges").and_then(|e| e.as_array());
                                if let Some(edges) = edges {
                                    for e in edges.iter().take(5) {
                                        let rel = e.get("rel").and_then(|r| r.as_str()).unwrap_or("");
                                        let target = e.get("target").and_then(|t| t.as_str()).unwrap_or("");
                                        let econf = e.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5);
                                        if !rel.is_empty() && !target.is_empty() {
                                            research_summary.push_str(&format!("  - {} --[{}]--> {} (confidence: {:.2})\n", label, rel, target, econf));
                                        }
                                    }
                                }
                            }
                            all_tool_invocations.push(ToolInvocation {
                                tool_name: "engram_query".into(),
                                arguments: serde_json::json!({"start": label}),
                                result_summary: format!("{} nodes traversed", traversal.len()),
                            });
                        }
                    }
                }
            }
        }
    }

    // Web search (all agents get web access now)
    {
        let _ = tx.send(format!("event: tool_call\ndata: {}\n\n", serde_json::json!({
            "agent_id": agent.id, "tool_name": "web_search", "args": {"query": topic}
        })));

        let web_result = execute_web_search(state, topic).await;
        if !web_result.is_empty() {
            research_summary.push_str(&format!("\n[Web search for \"{}\"]:\n{}\n", topic, web_result));
            all_tool_invocations.push(ToolInvocation {
                tool_name: "web_search".into(),
                arguments: serde_json::json!({"query": topic}),
                result_summary: format!("{} chars of results", web_result.len()),
            });
            // Add web evidence
            all_evidence.push(TurnEvidence {
                entity: topic.to_string(),
                confidence: 0.4,
                source: "web".into(),
                supporting: true,
            });
        }

        // If there's a user injection, search for that too
        if let Some(injection) = user_injection {
            let inj_result = execute_web_search(state, injection).await;
            if !inj_result.is_empty() {
                research_summary.push_str(&format!("\n[Web search for \"{}\"]:\n{}\n", injection, inj_result));
                all_tool_invocations.push(ToolInvocation {
                    tool_name: "web_search".into(),
                    arguments: serde_json::json!({"query": injection}),
                    result_summary: format!("{} chars", inj_result.len()),
                });
            }
        }
    }

    let _ = tx.send(format!("event: tool_result\ndata: {}\n\n", serde_json::json!({
        "agent_id": agent.id,
        "tool_name": "research_complete",
        "summary": format!("{} evidence items gathered", all_evidence.len())
    })));

    // ── Phase 2: LLM position formation (no function calling) ──

    let system_prompt = build_agent_system_prompt(agent, topic, round, previous_turns, all_agents, user_injection, gap_research, briefing_summary, mode, mode_input);

    let user_content = if research_summary.is_empty() {
        format!(
            "Based on your knowledge and perspective, state your position on: \"{}\"\n\n\
             Note: No relevant data was found in the knowledge graph or web search for this topic.",
            topic
        )
    } else {
        format!(
            "Here is the research gathered for the topic \"{}\":\n\n{}\n\n\
             Based on this evidence and your perspective, state your position. \
             Cite specific evidence from the research above to support your arguments.",
            topic, research_summary
        )
    };

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_content}
        ],
        "temperature": (0.5 + agent.rigor_level * 0.3).min(1.0),
        "max_tokens": 2048
    });

    let response = call_llm(state, request).await?;
    let content = extract_content(&response).unwrap_or_else(|| "No position stated.".into());
    let meta = parse_turn_metadata(&content, all_agents);
    let position = strip_metadata_lines(&content);

    Ok(DebateTurn {
        agent_id: agent.id.clone(),
        position,
        evidence: all_evidence,
        confidence: meta.confidence,
        tools_used: all_tool_invocations,
        agrees_with: meta.agrees_with,
        disagrees_with: meta.disagrees_with,
        position_shift: meta.position_shift,
        concessions: meta.concessions,
    })
}
