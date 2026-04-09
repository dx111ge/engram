/// Research engine for the debate panel.
/// - Starter plate: deep topic decomposition + multi-source fact gathering
/// - Persistent gap-closing: multi-attempt with relevance filtering + query reformulation
/// - Moderator: fact-check claims against engram confidence

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content, parse_json_from_llm, short_output_budget, medium_output_budget};

/// Safely truncate a string at a char boundary, avoiding panics on multi-byte UTF-8.
pub(crate) fn safe_truncate(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map(|(i, _)| &s[..i]).unwrap_or(s)
}

// ── Starter Plate ───────────────────────────────────────────────────────

/// Build the starter plate briefing: decompose topic, gather facts, ingest, assemble.
pub async fn build_starter_plate(state: &AppState, topic: &str, topic_languages: &[String], tx: &tokio::sync::broadcast::Sender<String>) -> Briefing {
    let questions = decompose_topic(state, topic).await;
    let mut all_facts = Vec::new();
    let mut total_stored = 0u32;
    let mut total_relations = 0u32;

    let q_total = questions.len() + 1; // +1 for exact topic search

    // Search the exact topic first (highest priority)
    let _ = tx.send(format!("event: briefing_progress\ndata: {}\n\n",
        serde_json::json!({"question": topic, "index": 1, "total": q_total, "phase": "searching"})));
    let exact_facts = gather_facts_for_question(state, topic, topic, topic_languages).await;
    let _ = tx.send(format!("event: briefing_progress\ndata: {}\n\n",
        serde_json::json!({"question": topic, "index": 1, "total": q_total, "facts_found": exact_facts.len()})));
    all_facts.extend(exact_facts);

    // Search each decomposed question (dedup by content to avoid 4x repeats)
    for (qi, question) in questions.iter().enumerate() {
        let _ = tx.send(format!("event: briefing_progress\ndata: {}\n\n",
            serde_json::json!({"question": question, "index": qi + 2, "total": q_total, "phase": "searching"})));
        let facts = gather_facts_for_question(state, question, topic, topic_languages).await;
        let new_count = facts.len();
        for fact in facts {
            if !all_facts.iter().any(|f| f.content == fact.content) {
                all_facts.push(fact);
            }
        }
        let _ = tx.send(format!("event: briefing_progress\ndata: {}\n\n",
            serde_json::json!({"question": question, "index": qi + 2, "total": q_total, "facts_found": new_count})));
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

/// Gather facts for a question from graph + web (multi-language).
async fn gather_facts_for_question(state: &AppState, question: &str, _topic: &str, topic_languages: &[String]) -> Vec<BriefingFact> {
    let mut facts = Vec::new();

    // Graph search (spawn_blocking to never block async runtime)
    {
        let graph = state.graph.clone();
        let q = question.to_string();
        let graph_facts = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let g = match graph.read() {
                    Ok(g) => g,
                    Err(_) => return Vec::new(),
                };
                let mut result = Vec::new();
                if let Ok(results) = g.search_text(&q, 5) {
                    for r in &results {
                        if r.confidence >= 0.3 {
                            let mut content = format!("{} (confidence: {:.2})", r.label, r.confidence);
                            if let Ok(edges) = g.edges_from(&r.label) {
                                for e in edges.iter().take(3) {
                                    content.push_str(&format!("\n  -> {} -> {} ({:.2})", e.relationship, e.to, e.confidence));
                                }
                            }
                            result.push(BriefingFact {
                                question: q.clone(),
                                source: "graph".into(),
                                content,
                                confidence: r.confidence,
                            });
                        }
                    }
                }
                result
            }),
        ).await;
        if let Ok(Ok(graph_results)) = graph_facts {
            facts.extend(graph_results);
        }
    }

    // Web search -- English first, then retry with non-English topic languages
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

    // Retry with non-English languages for additional primary sources
    for lang in topic_languages.iter().filter(|l| l.as_str() != "en") {
        if let Ok(lang_results) = crate::handlers::web_search::search_with_language(state, question, lang).await {
            for r in lang_results.iter().take(3) {
                let content = format!("[{}] {} -- {}", lang, r.title, r.snippet);
                if !facts.iter().any(|f| f.content == content) {
                    facts.push(BriefingFact {
                        question: question.to_string(),
                        source: "web".into(),
                        content,
                        confidence: 0.35,
                    });
                }
            }
        }
    }

    facts
}

