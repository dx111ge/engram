/// Research engine for the debate panel.
/// - Starter plate: deep topic decomposition + multi-source fact gathering
/// - Persistent gap-closing: multi-attempt with relevance filtering + query reformulation
/// - Moderator: fact-check claims against engram confidence

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content, parse_json_from_llm, short_output_budget, medium_output_budget};

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

Example for "Should our company adopt a 4-day work week?":
- "What companies have implemented a 4-day work week and what were their productivity results?"
- "What percentage change in employee retention do companies see after switching to a 4-day week?"
- "What are the reported effects on revenue and output for firms that tried a 4-day schedule?"
- "What does the latest research say about the relationship between working hours and productivity?"
- "What industries have the highest and lowest success rates with reduced work weeks?"
- "What are the implementation costs (scheduling, overtime, hiring) of a 4-day work week?"
- "How do employees rate job satisfaction before vs after a 4-day week transition?"
- "What legal or contractual barriers exist for adopting a 4-day work week?"

Return ONLY a JSON array:
["question 1", "question 2", ...]"##,
        topic
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a research analyst. Respond ONLY with valid JSON, no explanation."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": medium_output_budget(state),
        "think": false
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
- Include units and timeframes (e.g., "global EV sales by country million units 2025")
- Focus on facts that agents disagreed on or cited uncertainty about
- Do NOT repeat already-closed gaps
- Prefer quantitative queries that will return numbers, not opinions

Return ONLY a JSON array: ["query 1", "query 2"]"#,
        summary, closed_list
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a research analyst. Respond ONLY with valid JSON, no explanation."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": medium_output_budget(state),
        "think": false
    });

    match call_llm(state, request).await {
        Ok(response) => {
            let content = extract_content(&response);
            let content_str = content.unwrap_or_default();
            if content_str.is_empty() {
                eprintln!("[debate] detect_gaps: empty LLM content, raw response: {}", &response.to_string()[..response.to_string().len().min(500)]);
                return Vec::new();
            }
            match parse_json_from_llm(&content_str) {
                Ok(parsed) => {
                    if let Some(arr) = parsed.as_array() {
                        let gaps: Vec<String> = arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .filter(|q| !closed_gaps.iter().any(|c| c == q))
                            .take(4)
                            .collect();
                        eprintln!("[debate] detect_gaps: found {} gaps", gaps.len());
                        return gaps;
                    }
                    eprintln!("[debate] detect_gaps: parsed JSON is not an array: {}", parsed);
                }
                Err(e) => {
                    eprintln!("[debate] detect_gaps: JSON parse failed: {} | raw: {}", e, &content_str[..content_str.len().min(300)]);
                }
            }
            Vec::new()
        }
        Err(e) => {
            eprintln!("[debate] detect_gaps: LLM call failed: {}", e);
            Vec::new()
        }
    }
}

/// Close gaps sequentially (Ollama serializes concurrent LLM calls anyway).
/// Each gap tries URLs one by one, collects 2 answers, cross-validates, then ingests.
pub async fn close_gaps(state: &AppState, gaps: &[String], topic: &str) -> Vec<GapResearch> {
    let mut results = Vec::new();
    for (i, gap_query) in gaps.iter().enumerate() {
        eprintln!("[debate] gap {}/{}: \"{}\"", i + 1, gaps.len(), &gap_query[..gap_query.len().min(60)]);
        let result = close_single_gap(state, gap_query, topic).await;
        results.push(result);
    }
    results
}

/// Close a single gap with a timeout safety net.
pub async fn close_single_gap_with_timeout(state: &AppState, query: &str, topic: &str, timeout_secs: u64) -> GapResearch {
    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        close_single_gap(state, query, topic),
    ).await {
        Ok(result) => result,
        Err(_) => {
            eprintln!("[debate] gap TIMED OUT ({}s): \"{}\"", timeout_secs, &query[..query.len().min(60)]);
            empty_gap(query)
        }
    }
}

/// An answer extracted from a single article.
struct GapAnswer {
    text: String,
    source_title: String,
    source_url: String,
}

