/// Research engine for the debate panel.
/// - Starter plate: deep topic decomposition + multi-source fact gathering
/// - Persistent gap-closing: multi-attempt with relevance filtering + query reformulation
/// - Moderator: fact-check claims against engram confidence

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content, parse_json_from_llm};

// ── Starter Plate ───────────────────────────────────────────────────────

/// Build the starter plate briefing: decompose topic, gather facts, ingest, assemble.
pub async fn build_starter_plate(state: &AppState, topic: &str) -> Briefing {
    let questions = decompose_topic(state, topic).await;
    let mut all_facts = Vec::new();
    let mut total_stored = 0u32;
    let mut total_relations = 0u32;

    // Search the exact topic first (highest priority)
    let exact_facts = gather_facts_for_question(state, topic, topic).await;
    all_facts.extend(exact_facts);

    // Search each decomposed question
    for question in &questions {
        let facts = gather_facts_for_question(state, question, topic).await;
        all_facts.extend(facts);
    }

    // Ingest relevant web findings via full pipeline
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

    // Re-search the enriched graph
    let enriched = gather_graph_facts(state, topic).await;
    for ef in enriched {
        if !all_facts.iter().any(|f| f.content == ef.content) {
            all_facts.push(ef);
        }
    }

    let summary = build_briefing_summary(&all_facts, topic);

    Briefing {
        questions,
        facts: all_facts,
        facts_stored: total_stored,
        relations_created: total_relations,
        summary,
    }
}

