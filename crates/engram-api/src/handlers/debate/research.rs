/// Research engine for the debate panel.
/// - Starter plate: topic decomposition + multi-source fact gathering before Round 1
/// - Gap-closing: targeted research between rounds with dedup
/// - Moderator: fact-check claims against engram confidence

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content, parse_json_from_llm};

// ── Starter Plate ───────────────────────────────────────────────────────

/// Build the starter plate briefing: decompose topic, gather facts, ingest, assemble.
pub async fn build_starter_plate(
    state: &AppState,
    topic: &str,
) -> Briefing {
    // Step 1: Decompose topic into factual sub-questions
    let questions = decompose_topic(state, topic).await;

    // Step 2: Gather facts for each question (graph + web + ingest)
    let mut all_facts = Vec::new();
    let mut total_stored = 0u32;
    let mut total_relations = 0u32;

    // First: search the exact topic (highest priority)
    let exact_facts = gather_facts_for_question(state, topic, topic).await;
    all_facts.extend(exact_facts);

    // Then: search each decomposed question
    for question in &questions {
        let facts = gather_facts_for_question(state, question, topic).await;
        all_facts.extend(facts);
    }

    // Step 3: Ingest all web findings into the graph via full pipeline
    let web_content: String = all_facts.iter()
        .filter(|f| f.source == "web")
        .map(|f| f.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");

    if !web_content.is_empty() {
        let (fs, rc) = run_ingest(state, &format!("briefing: {}", topic), &web_content).await;
        total_stored += fs;
        total_relations += rc;
    }

    // Step 4: Re-search the graph now that it's enriched (finds newly ingested data)
    let enriched_facts = gather_graph_facts(state, topic).await;
    for ef in enriched_facts {
        if !all_facts.iter().any(|f| f.content == ef.content) {
            all_facts.push(ef);
        }
    }

    // Step 5: Build summary text for agents
    let summary = build_briefing_summary(&all_facts, topic);

    Briefing {
        questions,
        facts: all_facts,
        facts_stored: total_stored,
        relations_created: total_relations,
        summary,
    }
}

/// Decompose a topic into 5-8 specific factual questions using LLM.
async fn decompose_topic(state: &AppState, topic: &str) -> Vec<String> {
    let prompt = format!(
        r#"Decompose this question into 5-8 specific factual sub-questions that need to be answered with data:

"{}"

Rules:
- Each question should be answerable with a specific fact, number, or data point
- Include quantitative questions (how much, how many, what percentage)
- Include the key entities and their relationships
- Include current state and historical context
- Be specific enough for a web search to find real answers

Return ONLY a JSON array of questions:
["question 1", "question 2", ...]"#,
        topic
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.3,
        "max_tokens": 512
    });

    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                if let Ok(parsed) = parse_json_from_llm(&content) {
                    if let Some(arr) = parsed.as_array() {
                        return arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .take(8)
                            .collect();
                    }
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

/// Gather facts for a specific question from graph + web.
async fn gather_facts_for_question(
    state: &AppState,
    question: &str,
    _topic: &str,
) -> Vec<BriefingFact> {
    let mut facts = Vec::new();

    // Graph search
    {
        let g = match state.graph.read() {
            Ok(g) => g,
            Err(_) => return facts,
        };
        if let Ok(results) = g.search_text(question, 5) {
            for r in &results {
                if r.confidence >= 0.3 {
                    // Also get edges from this node
                    let mut content = format!("{} (confidence: {:.2})", r.label, r.confidence);
                    if let Ok(edges) = g.edges_from(&r.label) {
                        for e in edges.iter().take(3) {
                            content.push_str(&format!("\n  -> {} -> {} ({:.2})", e.relationship, e.to, e.confidence));
                        }
                    }
                    facts.push(BriefingFact {
                        question: question.to_string(),
                        source: "graph".into(),
                        content,
                        confidence: r.confidence,
                    });
                }
            }
        }
    }

    // Web search
    let web_results = execute_web_search(state, question).await;
    if !web_results.is_empty() {
        for line in web_results.lines().take(5) {
            let line = line.trim().trim_start_matches("- ");
            if !line.is_empty() && line.len() > 20 {
                facts.push(BriefingFact {
                    question: question.to_string(),
                    source: "web".into(),
                    content: line.to_string(),
                    confidence: 0.4,
                });
            }
        }
    }

    facts
}

/// Search graph for facts related to the topic (used after ingest to find enriched data).
async fn gather_graph_facts(state: &AppState, topic: &str) -> Vec<BriefingFact> {
    let mut facts = Vec::new();
    let g = match state.graph.read() {
        Ok(g) => g,
        Err(_) => return facts,
    };

    // Search for key terms in the topic
    let terms: Vec<&str> = topic.split_whitespace()
        .filter(|w| w.len() > 3)
        .take(5)
        .collect();

    for term in terms {
        if let Ok(results) = g.search_text(term, 3) {
            for r in &results {
                let mut content = format!("{} (confidence: {:.2})", r.label, r.confidence);
                if let Ok(edges) = g.edges_from(&r.label) {
                    for e in edges.iter().take(5) {
                        content.push_str(&format!("\n  -> {} -> {} ({:.2})", e.relationship, e.to, e.confidence));
                    }
                }
                facts.push(BriefingFact {
                    question: topic.to_string(),
                    source: "graph".into(),
                    content,
                    confidence: r.confidence,
                });
            }
        }
    }

    facts
}

/// Build a text summary from gathered facts for inclusion in agent prompts.
fn build_briefing_summary(facts: &[BriefingFact], topic: &str) -> String {
    let mut summary = format!("=== BRIEFING: {} ===\n\n", topic);

    // Group by question
    let mut seen_questions = Vec::new();
    for fact in facts {
        if !seen_questions.contains(&fact.question) {
            seen_questions.push(fact.question.clone());
        }
    }

    for question in &seen_questions {
        summary.push_str(&format!("Q: {}\n", question));
        let relevant: Vec<&BriefingFact> = facts.iter()
            .filter(|f| f.question == *question)
            .take(5)
            .collect();
        for f in relevant {
            summary.push_str(&format!("  [{}] {}\n", f.source, f.content));
        }
        summary.push('\n');
    }

    summary
}

// ── Gap Detection & Closing ─────────────────────────────────────────────

/// Detect knowledge gaps from a round, excluding already-researched queries.
pub async fn detect_gaps(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
    already_researched: &[String],
) -> Vec<String> {
    let mut summary = format!("Topic: \"{}\"\n\n", topic);
    for turn in &round.turns {
        let name = agents.iter().find(|a| a.id == turn.agent_id)
            .map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
        summary.push_str(&format!(
            "{} (confidence: {:.0}%): {}\n\n",
            name, turn.confidence * 100.0, super::agents::truncate(&turn.position, 500)
        ));
    }

    let already_list = if already_researched.is_empty() {
        String::new()
    } else {
        format!("\n\nALREADY RESEARCHED (do NOT repeat these):\n{}\n",
            already_researched.iter().map(|q| format!("- {}", q)).collect::<Vec<_>>().join("\n"))
    };

    let prompt = format!(
        r#"You are a research analyst. Identify 2-4 SPECIFIC factual gaps that would improve the next round.
{}{}
Return ONLY a JSON array of NEW search queries not already researched:
["specific query 1", "specific query 2"]

Rules:
- Each query must be specific with context (e.g., "Iran oil export volume barrels per day 2025" not just "oil exports")
- Do NOT repeat anything from the already-researched list
- Focus on numerical data and verifiable facts that agents disagreed on"#,
        summary, already_list
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.3,
        "max_tokens": 512
    });

    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                if let Ok(parsed) = parse_json_from_llm(&content) {
                    if let Some(arr) = parsed.as_array() {
                        return arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .filter(|q| !already_researched.iter().any(|r| r == q))
                            .take(4)
                            .collect();
                    }
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

/// Close gaps via web search + ingest pipeline. Returns results and updates researched list.
pub async fn close_gaps(
    state: &AppState,
    gaps: &[String],
    topic: &str,
) -> Vec<GapResearch> {
    let mut results = Vec::new();

    for gap_query in gaps {
        let mut findings = Vec::new();
        let mut facts_stored = 0u32;
        let mut relations_created = 0u32;
        let mut entities_stored = Vec::new();

        // Graph search (might already have data from briefing)
        {
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => continue,
            };
            if let Ok(search_results) = g.search_text(gap_query, 3) {
                for r in &search_results {
                    findings.push(format!("[graph] {} (confidence: {:.2})", r.label, r.confidence));
                }
            }
        }

        // Web search
        let web_text = execute_web_search(state, gap_query).await;
        if !web_text.is_empty() {
            for line in web_text.lines().take(5) {
                let line = line.trim().trim_start_matches("- ");
                if !line.is_empty() && line.len() > 20 {
                    findings.push(format!("[web] {}", line));
                }
            }
        }

        // Ingest web findings via full pipeline
        let ingested = if !web_text.is_empty() {
            let (fs, rc) = run_ingest(state, &format!("gap: {}", gap_query), &web_text).await;
            facts_stored = fs;
            relations_created = rc;
            if fs > 0 || rc > 0 {
                entities_stored.push(format!("{} facts, {} relations", fs, rc));
            }
            fs > 0 || rc > 0
        } else {
            false
        };

        results.push(GapResearch {
            gap_query: gap_query.clone(),
            source: "round-analysis".into(),
            findings,
            ingested,
            entities_stored,
            facts_stored,
            relations_created,
        });
    }

    results
}

// ── Moderator ───────────────────────────────────────────────────────────

/// Fact-check agent claims against engram confidence scores.
pub async fn moderate_round(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
) -> Vec<ModeratorCheck> {
    // Build a summary of all claims made in this round
    let mut claims_summary = String::new();
    for turn in &round.turns {
        let name = agents.iter().find(|a| a.id == turn.agent_id)
            .map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
        claims_summary.push_str(&format!(
            "Agent {} said:\n{}\n\n",
            name, super::agents::truncate(&turn.position, 400)
        ));
    }

    // Ask LLM to extract key factual claims
    let prompt = format!(
        r#"Extract the 3-5 most important FACTUAL CLAIMS (not opinions) from these debate positions on "{}":

{}

Return ONLY a JSON array of claims with the agent who made them:
[{{"agent_id": "agent-1", "claim": "specific factual claim to verify"}}]"#,
        topic, claims_summary
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.2,
        "max_tokens": 512
    });

    let claims: Vec<(String, String)> = match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                if let Ok(parsed) = parse_json_from_llm(&content) {
                    if let Some(arr) = parsed.as_array() {
                        arr.iter().filter_map(|v| {
                            let aid = v.get("agent_id")?.as_str()?.to_string();
                            let claim = v.get("claim")?.as_str()?.to_string();
                            Some((aid, claim))
                        }).collect()
                    } else { Vec::new() }
                } else { Vec::new() }
            } else { Vec::new() }
        }
        Err(_) => Vec::new(),
    };

    // Check each claim against engram
    let mut checks = Vec::new();
    let g = match state.graph.read() {
        Ok(g) => g,
        Err(_) => return checks,
    };

    for (agent_id, claim) in &claims {
        // Search graph for evidence related to the claim
        let search_results = g.search_text(claim, 3).unwrap_or_default();

        let (verdict, confidence, explanation) = if search_results.is_empty() {
            (ModeratorVerdict::Unsupported, None,
             format!("No evidence found in engram for: \"{}\"", claim))
        } else {
            let best = &search_results[0];
            if best.confidence >= 0.7 {
                (ModeratorVerdict::Supported, Some(best.confidence),
                 format!("Supported by: {} (confidence: {:.2})", best.label, best.confidence))
            } else if best.confidence < 0.3 {
                (ModeratorVerdict::LowConfidence, Some(best.confidence),
                 format!("Low-confidence evidence: {} ({:.2}). Treat with caution.", best.label, best.confidence))
            } else {
                (ModeratorVerdict::Supported, Some(best.confidence),
                 format!("Partially supported: {} (confidence: {:.2})", best.label, best.confidence))
            }
        };

        checks.push(ModeratorCheck {
            agent_id: agent_id.clone(),
            claim: claim.clone(),
            verdict,
            engram_confidence: confidence,
            explanation,
        });
    }

    checks
}