/// Close a single gap:
/// 1. Check engram graph (maybe already answered)
/// 2. SPARQL knowledge bases (configured endpoints -- Wikidata etc.)
/// 3. Wikipedia API (free, fast, trusted, no rate limits)
/// 4. SearxNG web search -> try URLs one by one
/// 5. Collect 2 answers from different sources
/// 6. Cross-validate/combine the 2 answers via LLM
/// 7. Ingest the validated answer
async fn close_single_gap(state: &AppState, original_query: &str, _topic: &str) -> GapResearch {
    let mut all_findings = Vec::new();
    let mut facts_stored = 0u32;
    let mut relations_created = 0u32;
    let mut entities_stored = Vec::new();
    let mut ingested = false;

    let t0 = std::time::Instant::now();

    // 1. Check graph first
    {
        let g = match state.graph.read() {
            Ok(g) => g,
            Err(_) => return empty_gap(original_query),
        };
        if let Ok(results) = g.search_text(original_query, 5) {
            for r in &results {
                let finding = format!("[graph] {} (confidence: {:.2})", r.label, r.confidence);
                if !all_findings.contains(&finding) {
                    all_findings.push(finding);
                }
            }
        }
    }

    let mut answers: Vec<GapAnswer> = Vec::new();

    // 2. SPARQL knowledge bases (most precise for entity facts)
    if let Some(sparql_answer) = query_sparql_endpoints(state, original_query).await {
        all_findings.push(format!("[sparql] {}: {}", sparql_answer.source_title, &sparql_answer.text[..sparql_answer.text.len().min(100)]));
        eprintln!("[debate] gap: sparql answer from \"{}\" ({} chars) in {:.1}s",
            sparql_answer.source_title, sparql_answer.text.len(), t0.elapsed().as_secs_f32());
        answers.push(sparql_answer);
    } else {
        eprintln!("[debate] gap: no sparql answer in {:.1}s", t0.elapsed().as_secs_f32());
    }

    // 3. Wikipedia API (free, fast, trusted, no rate limits)
    if answers.len() < 2 {
        if let Some(wiki_answer) = search_wikipedia(state, original_query).await {
            all_findings.push(format!("[wikipedia] {}: {}", wiki_answer.source_title, &wiki_answer.text[..wiki_answer.text.len().min(100)]));
            eprintln!("[debate] gap: wikipedia answer from \"{}\" ({} chars) in {:.1}s",
                wiki_answer.source_title, wiki_answer.text.len(), t0.elapsed().as_secs_f32());
            answers.push(wiki_answer);
        } else {
            eprintln!("[debate] gap: no wikipedia answer in {:.1}s", t0.elapsed().as_secs_f32());
        }
    }

    // 3. Web search for second source (or first if Wikipedia had nothing)
    let web_results = execute_web_search_structured(state, original_query).await;
    eprintln!("[debate] gap: web_results={} in {:.1}s", web_results.len(), t0.elapsed().as_secs_f32());

    for r in web_results.iter().take(5) {
        let finding = format!("[web] {}: {}", r.title, r.snippet);
        if !all_findings.contains(&finding) {
            all_findings.push(finding);
        }
    }

    // 4. Try URLs one by one, collect answers until we have 2 total
    let mut urls_tried = 0u32;
    let mut urls_fetched = 0u32;

    for result in web_results.iter().take(8) {
        if answers.len() >= 2 { break; }
        if result.url.is_empty() { continue; }
        // Skip wikipedia URLs -- we already checked Wikipedia directly
        if result.url.contains("wikipedia.org") { continue; }
        urls_tried += 1;

        // Fetch article
        let content = match fetch_article_content(&state.http_client, &result.url).await {
            Some(c) => c,
            None => continue,
        };
        urls_fetched += 1;
        let truncated = if content.len() > 4000 { &content[..4000] } else { &content };

        // LLM reading comprehension on this single article
        let answer = answer_gap_from_article(state, original_query, &result.url, &result.title, truncated).await;
        if let Some(a) = answer {
            eprintln!("[debate] gap: answer {}/2 from \"{}\" ({} chars)",
                answers.len() + 1, &result.title[..result.title.len().min(40)], a.text.len());
            answers.push(a);
        }
    }

    eprintln!("[debate] gap: tried={} fetched={} answers={} in {:.1}s",
        urls_tried, urls_fetched, answers.len(), t0.elapsed().as_secs_f32());

    if answers.is_empty() {
        return GapResearch {
            gap_query: original_query.to_string(), source: "no-answer".into(),
            findings: all_findings, ingested: false, entities_stored: Vec::new(),
            facts_stored: 0, relations_created: 0,
        };
    }

    // 4. Cross-validate and combine
    let final_answer = if answers.len() >= 2 {
        cross_validate_answers(state, original_query, &answers).await
    } else {
        // Single answer -- use it but note it's unverified
        eprintln!("[debate] gap: only 1 answer, using without cross-validation");
        answers.remove(0).text
    };

    all_findings.push(format!("[answer] {}", &final_answer[..final_answer.len().min(200)]));

    // 5. Ingest the validated answer
    let source_label = if answers.is_empty() {
        "web".to_string()
    } else {
        // Already removed first for single-answer case; use the stored findings
        format!("gap: {}", &original_query[..original_query.len().min(60)])
    };
    let (fs, rc) = run_ingest(state, &source_label, &final_answer).await;
    facts_stored += fs;
    relations_created += rc;

    if fs > 0 || rc > 0 {
        entities_stored.push(format!("{} facts from {} sources", fs, urls_fetched));
        ingested = true;
        eprintln!("[debate] gap: RESOLVED -> {} facts, {} relations in {:.1}s",
            fs, rc, t0.elapsed().as_secs_f32());
    } else {
        // NER produced nothing -- store directly as fact node
        let prov = engram_core::graph::Provenance::user(&source_label);
        if let Ok(mut g) = state.graph.write() {
            let label = if final_answer.len() > 250 { &final_answer[..250] } else { &final_answer };
            if g.store_with_confidence(label, 0.55, &prov).is_ok() {
                state.dirty.store(true, std::sync::atomic::Ordering::Release);
                facts_stored += 1;
                ingested = true;
                entities_stored.push(format!("direct: {}", &final_answer[..final_answer.len().min(80)]));
                eprintln!("[debate] gap: stored as direct node in {:.1}s", t0.elapsed().as_secs_f32());
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

// ── SPARQL Knowledge Bases ─────────────────────────────────────────────

/// Query all configured SPARQL endpoints (Wikidata, DBpedia, custom) for structured facts.
/// Has the LLM generate a SPARQL query, executes it, formats results.
async fn query_sparql_endpoints(state: &AppState, gap_query: &str) -> Option<GapAnswer> {
    let endpoints = {
        let cfg = state.config.read().ok()?;
        cfg.kb_endpoints.clone().unwrap_or_default()
    };

    let enabled: Vec<_> = endpoints.iter().filter(|e| e.enabled).collect();
    if enabled.is_empty() {
        return None;
    }

    let client = &state.http_client;

    for ep in &enabled {
        // Step 1: LLM generates a SPARQL query for this endpoint
        let sparql = generate_sparql_query(state, gap_query, &ep.name).await;
        let sparql = match sparql {
            Some(q) if !q.is_empty() => q,
            _ => continue,
        };
        eprintln!("[sparql] generated query for {}:\n{}", ep.name, &sparql[..sparql.len().min(500)]);

        // Step 2: Execute SPARQL
        let mut req = client.get(&ep.url)
            .query(&[("query", sparql.as_str()), ("format", "json")])
            .header("Accept", "application/sparql-results+json")
            .header("User-Agent", "engram/1.1 (knowledge-graph)");

        // Add auth if configured
        if let Some(ref secret_key) = ep.auth_secret_key {
            // Resolve secret from secrets store
            let secret_value = {
                let secrets = state.secrets.read().ok()?;
                secrets.as_ref().and_then(|s| s.get(secret_key).map(|v| v.to_string()))
            };
            if let Some(val) = secret_value {
                match ep.auth_type.as_str() {
                    "bearer" => { req = req.header("Authorization", format!("Bearer {}", val)); }
                    "basic" => { req = req.header("Authorization", format!("Basic {}", val)); }
                    "api_key" => { req = req.header("X-API-Key", val); }
                    _ => {}
                }
            }
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[sparql] {} request failed: {}", ep.name, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            eprintln!("[sparql] {} returned HTTP {}", ep.name, resp.status());
            continue;
        }

        let resp_text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[sparql] {} response read failed: {}", ep.name, e);
                continue;
            }
        };

        // Cap response size -- huge responses mean the query was too broad
        if resp_text.len() > 100_000 {
            eprintln!("[sparql] {} response too large ({} bytes), query too broad", ep.name, resp_text.len());
            continue;
        }

        let data: serde_json::Value = match serde_json::from_str(&resp_text) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[sparql] {} JSON parse failed: {} | body: {}", ep.name, e, &resp_text[..resp_text.len().min(300)]);
                continue;
            }
        };

        // Step 3: Format SPARQL results into readable text
        let text = format_sparql_results(&data);
        if text.is_empty() {
            eprintln!("[sparql] {} returned 0 results", ep.name);
            continue;
        }

        eprintln!("[sparql] {} returned {} chars of data", ep.name, text.len());

        return Some(GapAnswer {
            text,
            source_title: format!("SPARQL: {}", ep.name),
            source_url: ep.url.clone(),
        });
    }

    None
}

