//! Entity linking: Wikipedia/Wikidata lookups, area-of-interest detection,
//! co-occurrence analysis, and web search fallback.

use super::*;

impl KbRelationExtractor {
    /// Search the web using the configured search provider.
    /// Returns a list of (title, snippet) pairs from search results.
    pub(super) fn web_search(&self, query: &str) -> Vec<(String, String)> {
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