// ── Shared helpers ──────────────────────────────────────────────────────

/// Execute a web search via configured provider.
pub async fn execute_web_search(state: &AppState, query: &str) -> String {
    let (provider, api_key, search_url) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.web_search_provider.clone().unwrap_or_else(|| "searxng".to_string()),
            cfg.web_search_api_key.clone().unwrap_or_default(),
            cfg.web_search_url.clone(),
        )
    };

    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut results = Vec::new();

    match provider.as_str() {
        "brave" => {
            let url = format!("https://api.search.brave.com/res/v1/web/search?q={}", urlencoding::encode(query));
            if let Ok(resp) = client.get(&url).header("Accept", "application/json").header("X-Subscription-Token", &api_key).send().await {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(web) = data.pointer("/web/results").and_then(|r| r.as_array()) {
                        for r in web.iter().take(8) {
                            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("");
                            let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            if !title.is_empty() {
                                results.push(format!("- {}: {}", title, snippet));
                            }
                        }
                    }
                }
            }
        }
        _ => {
            let base = search_url.unwrap_or_else(|| std::env::var("ENGRAM_SEARXNG_URL").unwrap_or_else(|_| "http://192.168.178.26:8080".into()));
            let url = format!("{}/search?q={}&format=json&engines=google,duckduckgo,bing&language=en", base.trim_end_matches('/'), urlencoding::encode(query));
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(res) = data.get("results").and_then(|r| r.as_array()) {
                        for r in res.iter().take(8) {
                            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("");
                            let snippet = r.get("content").and_then(|c| c.as_str()).unwrap_or("");
                            if !title.is_empty() {
                                results.push(format!("- {}: {}", title, snippet));
                            }
                        }
                    }
                }
            }
        }
    }

    results.join("\n")
}