/// Two-step SPARQL query generation:
/// 1. LLM extracts the entity name and property from the question
/// 2. Wikidata search API resolves the entity to a QID
/// 3. Build a correct SPARQL query with the verified QID
async fn generate_sparql_query(state: &AppState, gap_query: &str, endpoint_name: &str) -> Option<String> {
    // Step 1: LLM extracts entity + property from the question
    let prompt = format!(
        r#"Extract the main entity and the property being asked about from this question:

"{}"

Examples:
- "What is the population of Germany?" -> {{"entity": "Germany", "property": "population", "sparql_property": "wdt:P1082"}}
- "What is the GDP of Japan?" -> {{"entity": "Japan", "property": "GDP", "sparql_property": "wdt:P2131"}}
- "Who is the CEO of Tesla?" -> {{"entity": "Tesla, Inc.", "property": "CEO", "sparql_property": "wdt:P169"}}
- "When was the Eiffel Tower built?" -> {{"entity": "Eiffel Tower", "property": "inception", "sparql_property": "wdt:P571"}}
- "What is the area of France?" -> {{"entity": "France", "property": "area", "sparql_property": "wdt:P2046"}}
- "What is the capital of Brazil?" -> {{"entity": "Brazil", "property": "capital", "sparql_property": "wdt:P36"}}

Common Wikidata properties: P1082 (population), P2131 (GDP nominal), P2046 (area), P36 (capital), P6 (head of government), P169 (CEO), P571 (inception/founded), P17 (country), P1081 (HDI), P2299 (unemployment rate), P2250 (life expectancy), P1566 (GeoNames ID).

If the question is NOT about a specific entity's property (e.g., opinions, analysis, trends, comparisons), return {{"entity": "NONE"}}.

Return ONLY valid JSON, no other text."#,
        gap_query
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You extract structured entity-property pairs. Return ONLY valid JSON."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": short_output_budget(state),
        "think": false
    });

    let response = call_llm(state, request).await.ok()?;
    let content = extract_content(&response)?;
    let json = parse_json_from_llm(&content).ok()?;

    let entity = json.get("entity").and_then(|v| v.as_str()).unwrap_or("NONE");
    if entity == "NONE" || entity.is_empty() {
        eprintln!("[sparql] not an entity-fact question, skipping");
        return None;
    }
    let sparql_property = json.get("sparql_property").and_then(|v| v.as_str()).unwrap_or("");
    eprintln!("[sparql] extracted: entity=\"{}\" property=\"{}\"", entity, sparql_property);

    // Step 2: Resolve entity name to QID via Wikidata search API
    let is_wikidata = endpoint_name.to_lowercase().contains("wikidata");
    let qid = if is_wikidata {
        resolve_wikidata_qid(&state.http_client, entity).await
    } else {
        None
    };

    // Step 3: Build SPARQL query with verified QID
    if let Some(qid) = qid {
        if !sparql_property.is_empty() && sparql_property.starts_with("wdt:") {
            // Precise query: known entity + known property
            let query = format!(
                r#"SELECT ?value ?valueLabel WHERE {{
  wd:{qid} {prop} ?value .
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en" . }}
}}
LIMIT 10"#,
                qid = qid, prop = sparql_property
            );
            eprintln!("[sparql] built query: wd:{} {} -> ...", qid, sparql_property);
            return Some(query);
        } else {
            // Broad query: get key properties of the entity
            let query = format!(
                r#"SELECT ?property ?propertyLabel ?value ?valueLabel WHERE {{
  wd:{qid} ?prop ?value .
  ?property wikibase:directClaim ?prop .
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en" . }}
}}
LIMIT 20"#,
                qid = qid
            );
            eprintln!("[sparql] built broad query for wd:{}", qid);
            return Some(query);
        }
    }

    eprintln!("[sparql] could not resolve entity \"{}\" to QID", entity);
    None
}