/// Search graph for topic-related facts (used after ingest to find enriched data).
async fn gather_graph_facts(state: &AppState, topic: &str) -> Vec<BriefingFact> {
    let graph = state.graph.clone();
    let topic_owned = topic.to_string();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            let mut facts = Vec::new();
            let g = match graph.read() {
                Ok(g) => g,
                Err(_) => return facts,
            };
            let terms: Vec<&str> = topic_owned.split_whitespace().filter(|w| w.len() > 3).take(5).collect();
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
                            question: topic_owned.clone(),
                            source: "graph".into(),
                            content,
                            confidence: r.confidence,
                        });
                    }
                }
            }
            facts
        }),
    ).await;
    match result {
        Ok(Ok(facts)) => facts,
        _ => Vec::new(),
    }
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

    dbg_debate!("[gap-detect] calling LLM with {} chars prompt", prompt.len());
    eprintln!("[debate] detect_gaps: calling LLM now...");
    match call_llm(state, request).await {
        Ok(response) => {
            dbg_debate!("[gap-detect] LLM returned OK");
            let content = extract_content(&response);
            let content_str = content.unwrap_or_default();
            if content_str.is_empty() {
                eprintln!("[debate] detect_gaps: empty LLM content, raw response: {}", safe_truncate(&response.to_string(), 500));
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
                    eprintln!("[debate] detect_gaps: JSON parse failed: {} | raw: {}", e, safe_truncate(&content_str, 300));
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
pub async fn close_gaps(state: &AppState, gaps: &[String], topic: &str, topic_languages: &[String], tx: &tokio::sync::broadcast::Sender<String>) -> Vec<GapResearch> {
    let mut results = Vec::new();
    for (i, gap_query) in gaps.iter().enumerate() {
        eprintln!("[debate] gap {}/{}: \"{}\"", i + 1, gaps.len(), safe_truncate(gap_query, 60));
        let result = close_single_gap(state, gap_query, topic, topic_languages, tx).await;
        results.push(result);
    }
    results
}

/// Close a single gap with a timeout safety net.
pub async fn close_single_gap_with_timeout(state: &AppState, query: &str, topic: &str, timeout_secs: u64, topic_languages: &[String], tx: &tokio::sync::broadcast::Sender<String>) -> GapResearch {
    let _ = tx.send(format!("event: gap_progress\ndata: {}\n\n",
        serde_json::json!({"gap": safe_truncate(query, 80), "status": "searching"})));

    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        close_single_gap(state, query, topic, topic_languages, tx),
    ).await {
        Ok(result) => result,
        Err(_) => {
            eprintln!("[debate] gap TIMED OUT ({}s): \"{}\"", timeout_secs, safe_truncate(query, 60));
            let _ = tx.send(format!("event: gap_progress\ndata: {}\n\n",
                serde_json::json!({"gap": safe_truncate(query, 80), "status": "timeout"})));
            empty_gap(query)
        }
    };

    let _ = tx.send(format!("event: gap_progress\ndata: {}\n\n",
        serde_json::json!({
            "gap": safe_truncate(query, 80),
            "status": if result.ingested { "ingested" } else { "complete" },
            "source": result.source,
            "facts_stored": result.facts_stored,
            "findings": result.findings.len(),
        })));

    result
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
async fn close_single_gap(state: &AppState, original_query: &str, _topic: &str, topic_languages: &[String], tx: &tokio::sync::broadcast::Sender<String>) -> GapResearch {
    let mut all_findings = Vec::new();
    let mut facts_stored = 0u32;
    let mut relations_created = 0u32;
    let mut entities_stored = Vec::new();
    let mut ingested = false;
    let blocked = resolve_blocked_domains(state);

    let t0 = std::time::Instant::now();
    dbg_debate!("[gap] ===== START gap: \"{}\" =====", original_query);

    // 1. Check graph first (spawn_blocking to never block async runtime)
    let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
        serde_json::json!({"message": format!("Searching engram graph: {}", safe_truncate(original_query, 60))})));
    {
        let graph = state.graph.clone();
        let q = original_query.to_string();
        let graph_findings = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let g = match graph.read() {
                    Ok(g) => g,
                    Err(_) => return Vec::new(),
                };
                let mut findings = Vec::new();
                if let Ok(results) = g.search_text(&q, 5) {
                    for r in &results {
                        findings.push(format!("[graph] {} (confidence: {:.2})", r.label, r.confidence));
                    }
                }
                findings
            }),
        ).await;
        if let Ok(Ok(findings)) = graph_findings {
            if !findings.is_empty() {
                let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
                    serde_json::json!({"message": format!("Graph: {} existing facts found", findings.len())})));
            }
            for f in findings {
                if !all_findings.contains(&f) {
                    all_findings.push(f);
                }
            }
        }
    }

    let mut answers: Vec<GapAnswer> = Vec::new();

    // 2. SPARQL knowledge bases (most precise for entity facts)
    let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
        serde_json::json!({"message": "Querying SPARQL knowledge bases..."})));
    let primary_lang = topic_languages.first().map(|s| s.as_str()).unwrap_or("en");
    if let Some(sparql_answer) = query_sparql_endpoints(state, original_query, primary_lang).await {
        all_findings.push(format!("[sparql] {}: {}", sparql_answer.source_title, safe_truncate(&sparql_answer.text, 100)));
        let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
            serde_json::json!({"message": format!("SPARQL: found answer from {}", sparql_answer.source_title)})));
        eprintln!("[debate] gap: sparql answer from \"{}\" ({} chars) in {:.1}s",
            sparql_answer.source_title, sparql_answer.text.len(), t0.elapsed().as_secs_f32());
        answers.push(sparql_answer);
    } else {
        eprintln!("[debate] gap: no sparql answer in {:.1}s", t0.elapsed().as_secs_f32());
    }

    // Shorten query early — used for both Wikipedia and web search
    let short_query = shorten_for_search(state, original_query).await;

    // 3. Wikipedia API (free, fast, trusted, no rate limits)
    //    Use the shortened query — long gap questions return irrelevant Wikipedia articles
    if answers.len() < 2 {
        let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
            serde_json::json!({"message": format!("Searching Wikipedia: {}", safe_truncate(&short_query, 50))})));
        if let Some(wiki_answer) = search_wikipedia(state, &short_query).await {
            all_findings.push(format!("[wikipedia] {}: {}", wiki_answer.source_title, safe_truncate(&wiki_answer.text, 100)));
            let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
                serde_json::json!({"message": format!("Wikipedia: {}", wiki_answer.source_title)})));
            eprintln!("[debate] gap: wikipedia answer from \"{}\" ({} chars) in {:.1}s",
                wiki_answer.source_title, wiki_answer.text.len(), t0.elapsed().as_secs_f32());
            answers.push(wiki_answer);
        } else {
            eprintln!("[debate] gap: no wikipedia answer in {:.1}s", t0.elapsed().as_secs_f32());
        }
    }

    // 4. Web search for second source (or first if Wikipedia had nothing)
    let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
        serde_json::json!({"message": format!("Web search: {}", safe_truncate(&short_query, 50))})));
    let mut web_results = execute_web_search_structured(state, &short_query).await;
    eprintln!("[debate] gap: web_results={} (en) in {:.1}s", web_results.len(), t0.elapsed().as_secs_f32());
    if !web_results.is_empty() {
        let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
            serde_json::json!({"message": format!("Web: {} results, fetching articles...", web_results.len())})));
    }

    // If English search returned nothing, try non-English topic languages
    if web_results.is_empty() {
        for lang in topic_languages.iter().filter(|l| l.as_str() != "en") {
            dbg_debate!("[gap] retrying search in language '{}' for: {}", lang, safe_truncate(&short_query, 50));
            match crate::handlers::web_search::search_with_language(state, &short_query, lang).await {
                Ok(results) if !results.is_empty() => {
                    eprintln!("[debate] gap: web_results={} ({}) in {:.1}s", results.len(), lang, t0.elapsed().as_secs_f32());
                    web_results = results;
                    break;
                }
                _ => continue,
            }
        }
    }

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
        let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
            serde_json::json!({"message": format!("Fetching: {}", safe_truncate(&result.title, 50))})));
        let fetched = match fetch_article_content(&state.http_client, &result.url, &blocked).await {
            Some(f) => f,
            None => continue,
        };
        urls_fetched += 1;

        // Compute content hash (same algorithm as pipeline: SHA-256 of extracted text)
        let content_hash = engram_core::storage::doc_store::DocStore::hash_content(fetched.text.as_bytes());
        let content_hash_hex = engram_core::storage::doc_store::DocStore::hash_hex(&content_hash);
        let mime = engram_core::storage::doc_store::MimeType::from_mime_str(&fetched.mime_type);

        // Store to doc_store with correct mime type (fire-and-forget)
        {
            let doc_store = state.doc_store.clone();
            let content_bytes = fetched.text.as_bytes().to_vec();
            tokio::spawn(async move {
                if let Ok(mut store) = doc_store.write() {
                    let _ = store.store(&content_bytes, mime);
                }
            });
        }

        // Create pending Document node with full metadata (ner_complete: false)
        // Skip if URL is empty (shouldn't happen after fetch, but guard against bare nodes)
        let doc_url = if result.url.is_empty() { None } else { Some(result.url.as_str()) };
        let doc_title = if result.title.is_empty() { None } else { Some(result.title.as_str()) };
        if doc_url.is_some() {
            create_pending_document_node(
                &state.graph,
                &content_hash_hex,
                doc_url,
                doc_title,
                &fetched.mime_type,
                fetched.text.len(),
                topic_languages.first().map(|s| s.as_str()),
            );
        }

        let content = fetched.text;
        let truncated = safe_truncate(&content, 4000);

        // LLM reading comprehension on this single article
        let answer = answer_gap_from_article(state, original_query, &result.url, &result.title, truncated).await;
        if let Some(a) = answer {
            eprintln!("[debate] gap: answer {}/2 from \"{}\" ({} chars)",
                answers.len() + 1, safe_truncate(&result.title, 40), a.text.len());
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

    all_findings.push(format!("[answer] {}", safe_truncate(&final_answer, 200)));

    // 5. Ingest the validated answer
    let source_label = if answers.is_empty() {
        "web".to_string()
    } else {
        // Already removed first for single-answer case; use the stored findings
        format!("gap: {}", safe_truncate(original_query, 60))
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
        if let Ok(mut g) = state.graph.try_write() {
            let label = safe_truncate(&final_answer, 250);
            if g.store_with_confidence(label, 0.55, &prov).is_ok() {
                state.dirty.store(true, std::sync::atomic::Ordering::Release);
                facts_stored += 1;
                ingested = true;
                entities_stored.push(format!("direct: {}", safe_truncate(&final_answer, 80)));
                eprintln!("[debate] gap: stored as direct node in {:.1}s", t0.elapsed().as_secs_f32());
            }
        }
    }

    dbg_debate!("[gap] ===== END gap: {} answers, ingested={}, {:.1}s total =====", answers.len(), ingested, t0.elapsed().as_secs_f32());

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
async fn query_sparql_endpoints(state: &AppState, gap_query: &str, language: &str) -> Option<GapAnswer> {
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
        let t_ep = std::time::Instant::now();
        // Step 1: LLM generates a SPARQL query for this endpoint
        let sparql = generate_sparql_query(state, gap_query, &ep.name, language).await;
        let sparql = match sparql {
            Some(q) if !q.is_empty() => q,
            _ => continue,
        };
        eprintln!("[sparql] generated query for {}:\n{}", ep.name, safe_truncate(&sparql, 500));

        // Step 2: Execute SPARQL
        dbg_debate!("[sparql] >> querying {} url={}", ep.name, ep.url);
        let mut req = client.get(&ep.url)
            .query(&[("query", sparql.as_str()), ("format", "json")])
            .timeout(std::time::Duration::from_secs(10))
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
                dbg_debate!("[sparql] << {} request failed in {:.1}s: {}", ep.name, t_ep.elapsed().as_secs_f32(), e);
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
                eprintln!("[sparql] {} JSON parse failed: {} | body: {}", ep.name, e, safe_truncate(&resp_text, 300));
                continue;
            }
        };

        // Step 3: Format SPARQL results into readable text
        let text = format_sparql_results(&data);
        if text.is_empty() {
            eprintln!("[sparql] {} returned 0 results", ep.name);
            continue;
        }

        dbg_debate!("[sparql] << {} returned {} chars of data in {:.1}s", ep.name, text.len(), t_ep.elapsed().as_secs_f32());

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
async fn generate_sparql_query(state: &AppState, gap_query: &str, endpoint_name: &str, language: &str) -> Option<String> {
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
        resolve_wikidata_qid(&state.http_client, entity, language).await
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
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "{lang},en" . }}
}}
LIMIT 10"#,
                qid = qid, prop = sparql_property, lang = language
            );
            eprintln!("[sparql] built query: wd:{} {} -> ...", qid, sparql_property);
            return Some(query);
        } else {
            // Broad query: get key properties of the entity
            let query = format!(
                r#"SELECT ?property ?propertyLabel ?value ?valueLabel WHERE {{
  wd:{qid} ?prop ?value .
  ?property wikibase:directClaim ?prop .
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "{lang},en" . }}
}}
LIMIT 20"#,
                qid = qid, lang = language
            );
            eprintln!("[sparql] built broad query for wd:{}", qid);
            return Some(query);
        }
    }

    eprintln!("[sparql] could not resolve entity \"{}\" to QID", entity);
    None
}