/// Decompose a topic into specific factual sub-questions covering ALL dimensions.
async fn decompose_topic(state: &AppState, topic: &str) -> Vec<String> {
    let prompt = format!(
        r##"Decompose this question into 6-10 specific factual sub-questions:

"{}"

You MUST cover ALL of these dimensions:
1. KEY ENTITIES: Identify ALL parties/entities affected (not just the obvious one). For geographic events, include all countries, organizations, and supply chains impacted.
2. QUANTITATIVE DATA: How much, how many, what volume, what percentage, what price?
3. HISTORICAL PRECEDENTS: What happened in similar past events? What were the actual numbers?
4. CURRENT STATE: What is the current situation right now? Current prices, volumes, inventories?
5. ALTERNATIVES: What alternatives exist? What capacity do they have?
6. TIMELINE: How quickly would impacts materialize? Short-term vs long-term?
7. SECOND-ORDER EFFECTS: Who else is affected indirectly?

Example for "What happens to oil prices if Iran closes the Strait of Hormuz?":
- "How many million barrels per day of oil transit through the Strait of Hormuz from ALL countries (Saudi Arabia, UAE, Kuwait, Iraq, Qatar, Iran)?"
- "What is the current global daily oil consumption in million barrels per day?"
- "What percentage of global oil supply passes through the Strait of Hormuz?"
- "What alternative oil shipping routes exist (Suez, pipelines) and what is their spare capacity?"
- "What happened to oil prices during the 1980 Iran-Iraq War tanker war in the Strait of Hormuz?"
- "What are current global strategic petroleum reserve levels (US SPR, IEA member reserves)?"
- "What is the current price of Brent crude and WTI crude oil?"
- "What is OPEC's current spare production capacity in million barrels per day?"

Return ONLY a JSON array:
["question 1", "question 2", ...]"##,
        topic
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.3,
        "max_tokens": 1024
    });

    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                if let Ok(parsed) = parse_json_from_llm(&content) {
                    if let Some(arr) = parsed.as_array() {
                        return arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .take(10)
                            .collect();
                    }
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

/// Gather facts for a question from graph + web.
async fn gather_facts_for_question(state: &AppState, question: &str, _topic: &str) -> Vec<BriefingFact> {
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

/// Search graph for topic-related facts (used after ingest to find enriched data).
async fn gather_graph_facts(state: &AppState, topic: &str) -> Vec<BriefingFact> {
    let mut facts = Vec::new();
    let g = match state.graph.read() {
        Ok(g) => g,
        Err(_) => return facts,
    };

    let terms: Vec<&str> = topic.split_whitespace().filter(|w| w.len() > 3).take(5).collect();
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

fn build_briefing_summary(facts: &[BriefingFact], topic: &str) -> String {
    let mut summary = format!("=== BRIEFING: {} ===\n\n", topic);
    let mut seen_questions = Vec::new();
    for fact in facts {
        if !seen_questions.contains(&fact.question) {
            seen_questions.push(fact.question.clone());
        }
    }
    for question in &seen_questions {
        summary.push_str(&format!("Q: {}\n", question));
        let relevant: Vec<&BriefingFact> = facts.iter().filter(|f| f.question == *question).take(5).collect();
        for f in relevant {
            summary.push_str(&format!("  [{}] {}\n", f.source, f.content));
        }
        summary.push('\n');
    }
    summary
}

// ── Persistent Gap-Closing ──────────────────────────────────────────────

/// Detect knowledge gaps from a round, excluding already-closed gaps.
pub async fn detect_gaps(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
    closed_gaps: &[String],
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

    let closed_list = if closed_gaps.is_empty() {
        String::new()
    } else {
        format!("\n\nALREADY CLOSED (these gaps have been answered, do NOT repeat):\n{}\n",
            closed_gaps.iter().map(|q| format!("- {}", q)).collect::<Vec<_>>().join("\n"))
    };

    let prompt = format!(
        r#"Identify 2-4 SPECIFIC factual gaps from this debate round that need DATA to resolve:
{}{}
Rules:
- Each gap must be a specific, searchable query with enough context to find real data
- Include units and timeframes (e.g., "Iran oil exports million barrels per day 2025")
- Focus on facts that agents disagreed on or cited uncertainty about
- Do NOT repeat already-closed gaps
- Prefer quantitative queries that will return numbers, not opinions

Return ONLY a JSON array: ["query 1", "query 2"]"#,
        summary, closed_list
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
                            .filter(|q| !closed_gaps.iter().any(|c| c == q))
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

/// Persistently close gaps: multi-attempt per gap with relevance filtering and query reformulation.
/// Returns results. Gaps that couldn't be closed are marked with ingested=false.
pub async fn close_gaps(state: &AppState, gaps: &[String], topic: &str) -> Vec<GapResearch> {
    let mut results = Vec::new();

    for gap_query in gaps {
        let result = close_single_gap(state, gap_query, topic, 3).await;
        results.push(result);
    }

    results
}

/// Try to close a single gap with up to max_attempts, reformulating the query each time.
async fn close_single_gap(state: &AppState, original_query: &str, topic: &str, max_attempts: usize) -> GapResearch {
    let mut current_query = original_query.to_string();
    let mut all_findings = Vec::new();
    let mut facts_stored = 0u32;
    let mut relations_created = 0u32;
    let mut entities_stored = Vec::new();
    let mut ingested = false;

    for attempt in 0..max_attempts {
        // 1. Search graph
        {
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => break,
            };
            if let Ok(results) = g.search_text(&current_query, 5) {
                for r in &results {
                    let finding = format!("[graph] {} (confidence: {:.2})", r.label, r.confidence);
                    if !all_findings.contains(&finding) {
                        all_findings.push(finding);
                    }
                }
            }
        }

        // 2. Web search
        let web_text = execute_web_search(state, &current_query).await;

        // 3. Check relevance of web results BEFORE ingesting
        let relevant_text = if !web_text.is_empty() {
            filter_relevant_results(state, &web_text, original_query, topic).await
        } else {
            String::new()
        };

        if !relevant_text.is_empty() {
            // Add relevant findings
            for line in relevant_text.lines().take(5) {
                let line = line.trim().trim_start_matches("- ");
                if !line.is_empty() && line.len() > 20 {
                    let finding = format!("[web] {}", line);
                    if !all_findings.contains(&finding) {
                        all_findings.push(finding);
                    }
                }
            }

            // Ingest relevant content via pipeline
            let (fs, rc) = run_ingest(state, &format!("gap: {}", original_query), &relevant_text).await;
            facts_stored += fs;
            relations_created += rc;
            if fs > 0 || rc > 0 {
                entities_stored.push(format!("attempt {}: {} facts, {} relations", attempt + 1, fs, rc));
                ingested = true;
                break; // Gap closed successfully
            }
        }

        // 4. If we didn't find relevant results, reformulate the query
        if attempt + 1 < max_attempts {
            let reformulated = reformulate_query(state, &current_query, original_query, topic).await;
            if !reformulated.is_empty() && reformulated != current_query {
                current_query = reformulated;
            } else {
                break; // Can't reformulate, give up
            }
        }
    }

    GapResearch {
        gap_query: original_query.to_string(),
        source: "persistent-research".into(),
        findings: all_findings,
        ingested,
        entities_stored,
        facts_stored,
        relations_created,
    }
}

/// Filter web search results for relevance to the gap question.
/// Returns only the lines that are actually relevant, discards garbage.
async fn filter_relevant_results(
    state: &AppState,
    web_text: &str,
    gap_query: &str,
    topic: &str,
) -> String {
    let prompt = format!(
        r#"Filter these search results. Keep ONLY results that contain factual information relevant to:
Gap question: "{}"
Topic: "{}"

Search results:
{}

Return ONLY the relevant result lines, one per line. Remove any results about:
- Language translation, grammar, or word definitions
- Unrelated products, apps, or services
- Generic educational content not related to the topic

If NO results are relevant, return the single word: NONE"#,
        gap_query, topic, web_text
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.1,
        "max_tokens": 512
    });

    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                let trimmed = content.trim();
                if trimmed == "NONE" || trimmed.is_empty() {
                    String::new()
                } else {
                    trimmed.to_string()
                }
            } else {
                String::new()
            }
        }
        Err(_) => {
            // Fallback: return original if LLM fails (better than nothing)
            web_text.to_string()
        }
    }
}