/// Resolve an entity name to a Wikidata QID using the wbsearchentities API.
async fn resolve_wikidata_qid(client: &reqwest::Client, entity_name: &str) -> Option<String> {

    let url = format!(
        "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language=en&limit=3&format=json",
        urlencoding::encode(entity_name),
    );

    let resp = client.get(&url)
        .header("User-Agent", "engram/1.1 (knowledge-graph)")
        .send().await.ok()?;

    let data: serde_json::Value = resp.json().await.ok()?;
    let results = data.get("search")?.as_array()?;

    if let Some(first) = results.first() {
        let qid = first.get("id")?.as_str()?;
        let label = first.get("label").and_then(|l| l.as_str()).unwrap_or("");
        let desc = first.get("description").and_then(|d| d.as_str()).unwrap_or("");
        eprintln!("[sparql] resolved \"{}\" -> {} ({}: {})", entity_name, qid, label, desc);
        return Some(qid.to_string());
    }

    None
}

/// Format SPARQL JSON results into readable text.
fn format_sparql_results(data: &serde_json::Value) -> String {
    let bindings = match data.pointer("/results/bindings").and_then(|b| b.as_array()) {
        Some(b) if !b.is_empty() => b,
        _ => return String::new(),
    };

    let vars: Vec<&str> = data.pointer("/head/vars")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut lines = Vec::new();
    for binding in bindings.iter().take(10) {
        let mut parts = Vec::new();
        for var in &vars {
            if let Some(val) = binding.get(*var) {
                let value = val.get("value").and_then(|v| v.as_str()).unwrap_or("");
                if !value.is_empty() {
                    parts.push(format!("{}: {}", var, value));
                }
            }
        }
        if !parts.is_empty() {
            lines.push(parts.join(", "));
        }
    }

    lines.join("\n")
}