/// Resolve an entity name to a Wikidata QID using the wbsearchentities API.
async fn resolve_wikidata_qid(client: &reqwest::Client, entity_name: &str, language: &str) -> Option<String> {
    dbg_debate!("[wikidata] >> resolving entity=\"{}\" lang={}", entity_name, language);
    let t0 = std::time::Instant::now();

    let lang = if language.is_empty() { "en" } else { language };
    let url = format!(
        "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language={}&limit=3&format=json",
        urlencoding::encode(entity_name), lang,
    );

    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", "engram/1.1 (knowledge-graph)")
        .send().await.ok()?;

    let data: serde_json::Value = resp.json().await.ok()?;
    let results = data.get("search")?.as_array()?;

    if let Some(first) = results.first() {
        let qid = first.get("id")?.as_str()?;
        let label = first.get("label").and_then(|l| l.as_str()).unwrap_or("");
        let desc = first.get("description").and_then(|d| d.as_str()).unwrap_or("");
        dbg_debate!("[wikidata] << resolved \"{}\" -> {} ({}: {}) in {:.1}s", entity_name, qid, label, desc, t0.elapsed().as_secs_f32());
        return Some(qid.to_string());
    }

    dbg_debate!("[wikidata] << no result for \"{}\" in {:.1}s", entity_name, t0.elapsed().as_secs_f32());
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
    let t0 = std::time::Instant::now();
    dbg_debate!("[wikipedia] >> searching \"{}\"", gap_query);

    // Step 1: Search Wikipedia for relevant articles
    let search_url = format!(
        "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&srnamespace=0&srlimit=3&format=json",
        urlencoding::encode(gap_query),
    );

    let resp = client.get(&search_url)
        .timeout(std::time::Duration::from_secs(8))
        .header("User-Agent", "engram/1.1 (knowledge-graph; contact@engram.dev)")
        .send().await.ok()?;
    if !resp.status().is_success() {
        dbg_debate!("[wikipedia] << search failed: HTTP {} in {:.1}s", resp.status(), t0.elapsed().as_secs_f32());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let results = data.pointer("/query/search")?.as_array()?;
    if results.is_empty() {
        dbg_debate!("[wikipedia] << 0 results in {:.1}s", t0.elapsed().as_secs_f32());
        return None;
    }
    dbg_debate!("[wikipedia] << {} results in {:.1}s", results.len(), t0.elapsed().as_secs_f32());

    // Step 2: Get the extract (plain text summary) of the top result
    let title = results[0].get("title")?.as_str()?;
    dbg_debate!("[wikipedia] >> fetching article \"{}\"", title);
    let extract_url = format!(
        "https://en.wikipedia.org/w/api.php?action=query&titles={}&prop=extracts&exintro=false&explaintext=true&exlimit=1&format=json",
        urlencoding::encode(title),
    );

    let resp = client.get(&extract_url)
        .timeout(std::time::Duration::from_secs(8))
        .header("User-Agent", "engram/1.1 (knowledge-graph; contact@engram.dev)")
        .send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;

    // Wikipedia API nests pages under their page ID
    let pages = data.pointer("/query/pages")?.as_object()?;
    let page = pages.values().next()?;
    let extract = page.get("extract")?.as_str()?;

    if extract.len() < 100 {
        dbg_debate!("[wikipedia] << article \"{}\" too short ({} chars) in {:.1}s", title, extract.len(), t0.elapsed().as_secs_f32());
        return None;
    }
    dbg_debate!("[wikipedia] << article \"{}\" {} chars in {:.1}s", title, extract.len(), t0.elapsed().as_secs_f32());

    // Truncate to reasonable size for LLM
    let content = safe_truncate(extract, 4000);
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
- Numbers, percentages, statistics, or data tables (even from other countries as baselines)
- Dates, timelines, trends, or time series data
- Names of organizations, countries, or people involved in the broader topic
- Comparative or contextual data that could inform analysis (e.g., global averages, peer country data)
- Economic indicators, production figures, trade volumes, even if from a different country
- General facts or context about the topic area

Only return found=false if the article is completely unrelated to the BROAD TOPIC AREA (not just the specific question).

Return ONLY this JSON (no other text, no code fences):
{{"found": true, "answer": "2-4 sentences summarizing ALL data and statistics in this article, noting the country/context. Include all numbers, percentages, dates, and comparisons."}}
or
{{"found": false}}"#,
        gap_query, title, safe_truncate(content, 4000)
    );

    let request = serde_json::json!({
        "messages": [
            {"role": "system", "content": "You extract information from articles for intelligence analysis. Return found=true if the article contains ANY data that could inform analysis of the topic -- including comparative baselines, contextual statistics, or related data from other countries/sectors. A US industrial production report IS relevant when analyzing Russian production because it provides comparison data. Return ONLY raw JSON, no markdown fences."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": short_output_budget(state),
        "think": false
    });

    let t0 = std::time::Instant::now();
    dbg_debate!("[gap] >> LLM reading article \"{}\" ({} chars)", safe_truncate(title, 40), content.len());
    let response = match call_llm(state, request).await {
        Ok(r) => r,
        Err(e) => {
            dbg_debate!("[gap] << LLM error for \"{}\" in {:.1}s: {}", safe_truncate(title, 40), t0.elapsed().as_secs_f32(), e);
            return None;
        }
    };
    let raw_content = match extract_content(&response) {
        Some(c) => c,
        None => {
            dbg_debate!("[gap] << empty LLM response for \"{}\" in {:.1}s", safe_truncate(title, 40), t0.elapsed().as_secs_f32());
            return None;
        }
    };

    let json = match parse_json_from_llm(&raw_content) {
        Ok(j) => j,
        Err(e) => {
            dbg_debate!("[gap] << JSON parse failed for \"{}\" in {:.1}s: {} | raw({} chars): {}", safe_truncate(title, 40), t0.elapsed().as_secs_f32(), e, raw_content.len(), safe_truncate(&raw_content, 500));
            // Try to salvage truncated JSON: if it starts with {"found": true, "answer": "...
            // extract the partial answer text
            if raw_content.contains("\"found\": true") || raw_content.contains("\"found\":true") {
                if let Some(start) = raw_content.find("\"answer\"") {
                    let after = &raw_content[start..];
                    // Find the opening quote of the answer value
                    if let Some(q1) = after.find(": \"").or_else(|| after.find(":\"")) {
                        let val_start = start + q1 + if after[q1..].starts_with(": \"") { 3 } else { 2 };
                        // Take everything after the opening quote, trim trailing junk
                        let partial = raw_content[val_start..].trim_end_matches(|c: char| c == '"' || c == '}' || c.is_whitespace());
                        if partial.len() >= 40 {
                            dbg_debate!("[gap] << salvaged truncated answer ({} chars) for \"{}\"", partial.len(), safe_truncate(title, 40));
                            return Some(GapAnswer {
                                text: partial.to_string(),
                                source_title: title.to_string(),
                                source_url: url.to_string(),
                            });
                        }
                    }
                }
            }
            return None;
        }
    };

    let found = json.get("found").and_then(|f| f.as_bool()).unwrap_or(false);
    dbg_debate!("[gap] << LLM result: found={} in {:.1}s for \"{}\"", found, t0.elapsed().as_secs_f32(), safe_truncate(title, 40));

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

    dbg_debate!("[gap] >> cross-validating {} answers via LLM", answers.len());
    let t_cv = std::time::Instant::now();
    match call_llm(state, request).await {
        Ok(response) => {
            if let Some(content) = extract_content(&response) {
                let trimmed = content.trim().to_string();
                dbg_debate!("[gap] << cross-validation done ({} chars) in {:.1}s", trimmed.len(), t_cv.elapsed().as_secs_f32());
                return trimmed;
            }
        }
        Err(e) => {
            dbg_debate!("[gap] << cross-validation LLM failed in {:.1}s: {}, using source 1", t_cv.elapsed().as_secs_f32(), e);
        }
    }

    // Fallback: concatenate both answers
    format!("{} Additionally: {}", answers[0].text, answers[1].text)
}


/// Reformulate a failed search query to try a different angle.
// ── Moderator ───────────────────────────────────────────────────────────

/// Fact-check agent claims against engram confidence scores.
pub async fn moderate_round(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
    tx: &tokio::sync::broadcast::Sender<String>,
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
                eprintln!("[debate] moderate_round: empty LLM content, raw response: {}", safe_truncate(&response.to_string(), 500));
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
                        eprintln!("[debate] moderate_round: JSON parse failed: {} | raw: {}", e, safe_truncate(&content_str, 300));
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

    // Fact-check claims against graph (spawn_blocking to never block async runtime)
    let graph = state.graph.clone();
    let claims_clone = claims.clone();
    let checks_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            let mut checks = Vec::new();
            let g = match graph.read() {
                Ok(g) => g,
                Err(_) => return checks,
            };

            for (agent_id, claim) in &claims_clone {
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
        }),
    ).await;

    let checks = match checks_result {
        Ok(Ok(checks)) => checks,
        _ => Vec::new(),
    };

    // Emit moderator check events for each claim
    for check in &checks {
        let _ = tx.send(format!("event: moderator_check\ndata: {}\n\n",
            serde_json::json!({
                "agent_id": check.agent_id,
                "claim": safe_truncate(&check.claim, 120),
                "verdict": format!("{:?}", check.verdict),
                "confidence": check.engram_confidence,
            })));
    }

    checks
}

