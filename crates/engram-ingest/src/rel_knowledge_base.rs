/// Knowledge Base relation extractor: SPARQL endpoint lookups.
///
/// Bootstraps empty graphs by querying external knowledge bases (Wikidata,
/// DBpedia, etc.) for entity links and relations. This extractor goes FIRST
/// in the relation chain so that subsequent backends (gazetteer, KGE) can
/// learn from the seeded data.
///
/// Design:
/// - Uses `reqwest::blocking::Client` (consistent with other extractors using blocking_read)
/// - SPARQL responses parsed as JSON (`application/sparql-results+json`)
/// - Graph property caching: stores `kb_id:{endpoint}` on matched nodes
/// - Timeout: 10s per SPARQL query
/// - Rate limited by `max_lookups_per_call`

use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::time::Instant;

use engram_core::graph::Graph;

use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
use crate::types::{ExtractionMethod, KbStats};

/// Configuration for a single KB endpoint (mirrors state.rs KbEndpointConfig).
#[derive(Clone, Debug)]
pub struct KbEndpoint {
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub auth_header: Option<String>,
    pub entity_link_template: Option<String>,
    pub relation_query_template: Option<String>,
    pub max_lookups: u32,
}

/// SPARQL-based knowledge base relation extractor.
pub struct KbRelationExtractor {
    endpoints: Vec<KbEndpoint>,
    client: reqwest::blocking::Client,
    graph: Arc<RwLock<Graph>>,
    /// Stats from the last extraction run.
    last_stats: Mutex<Option<KbStats>>,
    /// LLM endpoint for area-of-interest detection.
    llm_endpoint: Option<String>,
    /// LLM model name.
    llm_model: Option<String>,
    /// Event bus for streaming seed enrichment events via SSE.
    event_bus: Option<Arc<engram_core::events::EventBus>>,
    /// Web search provider: "searxng", "brave", "duckduckgo"
    web_search_provider: Option<String>,
    /// Web search API key (for Brave Search)
    web_search_api_key: Option<String>,
    /// Web search URL (for SearXNG self-hosted)
    web_search_url: Option<String>,
}