// ── Wikipedia API ──────────────────────────────────────────────────────

/// Search Wikipedia for a gap query. Returns extracted answer if a relevant article is found.
/// Uses the Wikipedia REST API (free, no auth, no rate limits, ~200ms).
async fn search_wikipedia(state: &AppState, gap_query: &str) -> Option<GapAnswer> {
    let client = &state.http_client;

    // Step 1: Search Wikipedia for relevant articles
    let search_url = format!(
        "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&srnamespace=0&srlimit=3&format=json",
        urlencoding::encode(gap_query),
    );

    let resp = client.get(&search_url)
        .header("User-Agent", "engram/1.1 (knowledge-graph; contact@engram.dev)")
        .send().await.ok()?;
    if !resp.status().is_success() {
        eprintln!("[wikipedia] search failed: HTTP {}", resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let results = data.pointer("/query/search")?.as_array()?;
    if results.is_empty() {
        return None;
    }

    // Step 2: Get the extract (plain text summary) of the top result
    let title = results[0].get("title")?.as_str()?;
    let extract_url = format!(
        "https://en.wikipedia.org/w/api.php?action=query&titles={}&prop=extracts&exintro=false&explaintext=true&exlimit=1&format=json",
        urlencoding::encode(title),
    );

    let resp = client.get(&extract_url)
        .header("User-Agent", "engram/1.1 (knowledge-graph; contact@engram.dev)")
        .send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;

    // Wikipedia API nests pages under their page ID
    let pages = data.pointer("/query/pages")?.as_object()?;
    let page = pages.values().next()?;
    let extract = page.get("extract")?.as_str()?;

    if extract.len() < 100 {
        return None;
    }

    // Truncate to reasonable size for LLM
    let content = if extract.len() > 4000 { &extract[..4000] } else { extract };
    let wiki_url = format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(title));

    // Step 3: LLM reading comprehension on the Wikipedia article
    answer_gap_from_article(state, gap_query, &wiki_url, &format!("Wikipedia: {}", title), content).await
}

fn empty_gap(query: &str) -> GapResearch {
    GapResearch {
        gap_query: query.to_string(), source: "error".into(), findings: Vec::new(),
        ingested: false, entities_stored: Vec::new(), facts_stored: 0, relations_created: 0,
    }
}

/// LLM reading comprehension on a single article.
/// Returns an answer if the article contains relevant information.
async fn answer_gap_from_article(
    state: &AppState,
    gap_query: &str,
    url: &str,
    title: &str,
    content: &str,
) -> Option<GapAnswer> {
    let prompt = format!(
        r#"Read this article and extract information relevant to the question.

Question: "{}"

Article: "{}"
{}

You MUST return found=true if the article contains ANY of these:
- Numbers, percentages, or statistics related to the topic
- Dates, timelines, or trends
- Names of organizations, countries, or people involved
- General facts or context about the topic

Only return found=false if the article is completely unrelated to the question.

Return ONLY this JSON (no other text, no code fences):
{{"found": true, "answer": "2-4 sentences summarizing what this article says about the topic, include all numbers and facts"}}
or
{{"found": false}}"#,
        gap_query, title, &content[..content.len().min(4000)]
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You extract information from articles. Be generous -- if the article is even remotely related to the question, return found=true with a summary. Return ONLY raw JSON, no markdown fences."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": short_output_budget(state),
        "think": false
    });

    let t0 = std::time::Instant::now();
    let response = match call_llm(state, request).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[debate] gap-read: LLM error for \"{}\": {}", &title[..title.len().min(40)], e);
            return None;
        }
    };
    let raw_content = match extract_content(&response) {
        Some(c) => c,
        None => {
            eprintln!("[debate] gap-read: empty LLM response for \"{}\"", &title[..title.len().min(40)]);
            return None;
        }
    };

    let json = match parse_json_from_llm(&raw_content) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[debate] gap-read: JSON parse failed for \"{}\": {} | raw({} chars): {}", &title[..title.len().min(40)], e, raw_content.len(), &raw_content[..raw_content.len().min(500)]);
            return None;
        }
    };

    let found = json.get("found").and_then(|f| f.as_bool()).unwrap_or(false);
    eprintln!("[debate] gap-read: found={} in {:.1}s for \"{}\"", found, t0.elapsed().as_secs_f32(), &title[..title.len().min(40)]);

    if !found {
        return None;
    }

    let answer = json.get("answer").and_then(|v| v.as_str())?.to_string();
    if answer.len() < 20 { return None; }

    Some(GapAnswer {
        text: answer,
        source_title: title.to_string(),
        source_url: url.to_string(),
    })
}