// ── Shared helpers ──────────────────────────────────────────────────────

pub use crate::handlers::web_search::WebSearchResult;

/// Convert a natural-language question into a short search-engine-friendly query via LLM.
/// Falls back to the original query on LLM failure.
pub async fn shorten_for_search(state: &AppState, question: &str) -> String {
    let prompt = serde_json::json!({
        "messages": [{"role": "system", "content": format!(
            "Convert this question into a short search engine query (max 8 words). \
             Keep all key entities, numbers, and years. Drop filler words. \
             Return ONLY the query, nothing else.\n\nQuestion: \"{}\"", question
        )}],
        "temperature": 0.1,
        "max_tokens": 64,
        "think": false
    });
    match call_llm(state, prompt).await {
        Ok(resp) => {
            let content = extract_content(&resp).unwrap_or_default().trim().trim_matches('"').to_string();
            if content.len() > 5 {
                dbg_debate!("[search] shortened \"{}\" -> \"{}\"", safe_truncate(question, 60), content);
                content
            } else {
                question.to_string()
            }
        }
        Err(_) => question.to_string(),
    }
}

/// Execute a web search and return structured results (with URLs).
pub async fn execute_web_search_structured(state: &AppState, query: &str) -> Vec<WebSearchResult> {
    let t0 = std::time::Instant::now();
    dbg_debate!("[search] >> query=\"{}\"", query);
    match crate::handlers::web_search::search(state, query).await {
        Ok(results) => {
            dbg_debate!("[search] << {} results in {:.1}s", results.len(), t0.elapsed().as_secs_f32());
            results
        }
        Err(e) => {
            dbg_debate!("[search] << FAILED in {:.1}s: {}", t0.elapsed().as_secs_f32(), e);
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

/// Resolve blocked domains from config (user list takes priority over defaults).
fn resolve_blocked_domains(state: &AppState) -> Vec<String> {
    if let Ok(config) = state.config.read() {
        if let Some(ref user_list) = config.blocked_domains {
            return user_list.clone();
        }
    }
    DEFAULT_BLOCKED_DOMAINS.iter().map(|s| s.to_string()).collect()
}

/// Public debug wrapper for fetch_article_content.
pub async fn fetch_article_content_debug(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build().ok()?;
    let defaults: Vec<String> = DEFAULT_BLOCKED_DOMAINS.iter().map(|s| s.to_string()).collect();
    fetch_article_content(&client, url, &defaults).await.map(|f| f.text)
}

/// Default domains that always block scrapers -- skip to save time.
/// Users can override via config: blocked_domains (replaces this list entirely).
const DEFAULT_BLOCKED_DOMAINS: &[&str] = &[
    "studylibid.com", "studylib.net", "doczz.net",
];


/// Extract text from a PDF response. Delegates to shared `engram_ingest::pdf`
/// utility, then caps at 8000 chars for debate fast-path.
#[cfg(feature = "pdf")]
async fn extract_pdf_text(resp: reqwest::Response, url_short: &str) -> Option<String> {
    let t0 = std::time::Instant::now();
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            dbg_debate!("[fetch] PDF body read error: {} | {}", e, url_short);
            return None;
        }
    };

    // Cap PDF size at 20MB for debate fast-path
    if bytes.len() > 20_000_000 {
        dbg_debate!("[fetch] SKIP PDF too large ({} bytes) | {}", bytes.len(), url_short);
        return None;
    }

    dbg_debate!("[fetch] PDF {} bytes, extracting text... | {}", bytes.len(), url_short);

    let bytes_vec = bytes.to_vec();
    let text = match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            engram_ingest::pdf::extract_text_from_pdf(&bytes_vec)
        })
    ).await {
        Ok(Ok(Ok(t))) => t,
        Ok(Ok(Err(e))) => {
            dbg_debate!("[fetch] PDF extraction failed: {} | {}", e, url_short);
            return None;
        }
        Ok(Err(e)) => {
            dbg_debate!("[fetch] PDF extraction task failed: {} | {}", e, url_short);
            return None;
        }
        Err(_) => {
            dbg_debate!("[fetch] PDF extraction TIMED OUT after 30s | {}", url_short);
            return None;
        }
    };

    dbg_debate!("[fetch] PDF extracted {} chars in {:.1}s | {}", text.len(), t0.elapsed().as_secs_f32(), url_short);

    // Cap at 8000 chars for debate fast-path
    Some(engram_ingest::pdf::cap_text(&text, 8000))
}

