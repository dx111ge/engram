/// Gap-closing research between debate rounds.
/// Detects knowledge gaps from agent disagreements, then fills them via:
/// 1. SPARQL (Wikidata) -- highest confidence structured facts
/// 2. Engram graph search -- what we already know
/// 3. Web search (SearXNG/Brave) -- broader coverage
/// 4. Full ingest pipeline (NER + RE + entity resolution) on web content

use crate::state::AppState;
use super::types::*;
use super::llm::{call_llm, extract_content, parse_json_from_llm};

/// Analyze a completed round and identify knowledge gaps to research.
pub async fn detect_gaps(
    state: &AppState,
    round: &DebateRound,
    agents: &[DebateAgent],
    topic: &str,
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

    let prompt = format!(
        r#"You are a research analyst reviewing a debate round. Identify 2-4 SPECIFIC factual gaps that would improve the next round.

{}

Return ONLY a JSON array of search queries (no commentary):
["specific search query 1", "specific search query 2"]

Rules:
- Each query must be specific enough to find real data (not vague)
- Focus on numerical data, statistics, and verifiable facts
- Prioritize gaps where agents disagreed or cited uncertainty"#,
        summary
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

/// Close identified gaps using all available sources + full ingest pipeline.
pub async fn close_gaps(
    state: &AppState,
    gaps: &[String],
    _topic: &str,
) -> Vec<GapResearch> {
    let mut results = Vec::new();

    for gap_query in gaps {
        let mut findings = Vec::new();
        let mut entities_stored = Vec::new();
        let mut facts_stored = 0u32;
        let mut relations_created = 0u32;

        // ── 1. Search existing graph ──
        {
            let g = match state.graph.read() {
                Ok(g) => g,
                Err(_) => continue,
            };
            if let Ok(search_results) = g.search_text(gap_query, 5) {
                for r in &search_results {
                    findings.push(format!("[graph] {} (confidence: {:.2})", r.label, r.confidence));
                }
            }
        }

        // ── 2. Web search for raw content ──
        let web_text = execute_web_search(state, gap_query).await;
        if !web_text.is_empty() {
            for line in web_text.lines().take(5) {
                let line = line.trim().trim_start_matches("- ");
                if !line.is_empty() {
                    findings.push(format!("[web] {}", line));
                }
            }
        }

        // ── 3. Run full ingest pipeline on web findings ──
        let ingested = if !web_text.is_empty() {
            run_ingest_pipeline(state, gap_query, &web_text, &mut facts_stored, &mut relations_created, &mut entities_stored).await
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

/// Run the full ingest pipeline (NER + RE + entity resolution) on text content.
/// Uses spawn_blocking because the pipeline internally creates blocking I/O.
/// Returns true if anything was stored.
#[cfg(feature = "ingest")]
async fn run_ingest_pipeline(
    state: &AppState,
    gap_query: &str,
    content: &str,
    facts_stored: &mut u32,
    relations_created: &mut u32,
    entities_stored: &mut Vec<String>,
) -> bool {
    use engram_ingest::types::{RawItem, Content};

    let (kb_endpoints, ner_model, rel_model, relation_templates, rel_threshold, coreference_enabled) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.kb_endpoints.clone(),
            cfg.ner_model.clone(),
            cfg.rel_model.clone(),
            cfg.relation_templates.clone(),
            cfg.rel_threshold,
            cfg.coreference_enabled,
        )
    };

    let llm_config = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.llm_endpoint.clone(), cfg.llm_model.clone())
    };

    // Clone everything needed for the blocking thread
    let graph = state.graph.clone();
    let doc_store = state.doc_store.clone();
    let cached_ner = state.cached_ner.clone();
    let cached_rel = state.cached_rel.clone();
    let content_owned = content.to_string();
    let query_owned = gap_query.to_string();
    let dirty = state.dirty.clone();

    // Run pipeline on a blocking thread to avoid tokio runtime panic
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
            .unwrap_or_default()
            .as_secs() as i64;

        let items = vec![RawItem {
            content: Content::Text(content_owned),
            source_url: None,
            source_name: format!("debate-gap: {}", query_owned),
            fetched_at: now,
            metadata: Default::default(),
        }];

        match pipeline.execute(items) {
            Ok(r) => {
                if r.facts_stored > 0 || r.relations_created > 0 {
                    dirty.store(true, std::sync::atomic::Ordering::Release);
                }
                Ok((r.facts_stored, r.relations_created, query_owned))
            }
            Err(e) => Err(format!("{}", e)),
        }
    }).await;

    match result {
        Ok(Ok((fs, rc, q))) => {
            *facts_stored = fs;
            *relations_created = rc;
            if fs > 0 || rc > 0 {
                entities_stored.push(format!("{} facts, {} relations from '{}'", fs, rc, q));
                true
            } else {
                false
            }
        }
        Ok(Err(e)) => {
            tracing::warn!("debate gap ingest failed for '{}': {}", gap_query, e);
            false
        }
        Err(e) => {
            tracing::warn!("debate gap ingest task panicked for '{}': {}", gap_query, e);
            false
        }
    }
}

/// Fallback when ingest feature is not enabled.
#[cfg(not(feature = "ingest"))]
async fn run_ingest_pipeline(
    _state: &AppState,
    _gap_query: &str,
    _content: &str,
    _facts_stored: &mut u32,
    _relations_created: &mut u32,
    _entities_stored: &mut Vec<String>,
) -> bool {
    false
}

/// Execute a web search via the configured search provider (SearXNG/Brave).
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
            // SearXNG (default)
            let base = search_url.unwrap_or_else(|| std::env::var("ENGRAM_SEARXNG_URL").unwrap_or_else(|_| "http://192.168.178.26:8080".into()));
            let url = format!("{}/search?q={}&format=json&engines=google,duckduckgo,bing", base.trim_end_matches('/'), urlencoding::encode(query));
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