/// Reformulate a failed search query to try a different angle.
async fn reformulate_query(
    state: &AppState,
    failed_query: &str,
    original_gap: &str,
    topic: &str,
) -> String {
    let prompt = format!(
        r#"The search query "{}" returned no relevant results for the topic "{}".
The original gap we're trying to close is: "{}"

Suggest ONE better search query that would find this specific data. Try:
- Different keywords or phrasing
- Adding specific data sources (IEA, OPEC, EIA, World Bank)
- Including specific years or units
- Being more or less specific

Return ONLY the new query string, nothing else."#,
        failed_query, topic, original_gap
    );

    let request = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.5,
        "max_tokens": 100
    });

    match call_llm(state, request).await {
        Ok(response) => {
            extract_content(&response)
                .map(|c| c.trim().trim_matches('"').to_string())
                .unwrap_or_default()
        }
        Err(_) => String::new(),
    }
}

// ── Moderator ───────────────────────────────────────────────────────────

/// Fact-check agent claims against engram confidence scores.
pub async fn moderate_round(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
) -> Vec<ModeratorCheck> {
    let mut claims_summary = String::new();
    for turn in &round.turns {
        let name = agents.iter().find(|a| a.id == turn.agent_id)
            .map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
        claims_summary.push_str(&format!("Agent {} ({}) said:\n{}\n\n",
            name, turn.agent_id, super::agents::truncate(&turn.position, 400)));
    }

    let prompt = format!(
        r#"Extract the 3-5 most important FACTUAL CLAIMS (not opinions) from these positions on "{}":

{}

Return ONLY a JSON array:
[{{"agent_id": "agent-1", "claim": "specific factual claim"}}]"#,
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

    let mut checks = Vec::new();
    let g = match state.graph.read() {
        Ok(g) => g,
        Err(_) => return checks,
    };

    for (agent_id, claim) in &claims {
        let results = g.search_text(claim, 3).unwrap_or_default();
        let (verdict, confidence, explanation) = if results.is_empty() {
            (ModeratorVerdict::Unsupported, None,
             format!("No evidence in engram for: \"{}\"", claim))
        } else {
            let best = &results[0];
            if best.confidence >= 0.7 {
                (ModeratorVerdict::Supported, Some(best.confidence),
                 format!("Supported: {} ({:.2})", best.label, best.confidence))
            } else if best.confidence < 0.3 {
                (ModeratorVerdict::LowConfidence, Some(best.confidence),
                 format!("Low-confidence: {} ({:.2})", best.label, best.confidence))
            } else {
                (ModeratorVerdict::Supported, Some(best.confidence),
                 format!("Partially supported: {} ({:.2})", best.label, best.confidence))
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
            let url = format!("{}/search?q={}&format=json&engines=google,duckduckgo,bing&language=en",
                base.trim_end_matches('/'), urlencoding::encode(query));
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

/// Run the full ingest pipeline on content.
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