/// Detect machine-readable download links in HTML (CSV, JSON, XML, XLSX, ASCII).
/// Scans full page for href attributes pointing to data files.
/// Returns the first usable download URL found.
fn find_data_download_link(html: &str, base_url: &str) -> Option<String> {
    // Only text-based data formats — skip binary (xlsx, xls) which produce garbage when read as text
    let extensions = [".csv", ".json", ".xml", ".tsv", ".ascii"];
    let lower = html.to_lowercase();

    // Scan all href="..." occurrences in the page
    for (pos, _) in lower.match_indices("href=\"") {
        let after = &lower[pos + 6..];
        let end = match after.find('"') {
            Some(e) if e < 500 => e, // reasonable URL length
            _ => continue,
        };
        let href = &html[pos + 6..pos + 6 + end]; // use original case from html

        // Check if href points to a data file
        let href_lower = href.to_lowercase();
        let matched_ext = extensions.iter().find(|ext| href_lower.ends_with(*ext) || href_lower.contains(&format!("{}?", ext)));
        if matched_ext.is_none() {
            // Also check for "download" or "export" in the href
            if !href_lower.contains("download") && !href_lower.contains("export") {
                continue;
            }
            // download/export links must also have a data-ish extension or format param
            if !extensions.iter().any(|ext| href_lower.contains(ext)) && !href_lower.contains("format=csv") && !href_lower.contains("format=json") {
                continue;
            }
        }

        if href.contains("javascript:") { continue; }

        // Filter out false positives (favicon, manifests, opensearch, feeds, social widgets)
        let skip_patterns = ["favicon", "manifest.json", "osd.xml", "opensearch",
            "apple-touch-icon", "browserconfig", "robots.txt", "sitemap",
            "wp-json", "oembed", "widget", "schema.json", "package.json",
            ".ico", "logo", "sprite", "comments/feed"];
        if skip_patterns.iter().any(|p| href_lower.contains(p)) { continue; }
        // Skip RSS/Atom feeds from non-data sites (they're news headlines, not structured data)
        if (href_lower.ends_with(".xml") || href_lower.contains("rss"))
            && !href_lower.contains("data") && !href_lower.contains("export") && !href_lower.contains("download") {
            continue;
        }

        let full_url = if href.starts_with("http") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("{}{}", base_url, href)
        } else {
            continue;
        };

        let ext_name = matched_ext.map(|e| &e[1..]).unwrap_or("data");
        dbg_debate!("[fetch] found data link: {} ({})", safe_truncate(&full_url, 100), ext_name);
        return Some(full_url);
    }
    None
}