/// Cross-validate 2 answers from different sources.
/// Combines them into a single verified answer, flagging contradictions.
async fn cross_validate_answers(
    state: &AppState,
    gap_query: &str,
    answers: &[GapAnswer],
) -> String {
    let prompt = format!(
        r#"Two independent sources answered this question. Cross-validate and combine them.

Question: "{}"

Source 1 ({}): {}

Source 2 ({}): {}

Instructions:
- If both sources AGREE on specific data, state it with high confidence.
- If they provide DIFFERENT but complementary data, combine both.
- If they CONTRADICT each other, note the disagreement and state both figures.
- Include ALL specific numbers, dates, and names from both sources.
- 3-5 sentences. Factual only, no opinion."#,
        gap_query,
        &answers[0].source_title, &answers[0].text,
        &answers[1].source_title, &answers[1].text,
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a fact-checker cross-validating data from multiple sources. Be precise."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": short_output_budget(state),
        "think": false
    });

    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                let trimmed = content.trim().to_string();
                eprintln!("[debate] gap: cross-validated {} chars from 2 sources", trimmed.len());
                return trimmed;
            }
        }
        Err(e) => {
            eprintln!("[debate] gap: cross-validation LLM failed: {}, using source 1", e);
        }
    }

    // Fallback: concatenate both answers
    format!("{} Additionally: {}", answers[0].text, answers[1].text)
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
- Adding specific data sources (e.g., WHO, World Bank, official reports)
- Including specific years or units
- Being more or less specific

