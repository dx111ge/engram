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
        }
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
    fn entity_link(
        &self,
        endpoint: &KbEndpoint,
        entity_label: &str,
        entity_type: &str,
        language: &str,
    ) -> Option<String> {
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
                Ok(json) => extract_first_uri(&json),
                Err(_) => None,
            };
        }

        // Wikidata: use Wikipedia search API for robust disambiguation
        let wiki_lang = match language {
            "de" => "de", "fr" => "fr", "es" => "es", "it" => "it",
            "pt" => "pt", "nl" => "nl", "ru" => "ru", "zh" => "zh",
            _ => "en",
        };

        // Search with entity_type as context for disambiguation
        let search_term = format!("{} {}", entity_label, entity_type);
        if let Some(qid) = self.wikipedia_search_qid(wiki_lang, &search_term) {
            return Some(qid);
        }

        // Fallback: search without type context
        self.wikipedia_search_qid(wiki_lang, entity_label)
    }

    /// Search Wikipedia and extract the Wikidata QID from the top result.
    fn wikipedia_search_qid(&self, wiki_lang: &str, search_term: &str) -> Option<String> {
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
        let title = json.pointer("/query/search/0/title")?.as_str()?;

        // Step 2: Get Wikidata QID from page props
        let resp2 = self.client
            .get(&format!("https://{}.wikipedia.org/w/api.php", wiki_lang))
            .query(&[
                ("action", "query"),
                ("titles", title),
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
                return Some(format!("http://www.wikidata.org/entity/{}", qid));
            }
        }

        None
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
    fn property_expansion(
        &self,
        endpoint: &KbEndpoint,
        qids: &[(&str, usize)], // (QID URI, entity_index)
        entity_labels: &[String],
        language: &str,
    ) -> Vec<(String, String, String)> {
        if qids.is_empty() {
            return Vec::new();
        }

        let values: String = qids.iter()
            .map(|(qid, _)| format!("wd:{}", extract_qid(qid)))
            .collect::<Vec<_>>()
            .join(" ");

        // Key Wikidata properties that discover interesting connected entities
        let query = format!(
            r#"SELECT ?entity ?entityLabel ?propLabel ?value ?valueLabel WHERE {{
                VALUES ?entity {{ {values} }}
                VALUES ?prop {{ wdt:P39 wdt:P27 wdt:P17 wdt:P159 wdt:P176 wdt:P495 wdt:P36 }}
                ?entity ?prop ?value .
                FILTER(isIRI(?value))
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

                // Skip if value is already one of our entities
                if entity_labels.contains(&value_label.to_string()) {
                    continue;
                }

                // Deduplicate
                let key = (entity_label.clone(), value_label.to_string());
                if seen.contains(&key) { continue; }
                seen.insert(key);

                // Map Wikidata property URIs to readable relation types
                let rel_type = wikidata_prop_to_rel_type(prop_label);

                results.push((entity_label, rel_type, value_label.to_string()));
            }
        }

        results
    }

    /// Look up relations between two KB entity IDs.
    fn relation_lookup(
        &self,
        endpoint: &KbEndpoint,
        id_a: &str,
        id_b: &str,
        language: &str,
    ) -> Vec<(String, String)> {
        let query = if let Some(ref template) = endpoint.relation_query_template {
            template
                .replace("{id_a}", &sparql_escape(id_a))
                .replace("{id_b}", &sparql_escape(id_b))
                .replace("{language}", language)
        } else {
            // Default Wikidata relation lookup
            // Extract just the QID from full URIs for wd: prefix
            let qa = extract_qid(id_a);
            let qb = extract_qid(id_b);
            format!(
                r#"SELECT ?prop ?propLabel WHERE {{
                    {{ wd:{qa} ?p wd:{qb} . ?prop wikibase:directClaim ?p . }}
                    UNION
                    {{ wd:{qb} ?p wd:{qa} . ?prop wikibase:directClaim ?p . }}
                    SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
                }}"#,
                qa = qa,
                qb = qb,
                lang = language,
            )
        };

        match self.sparql_query(endpoint, &query) {
            Ok(json) => extract_relations_from_sparql(&json),
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint.name,
                    id_a = id_a,
                    id_b = id_b,
                    error = %e,
                    "relation lookup failed"
                );
                Vec::new()
            }
        }
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

        for endpoint in &self.endpoints {
            let mut budget = endpoint.max_lookups;
            let mut entity_kb_ids: HashMap<usize, String> = HashMap::new();
            let mut stats = KbStats {
                endpoint: endpoint.name.clone(),
                ..Default::default()
            };

            // Phase 1: Entity linking — try to find KB IDs for each entity
            // First check graph properties for cached IDs
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

            // Query endpoint for entities not yet linked (uses wbsearchentities for Wikidata)
            for (idx, entity) in input.entities.iter().enumerate() {
                if entity_kb_ids.contains_key(&idx) || budget == 0 {
                    continue;
                }

                budget -= 1;
                match self.entity_link(endpoint, &entity.text, &entity.entity_type, &input.language) {
                    Some(ref kb_id) => {
                        entity_kb_ids.insert(idx, kb_id.clone());
                        stats.entities_linked += 1;
                    }
                    None => {
                        stats.entities_not_found += 1;
                    }
                }
            }

            // Cache newly discovered KB IDs back to graph properties
            {
                let labels_to_cache: Vec<(String, String)> = entity_kb_ids
                    .iter()
                    .map(|(idx, kb_id)| (input.entities[*idx].text.clone(), kb_id.clone()))
                    .collect();

                if !labels_to_cache.is_empty() {
                    if let Ok(mut g) = self.graph.write() {
                        for (label, kb_id) in labels_to_cache {
                            let _ = g.set_property(&label, &prop_key, &kb_id);
                        }
                    }
                }
            }

            // Phase 2: Batch relation discovery (single SPARQL query for ALL entity pairs)
            let qids: Vec<(&str, usize)> = entity_kb_ids.iter()
                .map(|(idx, qid)| (qid.as_str(), *idx))
                .collect();

            if qids.len() >= 2 {
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
                }

                // Phase 3: Pairwise deep lookup for linked pairs with no batch results
                let connected: std::collections::HashSet<(usize, usize)> = batch_results.iter()
                    .map(|(s, o, _, _)| (*s.min(o), *s.max(o)))
                    .collect();

                for i in 0..qids.len() {
                    for j in (i + 1)..qids.len() {
                        if budget == 0 { break; }
                        let (qi, idx_i) = qids[i];
                        let (qj, idx_j) = qids[j];
                        let pair = (idx_i.min(idx_j), idx_i.max(idx_j));
                        if connected.contains(&pair) { continue; }

                        budget -= 1;
                        let relations = self.relation_lookup(endpoint, qi, qj, &input.language);
                        for (rel_uri, rel_label) in &relations {
                            let rel_type = if rel_label.is_empty() {
                                uri_to_label(rel_uri)
                            } else {
                                rel_label.clone()
                            };
                            all_relations.push(CandidateRelation {
                                head_idx: idx_i,
                                tail_idx: idx_j,
                                rel_type,
                                confidence: 0.75,
                                method: ExtractionMethod::KnowledgeBase,
                            });
                            stats.relations_found += 1;
                        }
                    }
                }
            }

            // Phase 4: Property expansion — discover NEW entities from Wikidata properties.
            // E.g., Putin → position_held → "President of Russia", HIMARS → manufacturer → "Lockheed Martin"
            // Creates new nodes directly in the graph and returns relations to them.
            {
                let entity_labels: Vec<String> = input.entities.iter().map(|e| e.text.clone()).collect();
                let expansion = self.property_expansion(endpoint, &qids, &entity_labels, &input.language);

                if !expansion.is_empty() {
                    let provenance = engram_core::graph::Provenance {
                        source_type: engram_core::graph::SourceType::Api,
                        source_id: format!("kb:{}", endpoint.name),
                    };

                    if let Ok(mut g) = self.graph.write() {
                        for (from_label, rel_type, to_label) in &expansion {
                            // Create new node for the discovered entity
                            let _ = g.store_with_confidence(to_label, 0.70, &provenance);

                            // Create the edge
                            match g.relate(from_label, to_label, rel_type, &provenance) {
                                Ok(_) => stats.relations_found += 1,
                                Err(_) => {}
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

            tracing::info!(
                endpoint = %endpoint.name,
                linked = stats.entities_linked,
                not_found = stats.entities_not_found,
                relations = stats.relations_found,
                ms = stats.lookup_ms,
                "KB relation extraction complete"
            );
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