/// Extract HTML tables as markdown-formatted text, preserving structure.
/// Returns extracted table text if tables with numeric data are found.
pub(crate) fn extract_html_tables(html: &str, max_tables: usize) -> Option<String> {
    let mut result = String::new();
    let mut tables_found = 0;
    let lower = html.to_lowercase();

    // Find <table> blocks
    let mut search_from = 0;
    while tables_found < max_tables {
        let table_start = match lower[search_from..].find("<table") {
            Some(pos) => search_from + pos,
            None => break,
        };
        let table_end = match lower[table_start..].find("</table>") {
            Some(pos) => table_start + pos + 8,
            None => break,
        };
        search_from = table_end;

        let table_html = &html[table_start..table_end];
        // Skip tiny tables (likely navigation/layout, not data)
        if table_html.len() < 50 { continue; }

        // Simple extraction: strip tags, preserve rows
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut in_cell = false;
        let mut cell_content = String::new();
        let table_lower = table_html.to_lowercase();

        let mut i = 0;
        let chars: Vec<char> = table_html.chars().collect();
        while i < chars.len() {
            if i + 3 < chars.len() && &table_lower[i..i+3] == "<tr" {
                current_row = Vec::new();
                // Skip to end of tag
                while i < chars.len() && chars[i] != '>' { i += 1; }
            } else if i + 4 < chars.len() && &table_lower[i..i+4] == "</tr" {
                if !current_row.is_empty() { rows.push(current_row.clone()); }
                while i < chars.len() && chars[i] != '>' { i += 1; }
            } else if i + 3 < chars.len() && (&table_lower[i..i+3] == "<td" || &table_lower[i..i+3] == "<th") {
                in_cell = true;
                cell_content.clear();
                while i < chars.len() && chars[i] != '>' { i += 1; }
            } else if i + 4 < chars.len() && (&table_lower[i..i+4] == "</td" || &table_lower[i..i+4] == "</th") {
                in_cell = false;
                let clean = cell_content.trim().replace('\n', " ");
                let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
                current_row.push(clean);
                while i < chars.len() && chars[i] != '>' { i += 1; }
            } else if in_cell {
                if chars[i] == '<' {
                    // Skip inner tags
                    while i < chars.len() && chars[i] != '>' { i += 1; }
                } else {
                    cell_content.push(chars[i]);
                }
            }
            i += 1;
        }

        if rows.len() >= 2 {
            tables_found += 1;
            result.push_str(&format!("\n[Table {}]\n", tables_found));
            for (ri, row) in rows.iter().enumerate() {
                result.push_str("| ");
                result.push_str(&row.join(" | "));
                result.push_str(" |\n");
                // Add markdown header separator after first row
                if ri == 0 {
                    result.push_str("|");
                    for _ in row {
                        result.push_str(" --- |");
                    }
                    result.push('\n');
                }
            }
        }
    }

    if tables_found > 0 {
        dbg_debate!("[fetch] extracted {} tables ({} chars)", tables_found, result.len());
        Some(result)
    } else {
        None
    }
}