impl KbRelationExtractor {
    pub fn new(endpoints: Vec<KbEndpoint>, graph: Arc<RwLock<Graph>>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            endpoints,
            client,
            graph,
            last_stats: Mutex::new(None),
            llm_endpoint: None,
            llm_model: None,
            event_bus: None,
            web_search_provider: None,
            web_search_api_key: None,
            web_search_url: None,
        }
    }

    /// Create with LLM, event bus, and web search configuration (for interactive seed flow).
    pub fn with_config(
        endpoints: Vec<KbEndpoint>,
        graph: Arc<RwLock<Graph>>,
        llm_endpoint: Option<String>,
        llm_model: Option<String>,
        event_bus: Option<Arc<engram_core::events::EventBus>>,
        web_search_provider: Option<String>,
        web_search_api_key: Option<String>,
        web_search_url: Option<String>,
    ) -> Self {
        let mut ext = Self::new(endpoints, graph);
        ext.llm_endpoint = llm_endpoint;
        ext.llm_model = llm_model;
        ext.event_bus = event_bus;
        ext.web_search_provider = web_search_provider;
        ext.web_search_api_key = web_search_api_key;
        ext.web_search_url = web_search_url;
        ext
    }

    /// Search the web using the configured search provider.
    /// Returns a list of (title, snippet) pairs from search results.
    fn web_search(&self, query: &str) -> Vec<(String, String)> {
        let provider = self.web_search_provider.as_deref().unwrap_or("duckduckgo");
        let mut results = Vec::new();

        match provider {
            "searxng" => {
                let base = self.web_search_url.as_deref()
                    .unwrap_or("http://localhost:8090");
                let url = format!("{}/search", base);
                if let Ok(resp) = self.client.get(&url)
                    .query(&[("q", query), ("format", "json"), ("categories", "general")])
                    .header("Accept", "application/json")
                    .send()
                {
                    if let Ok(data) = resp.json::<serde_json::Value>() {
                        if let Some(arr) = data.get("results").and_then(|r| r.as_array()) {
                            for r in arr.iter().take(10) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let snippet = r.get("content").and_then(|c| c.as_str()).unwrap_or_default();
                                if !title.is_empty() || !snippet.is_empty() {
                                    results.push((title.to_string(), snippet.to_string()));
                                }
                            }
                        }
                    }
                }
            }
            "brave" => {
                let api_key = self.web_search_api_key.as_deref().unwrap_or_default();
                let url = "https://api.search.brave.com/res/v1/web/search";
                if let Ok(resp) = self.client.get(url)
                    .query(&[("q", query)])
                    .header("Accept", "application/json")
                    .header("X-Subscription-Token", api_key)
                    .send()
                {
                    if let Ok(data) = resp.json::<serde_json::Value>() {
                        if let Some(arr) = data.pointer("/web/results").and_then(|r| r.as_array()) {
                            for r in arr.iter().take(10) {
                                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
                                let snippet = r.get("description").and_then(|d| d.as_str()).unwrap_or_default();
                                if !title.is_empty() || !snippet.is_empty() {
                                    results.push((title.to_string(), snippet.to_string()));
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                // DuckDuckGo Instant Answer API (default)
                if let Ok(resp) = self.client.get("https://api.duckduckgo.com/")
                    .query(&[("q", query), ("format", "json"), ("no_html", "1"), ("skip_disambig", "1")])
                    .header("User-Agent", "engram/1.1")
                    .send()
                {
                    if let Ok(data) = resp.json::<serde_json::Value>() {
                        // Abstract
                        if let Some(abs) = data.get("AbstractText").and_then(|a| a.as_str()) {
                            if !abs.is_empty() {
                                let heading = data.get("Heading").and_then(|h| h.as_str()).unwrap_or_default();
                                results.push((heading.to_string(), abs.to_string()));
                            }
                        }
                        // Related topics
                        if let Some(topics) = data.get("RelatedTopics").and_then(|r| r.as_array()) {
                            for t in topics.iter().take(5) {
                                let text = t.get("Text").and_then(|x| x.as_str()).unwrap_or_default();
                                if !text.is_empty() {
                                    let title = text.split(" - ").next().unwrap_or(text);
                                    results.push((title.to_string(), text.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Execute a SPARQL query against an endpoint, returning parsed JSON results.
    fn sparql_query(
        &self,
        endpoint: &KbEndpoint,
        query: &str,
    ) -> Result<serde_json::Value, String> {
        let mut req = self
            .client
            .get(&endpoint.url)
            .query(&[("query", query), ("format", "json")])
            .header("Accept", "application/sparql-results+json")
            .header("User-Agent", "engram/1.1");

        // Add auth header if configured
        if let Some(ref header_val) = endpoint.auth_header {
            match endpoint.auth_type.as_str() {
                "bearer" => {
                    req = req.header("Authorization", format!("Bearer {header_val}"));
                }
                "basic" => {
                    req = req.header("Authorization", format!("Basic {header_val}"));
                }
                "api_key" => {
                    req = req.header("X-API-Key", header_val.as_str());
                }
                _ => {}
            }
        }

        let resp = req.send().map_err(|e| format!("SPARQL request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("SPARQL query returned {}", resp.status()));
        }

        resp.json::<serde_json::Value>()
            .map_err(|e| format!("SPARQL JSON parse failed: {e}"))
    }

    /// Link an entity to a Wikidata QID using Wikipedia search for disambiguation.
    ///
    /// Two-step approach:
    /// 1. Wikipedia search: "{entity_text} {entity_type}" → finds Wikipedia page title
    /// 2. Wikipedia page props → extracts wikibase_item (QID)
    ///
    /// Wikipedia's search naturally disambiguates using context -- "Putin person"
    /// finds Vladimir Putin, not the 2024 film.
    ///
    /// For non-Wikidata endpoints, falls back to configured SPARQL template.
    /// Returns (QID_URI, canonical_name) -- canonical_name is the Wikipedia page title.
    fn entity_link(
        &self,
        endpoint: &KbEndpoint,
        entity_label: &str,
        entity_type: &str,
        language: &str,
    ) -> Option<(String, String)> {
        // Custom endpoint: use configured template or SPARQL
        if endpoint.entity_link_template.is_some() || !endpoint.url.contains("wikidata") {
            let query = if let Some(ref template) = endpoint.entity_link_template {
                template
                    .replace("{entity_label}", &sparql_escape(entity_label))
                    .replace("{language}", language)
            } else {
                format!(
                    r#"SELECT ?item ?itemLabel WHERE {{
                        ?item rdfs:label "{label}"@{lang} .
                        SERVICE wikibase:label {{ bd:serviceParam wikibase:language "{lang},en" }}
                    }} LIMIT 5"#,
                    label = sparql_escape(entity_label),
                    lang = language,
                )
            };
            return match self.sparql_query(endpoint, &query) {
                Ok(json) => extract_first_uri(&json).map(|uri| (uri, entity_label.to_string())),
                Err(_) => None,
            };
        }

        // Wikidata: use Wikipedia search API for robust disambiguation
        let wiki_lang = match language {
            "de" => "de", "fr" => "fr", "es" => "es", "it" => "it",
            "pt" => "pt", "nl" => "nl", "ru" => "ru", "zh" => "zh",
            _ => "en",
        };

        // Search entity label alone first (Wikipedia ranking usually returns most famous)
        if let Some(result) = self.wikipedia_search_qid(wiki_lang, entity_label) {
            return Some(result);
        }

        // Fallback: add entity type as context for disambiguation
        let search_term = format!("{} {}", entity_label, entity_type);
        self.wikipedia_search_qid(wiki_lang, &search_term)
    }

    /// Search Wikipedia and extract the Wikidata QID + canonical title from the top result.
    /// Returns (QID_URI, canonical_page_title).
    fn wikipedia_search_qid(&self, wiki_lang: &str, search_term: &str) -> Option<(String, String)> {
        // Step 1: Wikipedia search
        let resp = self.client
            .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", search_term),
                ("srlimit", "1"),
                ("format", "json"),
            ])
            .header("User-Agent", "engram/1.1")
            .send()
            .ok()?;

        let json: serde_json::Value = resp.json().ok()?;
        let title = json.pointer("/query/search/0/title")?.as_str()?.to_string();

        // Step 2: Get Wikidata QID from page props
        let resp2 = self.client
            .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
            .query(&[
                ("action", "query"),
                ("titles", &title),
                ("prop", "pageprops"),
                ("format", "json"),
            ])
            .header("User-Agent", "engram/1.1")
            .send()
            .ok()?;

        let json2: serde_json::Value = resp2.json().ok()?;
        let pages = json2.pointer("/query/pages")?.as_object()?;

        for page in pages.values() {
            if let Some(qid) = page.pointer("/pageprops/wikibase_item").and_then(|v| v.as_str()) {
                return Some((format!("http://www.wikidata.org/entity/{}", qid), title));
            }
        }

        None
    }

    /// Detect area of interest from seed text using LLM or heuristic fallback.
    pub fn detect_area_of_interest(&self, text: &str, entities: &[String]) -> String {
        // Try LLM first
        if let (Some(endpoint), Some(model)) = (&self.llm_endpoint, &self.llm_model) {
            let body = serde_json::json!({
                "model": model,
                "messages": [{
                    "role": "user",
                    "content": format!(
                        "What is the primary area of interest or domain topic in this text? Reply with ONLY a short topic phrase (2-6 words), nothing else.\n\nText: {text}"
                    )
                }],
                "temperature": 0.1,
                "max_tokens": 50
            });

            if let Ok(resp) = self.client
                .post(endpoint)
                .json(&body)
                .timeout(std::time::Duration::from_secs(15))
                .send()
            {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    if let Some(content) = json
                        .pointer("/choices/0/message/content")
                        .and_then(|v| v.as_str())
                    {
                        let topic = content.trim().trim_matches('"').trim();
                        if !topic.is_empty() && topic.len() < 100 {
                            return topic.to_string();
                        }
                    }
                }
            }
        }

        // Heuristic fallback: parse "I track/monitor/analyze/study {topic}"
        let lower = text.to_lowercase();
        for prefix in &["i track ", "i monitor ", "i analyze ", "i study ", "i'm researching ", "i'm focused on ", "focused on "] {
            if let Some(idx) = lower.find(prefix) {
                let rest = &text[idx + prefix.len()..];
                let end = rest.find('.').unwrap_or_else(|| rest.len().min(80));
                let topic = rest[..end].trim();
                if !topic.is_empty() {
                    return topic.to_string();
                }
            }
        }

        // Last resort: first 3 entity labels
        entities.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
    }

    /// Fetch the Wikipedia article extract for the area of interest.
    /// Returns up to 5000 chars of article text.
    pub fn fetch_area_of_interest_article(&self, area: &str, lang: &str) -> Option<String> {
        let wiki_lang = match lang {
            "de" => "de", "fr" => "fr", "es" => "es", "it" => "it",
            "pt" => "pt", "nl" => "nl", "ru" => "ru", "zh" => "zh",
            _ => "en",
        };

        // Search Wikipedia for the area of interest
        let resp = self.client
            .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", area),
                ("srlimit", "1"),
                ("format", "json"),
            ])
            .header("User-Agent", "engram/1.1")
            .send()
            .ok()?;

        let json: serde_json::Value = resp.json().ok()?;
        let title = json.pointer("/query/search/0/title")?.as_str()?.to_string();

        // Fetch full article extract
        let resp2 = self.client
            .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
            .query(&[
                ("action", "query"),
                ("titles", &title),
                ("prop", "extracts"),
                ("explaintext", "true"),
                ("exchars", "5000"),
                ("format", "json"),
            ])
            .header("User-Agent", "engram/1.1")
            .send()
            .ok()?;

        let json2: serde_json::Value = resp2.json().ok()?;
        let pages = json2.pointer("/query/pages")?.as_object()?;

        for page in pages.values() {
            if let Some(extract) = page.get("extract").and_then(|v| v.as_str()) {
                if !extract.is_empty() && extract.len() > 50 {
                    return Some(extract.to_string());
                }
            }
        }

        None
    }

    /// Find co-occurrences of seed entities within paragraphs of an article.
    /// Returns pairs of entity indices that appear in the same paragraph.
    pub fn find_cooccurrences(&self, article: &str, entity_labels: &[String]) -> Vec<(usize, usize)> {
        let paragraphs: Vec<&str> = article.split("\n\n").collect();
        let mut pairs = std::collections::HashSet::new();

        for paragraph in &paragraphs {
            let para_lower = paragraph.to_lowercase();
            let mut found: Vec<usize> = Vec::new();

            for (idx, label) in entity_labels.iter().enumerate() {
                if para_lower.contains(&label.to_lowercase()) {
                    found.push(idx);
                }
            }

            // Generate all pairs from entities found in this paragraph
            for i in 0..found.len() {
                for j in (i + 1)..found.len() {
                    let a = found[i].min(found[j]);
                    let b = found[i].max(found[j]);
                    pairs.insert((a, b));
                }
            }
        }

        pairs.into_iter().collect()
    }

    /// For entities not mentioned in the main AoI article, search Wikipedia
    /// for "{entity} {area}" and check for co-occurrences with other seed entities.
    pub fn search_unmentioned_entities(
        &self,
        unmentioned: &[usize],
        all_labels: &[String],
        area: &str,
        lang: &str,
    ) -> Vec<(usize, usize)> {
        let wiki_lang = match lang {
            "de" => "de", "fr" => "fr", "es" => "es", "it" => "it",
            "pt" => "pt", "nl" => "nl", "ru" => "ru", "zh" => "zh",
            _ => "en",
        };

        let mut pairs = Vec::new();

        for &entity_idx in unmentioned {
            let search_term = format!("{} {}", all_labels[entity_idx], area);

            // Fetch article about this entity in context
            let resp = match self.client
                .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
                .query(&[
                    ("action", "query"),
                    ("list", "search"),
                    ("srsearch", &search_term),
                    ("srlimit", "1"),
                    ("format", "json"),
                ])
                .header("User-Agent", "engram/1.1")
                .send()
            {
                Ok(r) => r,
                Err(_) => continue,
            };

            let json: serde_json::Value = match resp.json() {
                Ok(j) => j,
                Err(_) => continue,
            };

            let title = match json.pointer("/query/search/0/title").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => continue,
            };

            // Fetch article extract
            let resp2 = match self.client
                .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
                .query(&[
                    ("action", "query"),
                    ("titles", &title),
                    ("prop", "extracts"),
                    ("explaintext", "true"),
                    ("exchars", "3000"),
                    ("format", "json"),
                ])
                .header("User-Agent", "engram/1.1")
                .send()
            {
                Ok(r) => r,
                Err(_) => continue,
            };

            let json2: serde_json::Value = match resp2.json() {
                Ok(j) => j,
                Err(_) => continue,
            };

            if let Some(pages) = json2.pointer("/query/pages").and_then(|v| v.as_object()) {
                for page in pages.values() {
                    if let Some(extract) = page.get("extract").and_then(|v| v.as_str()) {
                        let extract_lower = extract.to_lowercase();
                        // Check which OTHER seed entities appear in this article
                        for (other_idx, other_label) in all_labels.iter().enumerate() {
                            if other_idx == entity_idx { continue; }
                            if extract_lower.contains(&other_label.to_lowercase()) {
                                let a = entity_idx.min(other_idx);
                                let b = entity_idx.max(other_idx);
                                pairs.push((a, b));
                            }
                        }
                    }
                }
            }
        }

        pairs
    }

    /// Publish a seed enrichment event if event_bus is configured.
    fn emit(&self, event: engram_core::events::GraphEvent) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(event);
        }
    }

    /// Batch relation discovery: find ALL direct connections between a set of QIDs
    /// in a single SPARQL query.
    fn batch_relation_lookup(
        &self,
        endpoint: &KbEndpoint,
        qids: &[(&str, usize)], // (QID, entity_index)
        language: &str,
    ) -> Vec<(usize, usize, String, String)> {
        if qids.len() < 2 {
            return Vec::new();
        }

        // Build VALUES clause: wd:Q7747 wd:Q159 wd:Q212 ...
        let values: String = qids.iter()
            .map(|(qid, _)| format!("wd:{}", extract_qid(qid)))
            .collect::<Vec<_>>()
            .join(" ");

        let query = format!(
            r#"SELECT ?s ?sLabel ?p ?pLabel ?o ?oLabel WHERE {{
                VALUES ?s {{ {values} }}
                VALUES ?o {{ {values} }}
                ?s ?prop ?o .
                ?p wikibase:directClaim ?prop .
                FILTER(?s != ?o)
                SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
            }} LIMIT 500"#,
            values = values,
            lang = language,
        );

        let json = match self.sparql_query(endpoint, &query) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("batch SPARQL failed: {e}");
                return Vec::new();
            }
        };

        // Build QID → entity_index map
        let qid_to_idx: HashMap<String, usize> = qids.iter()
            .map(|(qid, idx)| (extract_qid(qid).to_string(), *idx))
            .collect();

        let mut results = Vec::new();
        if let Some(bindings) = json.pointer("/results/bindings").and_then(|b| b.as_array()) {
            for binding in bindings {
                let s_uri = binding.pointer("/s/value").and_then(|v| v.as_str()).unwrap_or("");
                let o_uri = binding.pointer("/o/value").and_then(|v| v.as_str()).unwrap_or("");
                let p_label = binding.pointer("/pLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let p_uri = binding.pointer("/p/value").and_then(|v| v.as_str()).unwrap_or("");

                let s_qid = extract_qid(s_uri);
                let o_qid = extract_qid(o_uri);

                if let (Some(&s_idx), Some(&o_idx)) = (qid_to_idx.get(s_qid), qid_to_idx.get(o_qid)) {
                    let rel_type = if p_label.is_empty() { uri_to_label(p_uri) } else { p_label.to_string() };
                    results.push((s_idx, o_idx, rel_type, p_uri.to_string()));
                }
            }
        }

        results
    }

    /// Property expansion: fetch key Wikidata properties for all linked entities.
    /// Discovers NEW entities (e.g., "Lockheed Martin" as manufacturer of HIMARS)
    /// and returns them as (entity_label, property_label, new_entity_label) triples.
    /// Returns (from_label, rel_type, to_label, valid_from, valid_to) tuples.
    fn property_expansion(
        &self,
        endpoint: &KbEndpoint,
        qids: &[(&str, usize)], // (QID URI, entity_index)
        entity_labels: &[String],
        language: &str,
    ) -> Vec<(String, String, String, Option<String>, Option<String>)> {
        if qids.is_empty() {
            return Vec::new();
        }

        let values: String = qids.iter()
            .map(|(qid, _)| format!("wd:{}", extract_qid(qid)))
            .collect::<Vec<_>>()
            .join(" ");

        // Key Wikidata properties that discover interesting connected entities.
        // Also fetches P580 (start time) and P582 (end time) temporal qualifiers
        // via statement nodes when available.
        let query = format!(
            r#"SELECT ?entity ?entityLabel ?propLabel ?value ?valueLabel ?startTime ?endTime WHERE {{
                VALUES ?entity {{ {values} }}
                VALUES ?propNode {{ wd:P39 wd:P27 wd:P17 wd:P159 wd:P176 wd:P495 wd:P36 }}
                ?propNode wikibase:claim ?propclaim .
                ?propNode wikibase:statementProperty ?stmtprop .
                ?entity ?propclaim ?stmt .
                ?stmt ?stmtprop ?value .
                FILTER(isIRI(?value))
                OPTIONAL {{ ?stmt pq:P580 ?startTime . }}
                OPTIONAL {{ ?stmt pq:P582 ?endTime . }}
                SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
            }} LIMIT 300"#,
            values = values,
            lang = language,
        );

        let json = match self.sparql_query(endpoint, &query) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        // Build QID → entity label map
        let qid_to_label: HashMap<String, String> = qids.iter()
            .map(|(qid, idx)| (extract_qid(qid).to_string(), entity_labels[*idx].clone()))
            .collect();

        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(bindings) = json.pointer("/results/bindings").and_then(|b| b.as_array()) {
            for binding in bindings {
                let entity_qid = binding.pointer("/entity/value").and_then(|v| v.as_str()).unwrap_or("");
                let prop_label = binding.pointer("/propLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let value_label = binding.pointer("/valueLabel/value").and_then(|v| v.as_str()).unwrap_or("");

                // Skip self-references and empty values
                if value_label.is_empty() || value_label.starts_with("http://") {
                    continue;
                }

                let entity_label = match qid_to_label.get(extract_qid(entity_qid)) {
                    Some(l) => l.clone(),
                    None => continue,
                };

                // Deduplicate
                let key = (entity_label.clone(), value_label.to_string());
                if seen.contains(&key) { continue; }
                seen.insert(key);

                // Extract temporal qualifiers
                let start_time = binding.pointer("/startTime/value")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect::<String>()); // YYYY-MM-DD
                let end_time = binding.pointer("/endTime/value")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect::<String>()); // YYYY-MM-DD

                // Map Wikidata property URIs to readable relation types
                let rel_type = wikidata_prop_to_rel_type(prop_label);

                results.push((entity_label, rel_type, value_label.to_string(), start_time, end_time));
            }
        }

        results
    }

    /// Batch shortest path: find 1-hop intermediate entities between ALL entity pairs
    /// in a single SPARQL query. Returns (from_label, rel_type, to_label) triples
    /// including the intermediate node.
    fn batch_shortest_paths(
        &self,
        endpoint: &KbEndpoint,
        qids: &[(&str, usize)],
        entity_labels: &[String],
        connected_pairs: &std::collections::HashSet<(usize, usize)>,
        language: &str,
    ) -> Vec<(String, String, String)> {
        if qids.len() < 2 {
            return Vec::new();
        }

        let values: String = qids.iter()
            .map(|(qid, _)| format!("wd:{}", extract_qid(qid)))
            .collect::<Vec<_>>()
            .join(" ");

        let qid_to_idx: HashMap<String, usize> = qids.iter()
            .map(|(qid, idx)| (extract_qid(qid).to_string(), *idx))
            .collect();

        // 1-hop: A → ?mid → B (single SPARQL for ALL pairs)
        let query = format!(
            r#"SELECT ?s ?o ?mid ?midLabel ?p1Label ?p2Label WHERE {{
                VALUES ?s {{ {values} }}
                VALUES ?o {{ {values} }}
                ?s ?prop1 ?mid . ?mid ?prop2 ?o .
                ?p1 wikibase:directClaim ?prop1 .
                ?p2 wikibase:directClaim ?prop2 .
                FILTER(?s != ?o && isIRI(?mid))
                SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
            }} LIMIT 500"#,
            values = values, lang = language,
        );

        let json = match self.sparql_query(endpoint, &query) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        let mut newly_connected = std::collections::HashSet::new();

        if let Some(bindings) = json.pointer("/results/bindings").and_then(|b| b.as_array()) {
            for binding in bindings {
                let s_uri = binding.pointer("/s/value").and_then(|v| v.as_str()).unwrap_or("");
                let o_uri = binding.pointer("/o/value").and_then(|v| v.as_str()).unwrap_or("");
                let mid_label = binding.pointer("/midLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let p1_label = binding.pointer("/p1Label/value").and_then(|v| v.as_str()).unwrap_or("");
                let p2_label = binding.pointer("/p2Label/value").and_then(|v| v.as_str()).unwrap_or("");

                if mid_label.is_empty() || mid_label.starts_with("http://") {
                    continue;
                }

                let s_qid = extract_qid(s_uri);
                let o_qid = extract_qid(o_uri);

                let (s_idx, o_idx) = match (qid_to_idx.get(s_qid), qid_to_idx.get(o_qid)) {
                    (Some(&s), Some(&o)) => (s, o),
                    _ => continue,
                };

                // Only add paths for pairs not already connected
                let pair = (s_idx.min(o_idx), s_idx.max(o_idx));
                if connected_pairs.contains(&pair) || newly_connected.contains(&pair) {
                    continue;
                }
                newly_connected.insert(pair);

                let s_label = &entity_labels[s_idx];
                let o_label = &entity_labels[o_idx];
                let r1 = wikidata_prop_to_rel_type(p1_label);
                let r2 = wikidata_prop_to_rel_type(p2_label);

                results.push((s_label.clone(), r1, mid_label.to_string()));
                results.push((mid_label.to_string(), r2, o_label.clone()));
            }
        }

        results
    }

}