Return ONLY the new query string, nothing else."#,
        failed_query, topic, original_gap
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You are a search query optimizer. Return ONLY the improved query string, nothing else."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.5,
        "max_tokens": short_output_budget(state),
        "think": false
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
        "messages": [
            {"role": "system", "content": "You are a fact-checking analyst. Respond ONLY with valid JSON, no explanation."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2,
        "max_tokens": medium_output_budget(state),
        "think": false
    });

    let claims: Vec<(String, String)> = match call_llm(state, request).await {
        Ok(response) => {
            let content = extract_content(&response);
            let content_str = content.unwrap_or_default();
            if content_str.is_empty() {
                eprintln!("[debate] moderate_round: empty LLM content, raw response: {}", &response.to_string()[..response.to_string().len().min(500)]);
                Vec::new()
            } else {
                match parse_json_from_llm(&content_str) {
                    Ok(parsed) => {
                        if let Some(arr) = parsed.as_array() {
                            let c: Vec<_> = arr.iter().filter_map(|v| {
                                let aid = v.get("agent_id")?.as_str()?.to_string();
                                let claim = v.get("claim")?.as_str()?.to_string();
                                Some((aid, claim))
                            }).collect();
                            eprintln!("[debate] moderate_round: extracted {} claims", c.len());
                            c
                        } else {
                            eprintln!("[debate] moderate_round: parsed JSON not array: {}", parsed);
                            Vec::new()
                        }
                    }
                    Err(e) => {
                        eprintln!("[debate] moderate_round: JSON parse failed: {} | raw: {}", e, &content_str[..content_str.len().min(300)]);
                        Vec::new()
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[debate] moderate_round: LLM call failed: {}", e);
            Vec::new()
        }
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

pub use crate::handlers::web_search::WebSearchResult;

/// Execute a web search and return structured results (with URLs).
pub async fn execute_web_search_structured(state: &AppState, query: &str) -> Vec<WebSearchResult> {
    match crate::handlers::web_search::search(state, query).await {
        Ok(results) => results,
        Err(e) => {
            eprintln!("[debate] web search failed: {}", e);
            Vec::new()
        }
    }
}

/// Legacy wrapper: returns formatted strings for display.
pub async fn execute_web_search(state: &AppState, query: &str) -> String {
    let results = execute_web_search_structured(state, query).await;
    results.iter()
        .map(|r| format!("- {}: {}", r.title, r.snippet))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Public debug wrapper for fetch_article_content.
pub async fn fetch_article_content_debug(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build().ok()?;
    fetch_article_content(&client, url).await
}

/// Fetch article content from a URL using dom_smoothie (Mozilla Readability).
/// Returns extracted plain text, or None on failure.
async fn fetch_article_content(client: &reqwest::Client, url: &str) -> Option<String> {
    let url_short = &url[..url.len().min(80)];
    let t0 = std::time::Instant::now();

    let fetch_url = if url.contains("reddit.com/") && !url.contains("old.reddit.com") {
        url.replace("www.reddit.com", "old.reddit.com")
            .replace("reddit.com", "old.reddit.com")
    } else {
        url.to_string()
    };

    eprintln!("[fetch] START {}", url_short);
    let resp = match client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (compatible; engram/1.1)")
        .send().await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[fetch] FAIL {:.1}s send error: {} | {}", t0.elapsed().as_secs_f32(), e, url_short);
            return None;
        }
    };
    eprintln!("[fetch] HTTP {} in {:.1}s | {}", resp.status(), t0.elapsed().as_secs_f32(), url_short);
    if !resp.status().is_success() { return None; }

    let html = match resp.text().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("[fetch] FAIL {:.1}s body read error: {} | {}", t0.elapsed().as_secs_f32(), e, url_short);
            return None;
        }
    };
    eprintln!("[fetch] body {} bytes in {:.1}s | {}", html.len(), t0.elapsed().as_secs_f32(), url_short);
    if html.len() < 200 { return None; }

    // Cap HTML size to prevent dom_smoothie from spending too long on huge pages
    let html_capped = if html.len() > 500_000 {
        eprintln!("[fetch] SKIP html too large ({} bytes) | {}", html.len(), url_short);
        return None;
    } else {
        html
    };

    // dom_smoothie is synchronous -- run on blocking thread to avoid starving tokio
    let parse_result = tokio::task::spawn_blocking(move || {
        let t_parse = std::time::Instant::now();
        let mut readability = dom_smoothie::Readability::new(html_capped, None, None).ok()?;
        let article = readability.parse().ok()?;
        let text = article.text_content.to_string().trim().to_string();
        eprintln!("[fetch] parsed {} chars in {:.1}s", text.len(), t_parse.elapsed().as_secs_f32());
        if text.len() > 100 { Some(text) } else { None }
    }).await.ok().flatten();

    eprintln!("[fetch] DONE {:.1}s result={} | {}", t0.elapsed().as_secs_f32(), if parse_result.is_some() { "OK" } else { "EMPTY" }, url_short);
    parse_result
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