/// Run the full ingest pipeline on content. Returns (facts_stored, relations_created).
#[cfg(feature = "ingest")]
async fn run_ingest(state: &AppState, source_name: &str, content: &str) -> (u32, u32) {
    use engram_ingest::types::{RawItem, Content};

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.kb_endpoints.clone(), cfg.ner_model.clone(), cfg.rel_model.clone(),
         cfg.relation_templates.clone(), cfg.rel_threshold, cfg.coreference_enabled)
    };
    let llm_config = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.llm_endpoint.clone(), cfg.llm_model.clone())
    };

    let graph = state.graph.clone();
    let doc_store = state.doc_store.clone();
    let cached_ner = state.cached_ner.clone();
    let cached_rel = state.cached_rel.clone();
    let content_owned = content.to_string();
    let source_owned = source_name.to_string();
    let dirty = state.dirty.clone();

    let result = tokio::task::spawn_blocking(move || {
        let config = engram_ingest::PipelineConfig::default();
        let mut pipeline = super::super::ingest::build_pipeline(
            graph, config, kb_endpoints, ner_model, rel_model,
            relation_templates, rel_threshold, coreference_enabled,
            cached_ner, cached_rel,
        );
        pipeline.set_doc_store(doc_store);
        if let (Some(ep), Some(m)) = (llm_config.0.as_ref(), llm_config.1.as_ref()) {
            pipeline.set_llm(ep.clone(), m.clone());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs() as i64;

        let items = vec![RawItem {
            content: Content::Text(content_owned),
            source_url: None,
            source_name: source_owned,
            fetched_at: now,
            metadata: Default::default(),
        }];

        match pipeline.execute(items) {
            Ok(r) => {
                if r.facts_stored > 0 || r.relations_created > 0 {
                    dirty.store(true, std::sync::atomic::Ordering::Release);
                }
                (r.facts_stored, r.relations_created)
            }
            Err(e) => {
                tracing::warn!("debate ingest failed: {}", e);
                (0, 0)
            }
        }
    }).await;

    result.unwrap_or((0, 0))
}

#[cfg(not(feature = "ingest"))]
async fn run_ingest(_state: &AppState, _source: &str, _content: &str) -> (u32, u32) {
    (0, 0)
}
