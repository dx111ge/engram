//! Entity linking: Wikipedia/Wikidata lookups, area-of-interest detection,
//! co-occurrence analysis, and web search fallback.

use super::*;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

impl KbRelationExtractor {
    /// Fetch full article content from a URL using HTTP GET + dom_smoothie extraction.
    /// Returns (article_text, title, fetch_status).
    pub(super) fn fetch_url_content(&self, url: &str) -> (Option<String>, Option<String>, &'static str) {
        if url.is_empty() {
            return (None, None, "no_url");
        }

        // URL rewriting (reddit -> old.reddit for plain HTML)
        let fetch_url = if url.contains("reddit.com/") && !url.contains("old.reddit.com") {
            url.replace("www.reddit.com", "old.reddit.com")
                .replace("reddit.com", "old.reddit.com")
        } else {
            url.to_string()
        };

        // Fetch with browser UA
        let resp = match self.client
            .get(&fetch_url)
            .header("User-Agent", BROWSER_UA)
            .timeout(std::time::Duration::from_secs(15))
            .send()
        {
            Ok(r) => r,
            Err(_) => return (None, None, "timeout"),
        };

        let status = resp.status().as_u16();
        let html = match resp.text() {
            Ok(t) => t,
            Err(_) => return (None, None, "body_error"),
        };

        // Detect paywall / blocking
        if status == 402 || status == 403 || status == 451 {
            return (None, None, "paywalled");
        }
        if html.len() < 1000 {
            return (None, None, "js_required");
        }

        // Extract article text using dom_smoothie (Mozilla Readability)
        let mut readability = match dom_smoothie::Readability::new(html, None, None) {
            Ok(r) => r,
            Err(_) => return (None, None, "parse_error"),
        };
        let article = match readability.parse() {
            Ok(a) => a,
            Err(_) => return (None, None, "extract_error"),
        };

        let text = article.text_content.to_string();
        let title = if article.title.is_empty() { None } else { Some(article.title) };

        // Quality check: if extracted text is very short, likely soft paywall
        if text.len() < 200 {
            return (None, title, "soft_paywall");
        }

        // Truncate to 50KB max
        let text = if text.len() > 50_000 {
            text[..50_000].to_string()
        } else {
            text
        };

        (Some(text), title, "fetched")
    }
    /// Search the web using the configured search provider.
    /// Returns results with title, snippet, and URL for document provenance.
    pub(super) fn web_search(&self, query: &str) -> Vec<WebSearchResult> {
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
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                if !title.is_empty() || !snippet.is_empty() {
                                    results.push(WebSearchResult {
                                        title: title.to_string(),
                                        snippet: snippet.to_string(),
                                        url: url.to_string(),
                                    });
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
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or_default();
                                if !title.is_empty() || !snippet.is_empty() {
                                    results.push(WebSearchResult {
                                        title: title.to_string(),
                                        snippet: snippet.to_string(),
                                        url: url.to_string(),
                                    });
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
                        let abs_url = data.get("AbstractURL").and_then(|u| u.as_str()).unwrap_or_default();
                        if let Some(abs) = data.get("AbstractText").and_then(|a| a.as_str()) {
                            if !abs.is_empty() {
                                let heading = data.get("Heading").and_then(|h| h.as_str()).unwrap_or_default();
                                results.push(WebSearchResult {
                                    title: heading.to_string(),
                                    snippet: abs.to_string(),
                                    url: abs_url.to_string(),
                                });
                            }
                        }
                        if let Some(topics) = data.get("RelatedTopics").and_then(|r| r.as_array()) {
                            for t in topics.iter().take(5) {
                                let text = t.get("Text").and_then(|x| x.as_str()).unwrap_or_default();
                                let first_url = t.get("FirstURL").and_then(|u| u.as_str()).unwrap_or_default();
                                if !text.is_empty() {
                                    let title = text.split(" - ").next().unwrap_or(text);
                                    results.push(WebSearchResult {
                                        title: title.to_string(),
                                        snippet: text.to_string(),
                                        url: first_url.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Store web search results as Document nodes in the graph and cache content in DocStore.
    /// Fetches full article content from URLs for better fact extraction.
    /// Creates: Document -> Publisher edges, and Fact -> Document for each entity mentioned.
    pub(super) fn store_web_search_documents(
        &self,
        results: &[WebSearchResult],
        entity_labels: &[String],
    ) {
        if results.is_empty() {
            return;
        }

        // Phase 1: Fetch full article content in parallel (before acquiring graph lock)
        let total_urls = results.len() as u32;
        let session_id = self.session_id.clone().unwrap_or_default();
        let event_bus = self.event_bus.clone();

        let fetched_articles: Vec<(Option<String>, &'static str)> = std::thread::scope(|s| {
            let handles: Vec<_> = results.iter().enumerate().map(|(idx, result)| {
                let client = &self.client;
                let sid = session_id.clone();
                let bus = event_bus.clone();
                s.spawn(move || {
                    if result.url.is_empty() || result.snippet.is_empty() {
                        return (None, "no_url");
                    }

                    // Emit "fetching" event
                    if let Some(ref bus) = bus {
                        bus.publish(engram_core::events::GraphEvent::SeedArticleProgress {
                            session_id: std::sync::Arc::from(sid.as_str()),
                            current: idx as u32 + 1,
                            total: total_urls,
                            url: std::sync::Arc::from(result.url.as_str()),
                            status: std::sync::Arc::from("fetching"),
                            chars: 0,
                        });
                    }

                    let fetch_url = if result.url.contains("reddit.com/") && !result.url.contains("old.reddit.com") {
                        result.url.replace("www.reddit.com", "old.reddit.com")
                            .replace("reddit.com", "old.reddit.com")
                    } else {
                        result.url.clone()
                    };

                    let resp = match client
                        .get(&fetch_url)
                        .header("User-Agent", BROWSER_UA)
                        .timeout(std::time::Duration::from_secs(15))
                        .send()
                    {
                        Ok(r) => r,
                        Err(_) => return (None, "timeout"),
                    };

                    let status = resp.status().as_u16();
                    let html = match resp.text() {
                        Ok(t) => t,
                        Err(_) => return (None, "body_error"),
                    };

                    if status == 402 || status == 403 || status == 451 {
                        if let Some(ref bus) = bus {
                            bus.publish(engram_core::events::GraphEvent::SeedArticleProgress {
                                session_id: std::sync::Arc::from(sid.as_str()),
                                current: idx as u32 + 1, total: total_urls,
                                url: std::sync::Arc::from(result.url.as_str()),
                                status: std::sync::Arc::from("paywalled"), chars: 0,
                            });
                        }
                        return (None, "paywalled");
                    }
                    if html.len() < 1000 {
                        return (None, "js_required");
                    }

                    let mut readability = match dom_smoothie::Readability::new(html, None, None) {
                        Ok(r) => r,
                        Err(_) => return (None, "parse_error"),
                    };
                    let article = match readability.parse() {
                        Ok(a) => a,
                        Err(_) => return (None, "extract_error"),
                    };

                    let text = article.text_content.to_string();
                    if text.len() < 200 {
                        return (None, "soft_paywall");
                    }

                    let text = if text.len() > 50_000 { text[..50_000].to_string() } else { text };

                    // Emit "fetched" event
                    if let Some(ref bus) = bus {
                        bus.publish(engram_core::events::GraphEvent::SeedArticleProgress {
                            session_id: std::sync::Arc::from(sid.as_str()),
                            current: idx as u32 + 1, total: total_urls,
                            url: std::sync::Arc::from(result.url.as_str()),
                            status: std::sync::Arc::from("fetched"),
                            chars: text.len() as u32,
                        });
                    }

                    tracing::info!(url = %result.url, chars = text.len(), "Fetched full article content");
                    (Some(text), "fetched")
                })
            }).collect();

            handles.into_iter().map(|h| h.join().unwrap_or((None, "thread_error"))).collect()
        });

        // Phase 2: Store documents in graph (with graph lock)
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Api,
            source_id: "web_search".to_string(),
        };
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut graph = match self.graph.write() {
            Ok(g) => g,
            Err(_) => return,
        };

        for (i, result) in results.iter().enumerate() {
            if result.snippet.is_empty() {
                continue;
            }
            let (ref article_text, fetch_status) = fetched_articles[i];

            // Use full article for content hash if available, else snippet
            let primary_content = article_text.as_deref().unwrap_or(&result.snippet);
            let content_hash = engram_core::storage::doc_store::DocStore::hash_content(
                primary_content.as_bytes(),
            );
            let hash_hex = engram_core::storage::doc_store::DocStore::hash_hex(&content_hash);
            let doc_label = crate::document::doc_label(&hash_hex);

            // Skip if Document already exists
            if graph.find_node_id(&doc_label).ok().flatten().is_some() {
                continue;
            }

            // Create Document node
            if graph.store_with_confidence(&doc_label, 0.60, &prov).is_err() {
                continue;
            }
            let _ = graph.set_node_type(&doc_label, "Document");
            let _ = graph.set_property(&doc_label, "content_hash", &hash_hex);
            let _ = graph.set_property(&doc_label, "title", &result.title);
            if !result.url.is_empty() {
                let _ = graph.set_property(&doc_label, "url", &result.url);
            }
            let _ = graph.set_property(&doc_label, "mime_type", "text/html");
            let _ = graph.set_property(&doc_label, "ingested_at", &now_ts.to_string());
            let _ = graph.set_property(&doc_label, "content_length",
                &primary_content.len().to_string());
            let _ = graph.set_property(&doc_label, "fetch_status", fetch_status);
            let content_type = if article_text.is_some() { "article" } else { "snippet" };
            let _ = graph.set_property(&doc_label, "content_type", content_type);

            // Create Publisher node from URL
            let publisher_label = if !result.url.is_empty() {
                let (stype, sid) = crate::learned_trust::extract_source_from_url(&result.url);
                format!("Source:{}:{}", stype, sid)
            } else {
                "Source:web:search".to_string()
            };
            if graph.find_node_id(&publisher_label).ok().flatten().is_none() {
                let _ = graph.store_with_confidence(&publisher_label, 0.50, &prov);
                let _ = graph.set_node_type(&publisher_label, "Source");
            }
            let _ = graph.relate_upsert(&doc_label, &publisher_label, "published_by", &prov);

            // Layer 1: Entity -> Document (mentioned_in) for each entity in content
            let search_text = primary_content.to_lowercase();
            for label in entity_labels {
                if search_text.contains(&label.to_lowercase()) {
                    let _ = graph.relate_upsert(label, &doc_label, "mentioned_in", &prov);
                }
            }

            // Cache content in DocStore
            if let Some(ref store_arc) = self.doc_store {
                if let Ok(mut store) = store_arc.write() {
                    let mime = engram_core::storage::doc_store::MimeType::Text;
                    let _ = store.store(primary_content.as_bytes(), mime);
                }
            }
        }

        // Drop graph lock before LLM calls
        drop(graph);

        // Layer 2: LLM fact extraction -- use full article text when available
        self.extract_facts_from_results(results, &fetched_articles, entity_labels);
    }

    /// Run LLM fact extraction on web search results and store Fact nodes.
    /// Uses full article text when available, falls back to snippet.
    fn extract_facts_from_results(
        &self,
        results: &[WebSearchResult],
        fetched_articles: &[(Option<String>, &'static str)],
        entity_labels: &[String],
    ) {
        let (endpoint, model) = match (&self.llm_endpoint, &self.llm_model) {
            (Some(ep), Some(m)) if !ep.is_empty() && !m.is_empty() => (ep.clone(), m.clone()),
            _ => return,
        };

        let total_docs = results.len() as u32;
        let session_id = self.session_id.clone().unwrap_or_default();
        let mut cumulative_facts = 0u32;

        for (i, result) in results.iter().enumerate() {
            if result.snippet.is_empty() {
                continue;
            }
            let (ref article_text, _fetch_status) = fetched_articles[i];

            // Emit fact extraction progress
            self.emit(engram_core::events::GraphEvent::SeedFactProgress {
                session_id: std::sync::Arc::from(session_id.as_str()),
                current: i as u32 + 1,
                total: total_docs,
                doc_title: std::sync::Arc::from(result.title.as_str()),
                facts_found: cumulative_facts,
            });

            // Use full article text if available, otherwise snippet
            let extract_text = article_text.as_deref().unwrap_or(&result.snippet);
            let is_full_article = article_text.is_some();

            // Configure extraction based on content type
            let config = crate::fact_extract::FactExtractConfig {
                llm_endpoint: endpoint.clone(),
                llm_model: model.clone(),
                gleaning: is_full_article, // enable gleaning for full articles
                max_tokens: if is_full_article { 2000 } else { 1000 },
                temperature: 0.1,
            };

            // Chunk the text (full articles may need multiple chunks)
            let chunks = crate::fact_extract::chunk_text(extract_text, 3000);

            let primary_content = article_text.as_deref().unwrap_or(&result.snippet);
            let content_hash = engram_core::storage::doc_store::DocStore::hash_content(
                primary_content.as_bytes(),
            );
            let hash_hex = engram_core::storage::doc_store::DocStore::hash_hex(&content_hash);
            let doc_label = crate::document::doc_label(&hash_hex);

            let mut all_claims = Vec::new();
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                let claims = crate::fact_extract::extract_claims(
                    &self.client, &config, chunk, entity_labels, chunk_idx,
                );
                all_claims.extend(claims);
            }

            if all_claims.is_empty() {
                continue;
            }

            if let Ok(mut graph) = self.graph.write() {
                let count = crate::fact_extract::store_facts_in_graph(
                    &mut graph, &all_claims, &doc_label, entity_labels,
                );
                if count > 0 {
                    let source = if is_full_article { "full article" } else { "snippet" };
                    tracing::info!(
                        doc = %doc_label,
                        facts = count,
                        source = source,
                        "LLM extracted facts from web search result"
                    );
                    cumulative_facts += count;
                    // Emit cumulative fact count update
                    self.emit(engram_core::events::GraphEvent::SeedFactProgress {
                        session_id: std::sync::Arc::from(session_id.as_str()),
                        current: i as u32 + 1,
                        total: total_docs,
                        doc_title: std::sync::Arc::from(result.title.as_str()),
                        facts_found: cumulative_facts,
                    });
                }
            }
        }
    }

    /// Link an entity to a Wikidata QID using Wikipedia search for disambiguation.
    ///
    /// Two-step approach:
    /// 1. Wikipedia search: "{entity_text} {entity_type}" -> finds Wikipedia page title
    /// 2. Wikipedia page props -> extracts wikibase_item (QID)
    ///
    /// Wikipedia's search naturally disambiguates using context -- "Putin person"
    /// finds Vladimir Putin, not the 2024 film.
    ///
    /// For non-Wikidata endpoints, falls back to configured SPARQL template.
    /// Returns (QID_URI, canonical_name) -- canonical_name is the Wikipedia page title.
    pub(super) fn entity_link(
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
    pub(super) fn wikipedia_search_qid(&self, wiki_lang: &str, search_term: &str) -> Option<(String, String)> {
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
}
