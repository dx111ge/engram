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

mod entity_link;
mod sparql;
#[cfg(test)]
mod tests;

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

    /// Publish a seed enrichment event if event_bus is configured.
    fn emit(&self, event: engram_core::events::GraphEvent) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(event);
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
        let entity_labels: Vec<String> = input.entities.iter().map(|e| e.text.clone()).collect();
        let session_id: Arc<str> = Arc::from("pipeline");

        // --- Step 0: Detect area of interest ---
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

            // --- Step 1: Entity linking via Wikipedia ---
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

            // --- Step 2: Area-of-interest article co-occurrence ---
            // Fetch the AoI article and find which seed entities co-occur in paragraphs
            let mut cooccurrence_relations = 0u32;

            if let Some(article) = self.fetch_area_of_interest_article(&area_of_interest, &input.language) {
                let cooccurrences = self.find_cooccurrences(&article, &entity_labels);
                for (a, b) in &cooccurrences {
                    let inferred = crate::rel_type_templates::infer_from_types(
                        &input.entities[*a].entity_type,
                        &input.entities[*b].entity_type,
                    );
                    all_relations.push(CandidateRelation {
                        head_idx: *a,
                        tail_idx: *b,
                        rel_type: inferred.clone(),
                        confidence: 0.60,
                        method: ExtractionMethod::KnowledgeBase,
                    });
                    cooccurrence_relations += 1;

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[*a].as_str()),
                        to: Arc::from(entity_labels[*b].as_str()),
                        rel_type: Arc::from(inferred.as_str()),
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
                        let inferred = crate::rel_type_templates::infer_from_types(
                            &input.entities[*a].entity_type,
                            &input.entities[*b].entity_type,
                        );
                        all_relations.push(CandidateRelation {
                            head_idx: *a,
                            tail_idx: *b,
                            rel_type: inferred.clone(),
                            confidence: 0.55,
                            method: ExtractionMethod::KnowledgeBase,
                        });
                        cooccurrence_relations += 1;

                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                            session_id: session_id.clone(),
                            from: Arc::from(entity_labels[*a].as_str()),
                            to: Arc::from(entity_labels[*b].as_str()),
                            rel_type: Arc::from(inferred.as_str()),
                            source: Arc::from("entity_context_search"),
                        });
                    }
                }
            }

            // --- Step 2c: Web search fallback for still-unconnected entities ---
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
                                        let inferred = crate::rel_type_templates::infer_from_types(
                                            &input.entities[uidx].entity_type,
                                            &input.entities[oidx].entity_type,
                                        );
                                        all_relations.push(CandidateRelation {
                                            head_idx: uidx,
                                            tail_idx: oidx,
                                            rel_type: inferred.clone(),
                                            confidence: 0.50,
                                            method: ExtractionMethod::KnowledgeBase,
                                        });
                                        cooccurrence_relations += 1;

                                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                                            session_id: session_id.clone(),
                                            from: Arc::from(entity_labels[uidx].as_str()),
                                            to: Arc::from(entity_labels[oidx].as_str()),
                                            rel_type: Arc::from(inferred.as_str()),
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

            // --- Step 3: Batch SPARQL + property expansion + shortest path ---
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

                // 3b: Property expansion -- discover NEW entities
                {
                    let expansion = self.property_expansion(endpoint, &qids, &entity_labels, &input.language);

                    if !expansion.is_empty() {
                        let provenance = engram_core::graph::Provenance {
                            source_type: engram_core::graph::SourceType::Api,
                            source_id: format!("kb:{}", endpoint.name),
                        };

                        if let Ok(mut g) = self.graph.write() {
                            for (from_label, rel_type, to_label, valid_from, valid_to) in &expansion {
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

                                // Use temporal relate if SPARQL returned date bounds
                                let result = if valid_from.is_some() || valid_to.is_some() {
                                    g.relate_with_temporal(
                                        from_label, to_label, rel_type, 0.8,
                                        valid_from.as_deref(), valid_to.as_deref(),
                                        &provenance,
                                    )
                                } else {
                                    g.relate(from_label, to_label, rel_type, &provenance)
                                };
                                match result {
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

                // 3c: Shortest path discovery -- 1-hop intermediate entities
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

        // --- Step 4: Fix disconnected islands ---
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
                    let inferred = crate::rel_type_templates::infer_from_types(
                        &input.entities[idx].entity_type,
                        &input.entities[anchor_idx].entity_type,
                    );
                    all_relations.push(CandidateRelation {
                        head_idx: idx,
                        tail_idx: anchor_idx,
                        rel_type: inferred.clone(),
                        confidence: 0.45,
                        method: ExtractionMethod::KnowledgeBase,
                    });

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[idx].as_str()),
                        to: Arc::from(entity_labels[anchor_idx].as_str()),
                        rel_type: Arc::from(inferred.as_str()),
                        source: Arc::from("co-extracted_seed_text"),
                    });
                }
            }
        }

        // --- Post-process: upgrade "related_to" to typed relations ---
        // If SPARQL (Step 3) discovered a typed relation for the same entity pair,
        // upgrade the co-occurrence's "related_to" to the typed relation.
        {
            // Build map of (min_idx, max_idx) -> typed_rel_type from non-related_to relations
            let typed_map: HashMap<(usize, usize), String> = all_relations.iter()
                .filter(|r| r.rel_type != "related_to")
                .map(|r| ((r.head_idx.min(r.tail_idx), r.head_idx.max(r.tail_idx)), r.rel_type.clone()))
                .collect();

            if !typed_map.is_empty() {
                let mut upgraded = 0u32;
                for rel in &mut all_relations {
                    if rel.rel_type == "related_to" {
                        let key = (rel.head_idx.min(rel.tail_idx), rel.head_idx.max(rel.tail_idx));
                        if let Some(typed) = typed_map.get(&key) {
                            rel.rel_type = typed.clone();
                            upgraded += 1;
                        }
                    }
                }
                if upgraded > 0 {
                    tracing::info!(upgraded, "upgraded co-occurrence relations with SPARQL types");
                }
            }

            // Dedup: keep highest confidence relation per (head, tail, type) triple
            let mut seen: HashMap<(usize, usize, String), usize> = HashMap::new();
            let mut deduped: Vec<CandidateRelation> = Vec::with_capacity(all_relations.len());
            for rel in all_relations.drain(..) {
                let key = (rel.head_idx.min(rel.tail_idx), rel.head_idx.max(rel.tail_idx), rel.rel_type.clone());
                if let Some(&existing_idx) = seen.get(&key) {
                    if rel.confidence > deduped[existing_idx].confidence {
                        deduped[existing_idx] = rel;
                    }
                } else {
                    seen.insert(key, deduped.len());
                    deduped.push(rel);
                }
            }
            all_relations = deduped;
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

// -- Helpers --

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
#[allow(dead_code)] // used in tests
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