impl RelationExtractor for KbRelationExtractor {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        if self.endpoints.is_empty() || input.entities.len() < 2 {
            return Vec::new();
        }

        let start = Instant::now();
        let mut all_relations = Vec::new();
        let mut total_stats = KbStats::default();
        let entity_labels: Vec<String> = input.entities.iter().map(|e| e.text.clone()).collect();
        let session_id: Arc<str> = Arc::from("pipeline");

        // ─── Step 0: Detect area of interest ───
        let area_of_interest = input.area_of_interest.clone().unwrap_or_else(|| {
            self.detect_area_of_interest(&input.text, &entity_labels)
        });

        tracing::info!(area = %area_of_interest, "Step 0: area of interest detected");
        self.emit(engram_core::events::GraphEvent::SeedAoiDetected {
            session_id: session_id.clone(),
            area_of_interest: Arc::from(area_of_interest.as_str()),
        });

        for endpoint in &self.endpoints {
            let mut budget = endpoint.max_lookups;
            let mut entity_kb_ids: HashMap<usize, String> = HashMap::new();
            let mut stats = KbStats {
                endpoint: endpoint.name.clone(),
                ..Default::default()
            };

            // ─── Step 1: Entity linking via Wikipedia ───
            let prop_key = format!("kb_id:{}", endpoint.name);
            {
                let g = self.graph.read().unwrap();
                for (idx, entity) in input.entities.iter().enumerate() {
                    if let Ok(Some(kb_id)) = g.get_property(&entity.text, &prop_key) {
                        entity_kb_ids.insert(idx, kb_id);
                        stats.entities_linked += 1;
                    }
                }
            }

            let mut canonical_names: HashMap<usize, String> = HashMap::new();

            for (idx, entity) in input.entities.iter().enumerate() {
                if entity_kb_ids.contains_key(&idx) || budget == 0 {
                    continue;
                }

                budget -= 1;
                match self.entity_link(endpoint, &entity.text, &entity.entity_type, &input.language) {
                    Some((ref kb_id, ref canonical)) => {
                        entity_kb_ids.insert(idx, kb_id.clone());
                        canonical_names.insert(idx, canonical.clone());
                        stats.entities_linked += 1;

                        let qid_str = extract_qid(kb_id);
                        self.emit(engram_core::events::GraphEvent::SeedEntityLinked {
                            session_id: session_id.clone(),
                            label: Arc::from(entity.text.as_str()),
                            canonical: Arc::from(canonical.as_str()),
                            description: Arc::from(""),
                            qid: Arc::from(qid_str),
                        });
                    }
                    None => {
                        stats.entities_not_found += 1;
                    }
                }
            }

            // Cache KB IDs and canonical names as graph properties
            {
                if let Ok(mut g) = self.graph.write() {
                    for (idx, kb_id) in &entity_kb_ids {
                        let ner_label = &input.entities[*idx].text;
                        let _ = g.set_property(ner_label, &prop_key, kb_id);

                        if let Some(canonical) = canonical_names.get(idx) {
                            if canonical != ner_label {
                                let _ = g.set_property(ner_label, "canonical_name", canonical);
                            }
                        }
                    }
                }
            }

            self.emit(engram_core::events::GraphEvent::SeedPhaseComplete {
                session_id: session_id.clone(),
                phase: 1,
                entities_processed: stats.entities_linked,
                relations_found: 0,
            });

            // ─── Step 2: Area-of-interest article co-occurrence ───
            // Fetch the AoI article and find which seed entities co-occur in paragraphs
            let mut cooccurrence_relations = 0u32;

            if let Some(article) = self.fetch_area_of_interest_article(&area_of_interest, &input.language) {
                let cooccurrences = self.find_cooccurrences(&article, &entity_labels);
                for (a, b) in &cooccurrences {
                    all_relations.push(CandidateRelation {
                        head_idx: *a,
                        tail_idx: *b,
                        rel_type: "related_to".to_string(),
                        confidence: 0.60,
                        method: ExtractionMethod::KnowledgeBase,
                    });
                    cooccurrence_relations += 1;

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[*a].as_str()),
                        to: Arc::from(entity_labels[*b].as_str()),
                        rel_type: Arc::from("related_to"),
                        source: Arc::from("area_of_interest_article"),
                    });
                }

                // Step 2b: Search for entities not mentioned in the main article
                let mentioned: std::collections::HashSet<usize> = cooccurrences.iter()
                    .flat_map(|(a, b)| vec![*a, *b])
                    .collect();
                let unmentioned: Vec<usize> = (0..entity_labels.len())
                    .filter(|i| !mentioned.contains(i))
                    .collect();

                if !unmentioned.is_empty() {
                    let extra_pairs = self.search_unmentioned_entities(
                        &unmentioned, &entity_labels, &area_of_interest, &input.language,
                    );
                    for (a, b) in &extra_pairs {
                        all_relations.push(CandidateRelation {
                            head_idx: *a,
                            tail_idx: *b,
                            rel_type: "related_to".to_string(),
                            confidence: 0.55,
                            method: ExtractionMethod::KnowledgeBase,
                        });
                        cooccurrence_relations += 1;

                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                            session_id: session_id.clone(),
                            from: Arc::from(entity_labels[*a].as_str()),
                            to: Arc::from(entity_labels[*b].as_str()),
                            rel_type: Arc::from("related_to"),
                            source: Arc::from("entity_context_search"),
                        });
                    }
                }
            }

            // ─── Step 2c: Web search fallback for still-unconnected entities ───
            if self.web_search_provider.is_some() {
                let connected_so_far: std::collections::HashSet<usize> = all_relations.iter()
                    .flat_map(|r| vec![r.head_idx, r.tail_idx])
                    .collect();
                let still_unconnected: Vec<usize> = (0..entity_labels.len())
                    .filter(|i| !connected_so_far.contains(i))
                    .collect();

                if !still_unconnected.is_empty() {
                    tracing::info!(
                        unconnected = still_unconnected.len(),
                        total = entity_labels.len(),
                        "Step 2c: web search fallback for unconnected entities"
                    );

                    for &uidx in &still_unconnected {
                        let search_query = format!("{} {}", &entity_labels[uidx], &area_of_interest);
                        let web_results = self.web_search(&search_query);

                        // Scan snippets for mentions of other seed entities
                        for (_title, snippet) in &web_results {
                            let snippet_lower = snippet.to_lowercase();
                            for (oidx, other_label) in entity_labels.iter().enumerate() {
                                if oidx == uidx { continue; }
                                if snippet_lower.contains(&other_label.to_lowercase()) {
                                    // Avoid duplicate relations
                                    let already = all_relations.iter().any(|r|
                                        (r.head_idx == uidx && r.tail_idx == oidx) ||
                                        (r.head_idx == oidx && r.tail_idx == uidx)
                                    );
                                    if !already {
                                        all_relations.push(CandidateRelation {
                                            head_idx: uidx,
                                            tail_idx: oidx,
                                            rel_type: "related_to".to_string(),
                                            confidence: 0.50,
                                            method: ExtractionMethod::KnowledgeBase,
                                        });
                                        cooccurrence_relations += 1;

                                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                                            session_id: session_id.clone(),
                                            from: Arc::from(entity_labels[uidx].as_str()),
                                            to: Arc::from(entity_labels[oidx].as_str()),
                                            rel_type: Arc::from("related_to"),
                                            source: Arc::from("web_search"),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.emit(engram_core::events::GraphEvent::SeedPhaseComplete {
                session_id: session_id.clone(),
                phase: 2,
                entities_processed: entity_labels.len() as u32,
                relations_found: cooccurrence_relations,
            });

            // ─── Step 3: Batch SPARQL + property expansion + shortest path ───
            let qids: Vec<(&str, usize)> = entity_kb_ids.iter()
                .map(|(idx, qid)| (qid.as_str(), *idx))
                .collect();

            if qids.len() >= 2 {
                // 3a: Batch direct relation lookup
                let batch_results = self.batch_relation_lookup(endpoint, &qids, &input.language);
                for (s_idx, o_idx, rel_type, _rel_uri) in &batch_results {
                    all_relations.push(CandidateRelation {
                        head_idx: *s_idx,
                        tail_idx: *o_idx,
                        rel_type: rel_type.clone(),
                        confidence: 0.80,
                        method: ExtractionMethod::KnowledgeBase,
                    });
                    stats.relations_found += 1;

                    self.emit(engram_core::events::GraphEvent::SeedSparqlRelation {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[*s_idx].as_str()),
                        to: Arc::from(entity_labels[*o_idx].as_str()),
                        rel_type: Arc::from(rel_type.as_str()),
                    });
                }

                // 3b: Property expansion — discover NEW entities
                {
                    let expansion = self.property_expansion(endpoint, &qids, &entity_labels, &input.language);

                    if !expansion.is_empty() {
                        let provenance = engram_core::graph::Provenance {
                            source_type: engram_core::graph::SourceType::Api,
                            source_id: format!("kb:{}", endpoint.name),
                        };

                        if let Ok(mut g) = self.graph.write() {
                            for (from_label, rel_type, to_label, _valid_from, _valid_to) in &expansion {
                                let _ = g.store_with_confidence(to_label, 0.70, &provenance);

                                let node_type = match rel_type.as_str() {
                                    "citizen_of" | "located_in" | "origin_country" | "capital_of" => "location",
                                    "headquartered_in" => "location",
                                    "manufactured_by" => "organization",
                                    "holds_position" => "position",
                                    "governed_by" => "person",
                                    _ => "entity",
                                };
                                let _ = g.set_node_type(to_label, node_type);

                                if g.find_node_id(from_label).ok().flatten().is_none() {
                                    let _ = g.store_with_confidence(from_label, 0.70, &provenance);
                                }

                                match g.relate(from_label, to_label, rel_type, &provenance) {
                                    Ok(_) => stats.relations_found += 1,
                                    Err(e) => {
                                        eprintln!("[KB] relate FAILED: {} -[{}]-> {}: {}", from_label, rel_type, to_label, e);
                                    }
                                }

                                self.emit(engram_core::events::GraphEvent::SeedSparqlRelation {
                                    session_id: session_id.clone(),
                                    from: Arc::from(from_label.as_str()),
                                    to: Arc::from(to_label.as_str()),
                                    rel_type: Arc::from(rel_type.as_str()),
                                });
                            }
                        }
                    }
                }

                // 3c: Shortest path discovery — 1-hop intermediate entities
                {
                    let connected: std::collections::HashSet<(usize, usize)> = all_relations.iter()
                        .map(|r| (r.head_idx.min(r.tail_idx), r.head_idx.max(r.tail_idx)))
                        .collect();

                    let path_results = self.batch_shortest_paths(
                        endpoint, &qids, &entity_labels, &connected, &input.language,
                    );

                    if !path_results.is_empty() {
                        let provenance = engram_core::graph::Provenance {
                            source_type: engram_core::graph::SourceType::Api,
                            source_id: format!("kb:{}", endpoint.name),
                        };

                        if let Ok(mut g) = self.graph.write() {
                            for (from_label, rel_type, to_label) in &path_results {
                                let _ = g.store_with_confidence(to_label, 0.65, &provenance);
                                let _ = g.store_with_confidence(from_label, 0.65, &provenance);

                                match g.relate(from_label, to_label, rel_type, &provenance) {
                                    Ok(_) => stats.relations_found += 1,
                                    Err(_) => {}
                                }
                            }
                        }
                    }
                }
            }

            stats.lookup_ms = start.elapsed().as_millis() as u64;

            // Merge into total stats
            total_stats.endpoint = if total_stats.endpoint.is_empty() {
                stats.endpoint.clone()
            } else {
                format!("{}, {}", total_stats.endpoint, stats.endpoint)
            };
            total_stats.entities_linked += stats.entities_linked;
            total_stats.entities_not_found += stats.entities_not_found;
            total_stats.relations_found += stats.relations_found;
            total_stats.errors += stats.errors;

            self.emit(engram_core::events::GraphEvent::SeedPhaseComplete {
                session_id: session_id.clone(),
                phase: 3,
                entities_processed: stats.entities_linked,
                relations_found: stats.relations_found,
            });

            tracing::info!(
                endpoint = %endpoint.name,
                area = %area_of_interest,
                linked = stats.entities_linked,
                not_found = stats.entities_not_found,
                relations = stats.relations_found,
                ms = stats.lookup_ms,
                "KB relation extraction complete (4-step)"
            );
        }

        // ─── Step 4: Fix disconnected islands ───
        // Any seed entity with NO edge to any other seed entity gets a
        // "related_to" edge back to the area_of_interest context. This ensures
        // the graph has no orphaned subgraphs from the same seed text.
        {
            let connected_entities: std::collections::HashSet<usize> = all_relations.iter()
                .flat_map(|r| vec![r.head_idx, r.tail_idx])
                .collect();

            let disconnected: Vec<usize> = (0..entity_labels.len())
                .filter(|i| !connected_entities.contains(i))
                .collect();

            if !disconnected.is_empty() {
                // Find the most-connected seed entity to anchor disconnected ones
                let mut connection_counts = vec![0usize; entity_labels.len()];
                for r in &all_relations {
                    connection_counts[r.head_idx] += 1;
                    connection_counts[r.tail_idx] += 1;
                }
                let anchor_idx = connection_counts.iter().enumerate()
                    .max_by_key(|(_, c)| *c)
                    .map(|(i, _)| i)
                    .unwrap_or(0);

                tracing::info!(
                    disconnected = disconnected.len(),
                    anchor = %entity_labels[anchor_idx],
                    "fixing disconnected seed entities"
                );

                for &idx in &disconnected {
                    if idx == anchor_idx { continue; }
                    all_relations.push(CandidateRelation {
                        head_idx: idx,
                        tail_idx: anchor_idx,
                        rel_type: "related_to".to_string(),
                        confidence: 0.45,
                        method: ExtractionMethod::KnowledgeBase,
                    });

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[idx].as_str()),
                        to: Arc::from(entity_labels[anchor_idx].as_str()),
                        rel_type: Arc::from("related_to"),
                        source: Arc::from("co-extracted_seed_text"),
                    });
                }
            }
        }

        total_stats.lookup_ms = start.elapsed().as_millis() as u64;
        *self.last_stats.lock().unwrap() = Some(total_stats);

        all_relations
    }

    fn name(&self) -> &str {
        "knowledge-base"
    }

    fn stats(&self) -> Option<KbStats> {
        self.last_stats.lock().unwrap().clone()
    }
}

