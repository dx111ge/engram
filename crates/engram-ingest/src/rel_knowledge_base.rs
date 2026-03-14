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

    /// Search for an entity in a SPARQL endpoint, returning the KB ID if found.
    fn entity_link(
        &self,
        endpoint: &KbEndpoint,
        entity_label: &str,
        language: &str,
    ) -> Option<String> {
        let query = if let Some(ref template) = endpoint.entity_link_template {
            template
                .replace("{entity_label}", &sparql_escape(entity_label))
                .replace("{language}", language)
        } else {
            // Default Wikidata entity search
            format!(
                r#"SELECT ?item ?itemLabel WHERE {{
                    ?item rdfs:label "{label}"@{lang} .
                    SERVICE wikibase:label {{ bd:serviceParam wikibase:language "{lang},en" }}
                }} LIMIT 5"#,
                label = sparql_escape(entity_label),
                lang = language,
            )
        };

        match self.sparql_query(endpoint, &query) {
            Ok(json) => extract_first_uri(&json),
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint.name,
                    entity = entity_label,
                    error = %e,
                    "entity linking failed"
                );
                None
            }
        }
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

            // Query endpoint for entities not yet linked
            for (idx, entity) in input.entities.iter().enumerate() {
                if entity_kb_ids.contains_key(&idx) || budget == 0 {
                    continue;
                }

                budget -= 1;
                match self.entity_link(endpoint, &entity.text, &input.language) {
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

            // Phase 2: Relation lookup — for each entity pair with KB IDs
            for i in 0..input.entities.len() {
                for j in (i + 1)..input.entities.len() {
                    if budget == 0 {
                        break;
                    }

                    let (id_a, id_b) = match (entity_kb_ids.get(&i), entity_kb_ids.get(&j)) {
                        (Some(a), Some(b)) => (a.clone(), b.clone()),
                        _ => continue,
                    };

                    budget -= 1;
                    let relations =
                        self.relation_lookup(endpoint, &id_a, &id_b, &input.language);

                    for (rel_uri, rel_label) in &relations {
                        let rel_type = if rel_label.is_empty() {
                            uri_to_label(rel_uri)
                        } else {
                            rel_label.clone()
                        };

                        all_relations.push(CandidateRelation {
                            head_idx: i,
                            tail_idx: j,
                            rel_type,
                            confidence: 0.75,
                            method: ExtractionMethod::KnowledgeBase,
                        });
                        stats.relations_found += 1;
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