/// Fetched article with extracted text and original mime type.
struct FetchedArticle {
    text: String,
    mime_type: String,
}

/// Fetch article content from a URL using dom_smoothie (Mozilla Readability).
/// For large pages: extracts tables and scans for data download links first.
/// Returns extracted text + detected mime type, or None on failure.
async fn fetch_article_content(client: &reqwest::Client, url: &str, blocked_domains: &[String]) -> Option<FetchedArticle> {
    let url_short = safe_truncate(url, 80);
    let t0 = std::time::Instant::now();

    // Skip known-blocked domains (user config or defaults)
    if let Some(domain) = url.split('/').nth(2) {
        if blocked_domains.iter().any(|d| domain.contains(d.as_str())) {
            dbg_debate!("[fetch] SKIP blocked domain {} | {}", domain, url_short);
            return None;
        }
    }

    let fetch_url = if url.contains("reddit.com/") && !url.contains("old.reddit.com") {
        url.replace("www.reddit.com", "old.reddit.com")
            .replace("reddit.com", "old.reddit.com")
    } else {
        url.to_string()
    };

    dbg_debate!("[fetch] >> START {}", url_short);
    let resp = match client.get(&fetch_url)
        .timeout(std::time::Duration::from_secs(15))
        .header("User-Agent", "Mozilla/5.0 (compatible; engram/1.1)")
        .send().await
    {
        Ok(r) => r,
        Err(e) => {
            dbg_debate!("[fetch] FAIL {:.1}s send error: {} | {}", t0.elapsed().as_secs_f32(), e, url_short);
            return None;
        }
    };
    dbg_debate!("[fetch] HTTP {} in {:.1}s | {}", resp.status(), t0.elapsed().as_secs_f32(), url_short);
    if !resp.status().is_success() { return None; }

    // Detect content type from headers
    let detected_mime = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/html")
        .split(';').next().unwrap_or("text/html")
        .trim().to_lowercase();

    // PDF extraction: extract text and find relevant paragraphs
    let is_pdf = detected_mime.contains("application/pdf")
        || url.to_lowercase().ends_with(".pdf");
    if is_pdf {
        #[cfg(feature = "pdf")]
        {
            return extract_pdf_text(resp, url_short).await
                .map(|text| FetchedArticle { text, mime_type: "application/pdf".into() });
        }
        #[cfg(not(feature = "pdf"))]
        {
            dbg_debate!("[fetch] SKIP PDF (pdf feature not enabled) | {}", url_short);
            return None;
        }
    }

    let html = match resp.text().await {
        Ok(h) => h,
        Err(e) => {
            dbg_debate!("[fetch] FAIL {:.1}s body read error: {} | {}", t0.elapsed().as_secs_f32(), e, url_short);
            return None;
        }
    };
    dbg_debate!("[fetch] body {} bytes in {:.1}s | {}", html.len(), t0.elapsed().as_secs_f32(), url_short);
    if html.len() < 200 { return None; }

    // Always scan for machine-readable data links first (CSV, JSON, XML)
    // These are more valuable than any HTML extraction
    let base_origin = url.split('/').take(3).collect::<Vec<_>>().join("/");
    if let Some(data_url) = find_data_download_link(&html, &base_origin) {
        dbg_debate!("[fetch] >> fetching data link: {}", safe_truncate(&data_url, 80));
        if let Ok(data_resp) = client.get(&data_url)
            .timeout(std::time::Duration::from_secs(10))
            .header("User-Agent", "Mozilla/5.0 (compatible; engram/1.1)")
            .send().await
        {
            if data_resp.status().is_success() {
                if let Ok(data_text) = data_resp.text().await {
                    let capped = safe_truncate(&data_text, 8000);
                    dbg_debate!("[fetch] << data link OK {} chars | {}", capped.len(), safe_truncate(&data_url, 60));
                    return Some(FetchedArticle { text: capped.to_string(), mime_type: detected_mime.clone() });
                }
            }
        }
    }

    // Timeout for all blocking parse operations (table extraction, readability)
    const PARSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

    // For large pages (>500KB): extract tables + readability fallback
    if html.len() > 500_000 {
        dbg_debate!("[fetch] large page ({} bytes), trying table extraction | {}", html.len(), url_short);

        // Extract HTML tables — run in spawn_blocking with timeout
        let html_for_tables = safe_truncate(&html, 2_000_000).to_string();
        let table_result = tokio::time::timeout(PARSE_TIMEOUT, tokio::task::spawn_blocking(move || {
            extract_html_tables(&html_for_tables, 5)
        })).await;
        if let Ok(Ok(Some(tables))) = table_result {
            if tables.len() > 200 {
                let capped = safe_truncate(&tables, 6000);
                dbg_debate!("[fetch] << DONE {:.1}s result=TABLES ({} chars) | {}", t0.elapsed().as_secs_f32(), capped.len(), url_short);
                return Some(FetchedArticle { text: capped.to_string(), mime_type: detected_mime.clone() });
            }
        } else if table_result.is_err() {
            dbg_debate!("[fetch] table extraction TIMED OUT after {}s | {}", PARSE_TIMEOUT.as_secs(), url_short);
        }

        // Last resort for large pages: take first 500KB through readability with timeout
        dbg_debate!("[fetch] large page: no tables/data links, trying readability on first 500KB | {}", url_short);
        let html_capped = safe_truncate(&html, 500_000).to_string();
        let parse_result = tokio::time::timeout(PARSE_TIMEOUT, tokio::task::spawn_blocking(move || {
            let mut readability = dom_smoothie::Readability::new(html_capped, None, None).ok()?;
            let article = readability.parse().ok()?;
            let text = article.text_content.to_string().trim().to_string();
            if text.len() > 100 { Some(text) } else { None }
        })).await;
        let parse_result = match parse_result {
            Ok(r) => r.ok().flatten(),
            Err(_) => { dbg_debate!("[fetch] readability TIMED OUT after {}s | {}", PARSE_TIMEOUT.as_secs(), url_short); None }
        };
        dbg_debate!("[fetch] << DONE {:.1}s result={} | {}", t0.elapsed().as_secs_f32(), if parse_result.is_some() { "OK" } else { "EMPTY" }, url_short);
        return parse_result.map(|text| FetchedArticle { text, mime_type: detected_mime.clone() });
    }

    // Normal path for pages < 500KB: readability extraction with timeout
    let parse_result = tokio::time::timeout(PARSE_TIMEOUT, tokio::task::spawn_blocking(move || {
        let t_parse = std::time::Instant::now();
        let mut readability = dom_smoothie::Readability::new(html, None, None).ok()?;
        let article = readability.parse().ok()?;
        let text = article.text_content.to_string().trim().to_string();
        dbg_debate!("[fetch] parsed {} chars in {:.1}s", text.len(), t_parse.elapsed().as_secs_f32());
        if text.len() > 100 { Some(text) } else { None }
    })).await;
    let parse_result = match parse_result {
        Ok(r) => r.ok().flatten(),
        Err(_) => { dbg_debate!("[fetch] readability TIMED OUT after {}s | {}", PARSE_TIMEOUT.as_secs(), url_short); None }
    };

    dbg_debate!("[fetch] << DONE {:.1}s result={} | {}", t0.elapsed().as_secs_f32(), if parse_result.is_some() { "OK" } else { "EMPTY" }, url_short);
    parse_result.map(|text| FetchedArticle { text, mime_type: detected_mime })
}