// ── Helpers ──

/// Escape special characters for SPARQL string literals.
fn sparql_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Extract the QID from a Wikidata URI (e.g. "http://www.wikidata.org/entity/Q42" -> "Q42").
fn extract_qid(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

/// Extract the first URI from a SPARQL results JSON.
fn extract_first_uri(json: &serde_json::Value) -> Option<String> {
    json.get("results")?
        .get("bindings")?
        .as_array()?
        .first()?
        .get("item")?
        .get("value")?
        .as_str()
        .map(|s| s.to_string())
}

/// Extract relation URI/label pairs from SPARQL results JSON.
fn extract_relations_from_sparql(json: &serde_json::Value) -> Vec<(String, String)> {
    let bindings = match json
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
    {
        Some(b) => b,
        None => return Vec::new(),
    };

    bindings
        .iter()
        .filter_map(|binding| {
            let uri = binding
                .get("prop")
                .and_then(|p| p.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let label = binding
                .get("propLabel")
                .and_then(|p| p.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if uri.is_empty() && label.is_empty() {
                None
            } else {
                Some((uri, label))
            }
        })
        .collect()
}

/// Convert a URI to a human-readable label (last path segment, snake_case).
fn uri_to_label(uri: &str) -> String {
    let segment = uri.rsplit('/').next().unwrap_or(uri);
    // Convert camelCase or PascalCase to snake_case
    let mut result = String::new();
    for (i, ch) in segment.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Map Wikidata property labels to engram relation types.
fn wikidata_prop_to_rel_type(prop_label: &str) -> String {
    // prop_label comes from Wikidata SERVICE wikibase:label, often as URI paths
    let label = if prop_label.contains('/') {
        uri_to_label(prop_label)
    } else {
        prop_label.to_lowercase().replace(' ', "_")
    };

    match label.as_str() {
        "p39" | "position_held" => "holds_position".to_string(),
        "p27" | "country_of_citizenship" => "citizen_of".to_string(),
        "p17" | "country" => "located_in".to_string(),
        "p159" | "headquarters_location" => "headquartered_in".to_string(),
        "p176" | "manufacturer" => "manufactured_by".to_string(),
        "p495" | "country_of_origin" => "origin_country".to_string(),
        "p36" | "capital" => "capital_of".to_string(),
        "p6" | "head_of_government" => "governed_by".to_string(),
        _ => label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparql_escape_handles_special_chars() {
        assert_eq!(sparql_escape(r#"foo"bar"#), r#"foo\"bar"#);
        assert_eq!(sparql_escape("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn extract_qid_from_uri() {
        assert_eq!(
            extract_qid("http://www.wikidata.org/entity/Q42"),
            "Q42"
        );
        assert_eq!(extract_qid("Q42"), "Q42");
    }

    #[test]
    fn uri_to_label_converts_camel_case() {
        assert_eq!(
            uri_to_label("http://www.wikidata.org/prop/direct/P31"),
            "p31"
        );
        assert_eq!(
            uri_to_label("http://schema.org/birthPlace"),
            "birth_place"
        );
    }

    #[test]
    fn extract_first_uri_from_sparql_json() {
        let json = serde_json::json!({
            "results": {
                "bindings": [{
                    "item": { "type": "uri", "value": "http://www.wikidata.org/entity/Q42" },
                    "itemLabel": { "type": "literal", "value": "Douglas Adams" }
                }]
            }
        });
        assert_eq!(
            extract_first_uri(&json),
            Some("http://www.wikidata.org/entity/Q42".to_string())
        );
    }

    #[test]
    fn extract_first_uri_empty_bindings() {
        let json = serde_json::json!({
            "results": { "bindings": [] }
        });
        assert_eq!(extract_first_uri(&json), None);
    }

    #[test]
    fn extract_relations_from_sparql_json() {
        let json = serde_json::json!({
            "results": {
                "bindings": [
                    {
                        "prop": { "type": "uri", "value": "http://www.wikidata.org/entity/P108" },
                        "propLabel": { "type": "literal", "value": "employer" }
                    },
                    {
                        "prop": { "type": "uri", "value": "http://www.wikidata.org/entity/P27" },
                        "propLabel": { "type": "literal", "value": "country of citizenship" }
                    }
                ]
            }
        });
        let rels = extract_relations_from_sparql(&json);
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].1, "employer");
        assert_eq!(rels[1].1, "country of citizenship");
    }
}