/// Run the full ingest pipeline on content.
#[cfg(feature = "ingest")]
async fn run_ingest(state: &AppState, source_name: &str, content: &str) -> (u32, u32) {
    use engram_ingest::types::{RawItem, Content};

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled,
         user_entity_labels, auto_label_threshold) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.kb_endpoints.clone(), cfg.ner_model.clone(), cfg.rel_model.clone(),
         cfg.relation_templates.clone(), cfg.rel_threshold, cfg.coreference_enabled,
         cfg.ner_entity_labels.clone().unwrap_or_default(),
         cfg.ner_auto_label_threshold.unwrap_or(3))
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

    #[cfg(feature = "gliner2")]
    let entity_labels = {
        let g = graph.read().unwrap();
        super::super::ingest::resolve_entity_labels(&g, &user_entity_labels, auto_label_threshold)
    };
    #[cfg(not(feature = "gliner2"))]
    let entity_labels: Vec<String> = Vec::new();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::task::spawn_blocking(move || {
            let config = engram_ingest::PipelineConfig {
                create_documents: false,
                ..Default::default()
            };
            let mut pipeline = super::super::ingest::build_pipeline(
                graph, config, kb_endpoints, ner_model, rel_model,
                relation_templates, rel_threshold, coreference_enabled,
                cached_ner, cached_rel, entity_labels,
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
        })
    ).await;

    match result {
        Ok(r) => r.unwrap_or((0, 0)),
        Err(_) => {
            tracing::warn!("debate ingest TIMED OUT after 60s");
            (0, 0)
        }
    }
}

#[cfg(not(feature = "ingest"))]
async fn run_ingest(_state: &AppState, _source: &str, _content: &str) -> (u32, u32) {
    (0, 0)
}

/// Create a pending Document node in the graph from fetched article content.
/// Saves metadata (URL, title, mime, content_hash) with `ner_complete: false`.
/// Uses `try_write` to avoid blocking the async runtime.
fn create_pending_document_node(
    graph: &std::sync::Arc<std::sync::RwLock<engram_core::graph::Graph>>,
    content_hash_hex: &str,
    url: Option<&str>,
    title: Option<&str>,
    mime_type: &str,
    content_length: usize,
    language: Option<&str>,
) {
    let short = if content_hash_hex.len() >= 8 { &content_hash_hex[..8] } else { content_hash_hex };
    let doc_label = format!("Doc:{short}");
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let Ok(mut g) = graph.try_write() else {
        eprintln!("[debate] WARN: could not acquire graph write lock for Document node {}", doc_label);
        return;
    };

    // Skip if already exists (same content hash = same doc label)
    if g.find_node_id(&doc_label).ok().flatten().is_some() {
        return;
    }

    let prov = engram_core::graph::Provenance {
        source_type: engram_core::graph::SourceType::Derived,
        source_id: "debate-fetch".to_string(),
    };
    if g.store_with_confidence(&doc_label, 0.80, &prov).is_err() {
        return;
    }
    let _ = g.set_node_type(&doc_label, "Document");
    let _ = g.set_property(&doc_label, "content_hash", content_hash_hex);
    let _ = g.set_property(&doc_label, "mime_type", mime_type);
    let _ = g.set_property(&doc_label, "fetched_at", &now_ts.to_string());
    let _ = g.set_property(&doc_label, "content_length", &content_length.to_string());
    let _ = g.set_property(&doc_label, "ner_complete", "false");
    if let Some(u) = url {
        if !u.is_empty() {
            let _ = g.set_property(&doc_label, "url", u);
        }
    }
    if let Some(t) = title {
        if !t.is_empty() {
            let _ = g.set_property(&doc_label, "title", t);
        }
    }
    if let Some(lang) = language {
        if !lang.is_empty() {
            let _ = g.set_property(&doc_label, "language", lang);
        }
    }
}
