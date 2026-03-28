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

/// A web search result with URL preserved for document provenance.
#[derive(Debug, Clone)]
pub struct WebSearchResult {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

/// A discovered entity pair from co-occurrence (NO relation type assigned).
/// Classification is deferred to SPARQL (ground truth) or GLiNER2 (text understanding).
#[derive(Debug, Clone)]
pub struct DiscoveredPair {
    pub head_idx: usize,
    pub tail_idx: usize,
    pub source: String,        // "aoi_article", "web_search", "co-extracted"
    pub confidence: f32,       // co-occurrence confidence (0.45-0.60)
}

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
    /// Optional GLiNER2 backend for classifying unresolved co-occurrence pairs.
    gliner2_backend: Option<Arc<dyn RelationExtractor>>,
    /// When true, Steps 3b/3c collect expansion results instead of writing to graph.
    pub defer_graph_writes: bool,
    /// Deferred expansion results: (from_label, rel_type, to_label, node_type, valid_from, valid_to).
    deferred_expansion: Mutex<Vec<(String, String, String, String, Option<String>, Option<String>)>>,
    /// Document store for caching web search content as provenance.
    doc_store: Option<Arc<RwLock<engram_core::storage::doc_store::DocStore>>>,
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
            gliner2_backend: None,
            defer_graph_writes: false,
            deferred_expansion: Mutex::new(Vec::new()),
            doc_store: None,
        }
    }

    /// Set the document store for web search content caching.
    pub fn set_doc_store(&mut self, store: Arc<RwLock<engram_core::storage::doc_store::DocStore>>) {
        self.doc_store = Some(store);
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

    /// Set the GLiNER2 backend for classifying unresolved co-occurrence pairs.
    pub fn set_gliner2_backend(&mut self, backend: Arc<dyn RelationExtractor>) {
        self.gliner2_backend = Some(backend);
    }

    /// Take all deferred expansion results (drains the buffer).
    /// Returns (from_label, rel_type, to_label, node_type, valid_from, valid_to).
    pub fn take_deferred_expansion(&self) -> Vec<(String, String, String, String, Option<String>, Option<String>)> {
        if let Ok(mut buf) = self.deferred_expansion.lock() {
            std::mem::take(&mut *buf)
        } else {
            Vec::new()
        }
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
        let mut all_relations: Vec<CandidateRelation> = Vec::new();
        let mut discovered_pairs: Vec<DiscoveredPair> = Vec::new();
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

        let mut article_text = String::new();

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

            // --- Step 2: Area-of-interest article co-occurrence (DISCOVERY ONLY) ---
            // Co-occurrence finds entity PAIRS but does NOT assign relation types.
            // Classification is deferred to SPARQL (Step 3) and GLiNER2 (Step 5).
            let mut cooccurrence_count = 0u32;

            if let Some(article) = self.fetch_area_of_interest_article(&area_of_interest, &input.language) {
                article_text = article.clone();
                let cooccurrences = self.find_cooccurrences(&article, &entity_labels);
                for (a, b) in &cooccurrences {
                    discovered_pairs.push(DiscoveredPair {
                        head_idx: *a,
                        tail_idx: *b,
                        source: "aoi_article".to_string(),
                        confidence: 0.60,
                    });
                    cooccurrence_count += 1;

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[*a].as_str()),
                        to: Arc::from(entity_labels[*b].as_str()),
                        rel_type: Arc::from("discovered"),
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
                        discovered_pairs.push(DiscoveredPair {
                            head_idx: *a,
                            tail_idx: *b,
                            source: "entity_context_search".to_string(),
                            confidence: 0.55,
                        });
                        cooccurrence_count += 1;

                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                            session_id: session_id.clone(),
                            from: Arc::from(entity_labels[*a].as_str()),
                            to: Arc::from(entity_labels[*b].as_str()),
                            rel_type: Arc::from("discovered"),
                            source: Arc::from("entity_context_search"),
                        });
                    }
                }
            }

            // --- Step 2c: Web search fallback for still-unconnected entities ---
            if self.web_search_provider.is_some() {
                let mut connected_so_far: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for p in &discovered_pairs {
                    connected_so_far.insert(p.head_idx);
                    connected_so_far.insert(p.tail_idx);
                }
                for r in &all_relations {
                    connected_so_far.insert(r.head_idx);
                    connected_so_far.insert(r.tail_idx);
                }
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

                        // Store web search results as Document nodes for provenance
                        self.store_web_search_documents(&web_results, &entity_labels);

                        for result in &web_results {
                            let snippet_lower = result.snippet.to_lowercase();
                            for (oidx, other_label) in entity_labels.iter().enumerate() {
                                if oidx == uidx { continue; }
                                if snippet_lower.contains(&other_label.to_lowercase()) {
                                    let already = discovered_pairs.iter().any(|p|
                                        (p.head_idx == uidx && p.tail_idx == oidx) ||
                                        (p.head_idx == oidx && p.tail_idx == uidx)
                                    );
                                    if !already {
                                        discovered_pairs.push(DiscoveredPair {
                                            head_idx: uidx,
                                            tail_idx: oidx,
                                            source: "web_search".to_string(),
                                            confidence: 0.50,
                                        });
                                        cooccurrence_count += 1;

                                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                                            session_id: session_id.clone(),
                                            from: Arc::from(entity_labels[uidx].as_str()),
                                            to: Arc::from(entity_labels[oidx].as_str()),
                                            rel_type: Arc::from("discovered"),
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
                relations_found: cooccurrence_count,
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
                let expansion = self.property_expansion(endpoint, &qids, &entity_labels, &input.language);
                {

                    if !expansion.is_empty() {
                        tracing::info!(
                            count = expansion.len(),
                            defer = self.defer_graph_writes,
                            "Step 3b: property expansion results"
                        );
                        if self.defer_graph_writes {
                            tracing::info!("Step 3b: deferring {} expansion items to session", expansion.len());
                            // Deferred mode: collect results for session review
                            if let Ok(mut buf) = self.deferred_expansion.lock() {
                                for (from_label, rel_type, to_label, _to_qid, valid_from, valid_to) in &expansion {
                                    let node_type = match rel_type.as_str() {
                                        "citizen_of" | "located_in" | "origin_country" | "capital_of" => "location",
                                        "headquartered_in" => "location",
                                        "manufactured_by" => "organization",
                                        "holds_position" => "position",
                                        "governed_by" => "person",
                                        "member_of" | "employed_by" => "organization",
                                        "educated_at" => "organization",
                                        _ => "entity",
                                    };
                                    buf.push((
                                        from_label.clone(),
                                        rel_type.clone(),
                                        to_label.clone(),
                                        node_type.to_string(),
                                        valid_from.clone(),
                                        valid_to.clone(),
                                    ));
                                    stats.relations_found += 1;

                                    self.emit(engram_core::events::GraphEvent::SeedSparqlRelation {
                                        session_id: session_id.clone(),
                                        from: Arc::from(from_label.as_str()),
                                        to: Arc::from(to_label.as_str()),
                                        rel_type: Arc::from(rel_type.as_str()),
                                    });
                                }
                            }
                        } else {
                            let provenance = engram_core::graph::Provenance {
                                source_type: engram_core::graph::SourceType::Api,
                                source_id: format!("kb:{}", endpoint.name),
                            };

                            if let Ok(mut g) = self.graph.write() {
                                for (from_label, rel_type, to_label, _to_qid, valid_from, valid_to) in &expansion {
                                    let _ = g.store_with_confidence(to_label, 0.70, &provenance);

                                    let node_type = match rel_type.as_str() {
                                        "citizen_of" | "located_in" | "origin_country" | "capital_of" => "location",
                                        "headquartered_in" => "location",
                                        "manufactured_by" => "organization",
                                        "holds_position" => "position",
                                        "governed_by" => "person",
                                        "member_of" | "employed_by" => "organization",
                                        "educated_at" => "organization",
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
                                            tracing::warn!("relate failed: {} -[{}]-> {}: {}", from_label, rel_type, to_label, e);
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
                }

                // 3b2: Org leadership enrichment -- fetch current leaders of discovered orgs
                {
                    let org_rel_types: &[&str] = &["member_of", "employed_by", "headquartered_in"];
                    let mut org_qids_for_enrichment: Vec<(String, String)> = Vec::new();
                    let mut seen_org_qids: std::collections::HashSet<String> = std::collections::HashSet::new();
                    for (_, rel_type, to_label, to_qid, _, _) in &expansion {
                        if org_rel_types.contains(&rel_type.as_str()) && !to_qid.is_empty() {
                            if seen_org_qids.insert(to_qid.clone()) {
                                org_qids_for_enrichment.push((to_qid.clone(), to_label.clone()));
                            }
                        }
                    }

                    if !org_qids_for_enrichment.is_empty() {
                        let org_refs: Vec<(&str, &str)> = org_qids_for_enrichment.iter()
                            .map(|(qid, label)| (qid.as_str(), label.as_str()))
                            .collect();
                        let leaders = self.org_leadership_enrichment(endpoint, &org_refs, &input.language);

                        if self.defer_graph_writes {
                            if let Ok(mut buf) = self.deferred_expansion.lock() {
                                for (org_label, role_rel, leader_label, _leader_qid) in &leaders {
                                    buf.push((
                                        leader_label.clone(),
                                        role_rel.clone(),
                                        org_label.clone(),
                                        "person".to_string(),
                                        None,
                                        None,
                                    ));
                                    stats.relations_found += 1;

                                    self.emit(engram_core::events::GraphEvent::SeedSparqlRelation {
                                        session_id: session_id.clone(),
                                        from: Arc::from(leader_label.as_str()),
                                        to: Arc::from(org_label.as_str()),
                                        rel_type: Arc::from(role_rel.as_str()),
                                    });
                                }
                            }
                        } else {
                            let provenance = engram_core::graph::Provenance {
                                source_type: engram_core::graph::SourceType::Api,
                                source_id: format!("kb:{}", endpoint.name),
                            };
                            if let Ok(mut g) = self.graph.write() {
                                for (org_label, role_rel, leader_label, _leader_qid) in &leaders {
                                    let _ = g.store_with_confidence(leader_label, 0.70, &provenance);
                                    let _ = g.set_node_type(leader_label, "person");
                                    match g.relate(leader_label, org_label, role_rel, &provenance) {
                                        Ok(_) => stats.relations_found += 1,
                                        Err(e) => tracing::warn!("relate failed: {} -[{}]-> {}: {}", leader_label, role_rel, org_label, e),
                                    }
                                }
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
                        if self.defer_graph_writes {
                            // Deferred mode: collect results for session review
                            if let Ok(mut buf) = self.deferred_expansion.lock() {
                                for (from_label, rel_type, to_label) in &path_results {
                                    buf.push((
                                        from_label.clone(),
                                        rel_type.clone(),
                                        to_label.clone(),
                                        "entity".to_string(),
                                        None,
                                        None,
                                    ));
                                    stats.relations_found += 1;
                                }
                            }
                        } else {
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
        // discovery pair back to the most-connected entity. This ensures
        // the graph has no orphaned subgraphs from the same seed text.
        {
            let mut connected_entities: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for r in &all_relations {
                connected_entities.insert(r.head_idx);
                connected_entities.insert(r.tail_idx);
            }
            for p in &discovered_pairs {
                connected_entities.insert(p.head_idx);
                connected_entities.insert(p.tail_idx);
            }

            let disconnected: Vec<usize> = (0..entity_labels.len())
                .filter(|i| !connected_entities.contains(i))
                .collect();

            if !disconnected.is_empty() {
                let mut connection_counts = vec![0usize; entity_labels.len()];
                for r in &all_relations {
                    connection_counts[r.head_idx] += 1;
                    connection_counts[r.tail_idx] += 1;
                }
                for p in &discovered_pairs {
                    connection_counts[p.head_idx] += 1;
                    connection_counts[p.tail_idx] += 1;
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
                    discovered_pairs.push(DiscoveredPair {
                        head_idx: idx,
                        tail_idx: anchor_idx,
                        source: "co-extracted".to_string(),
                        confidence: 0.45,
                    });

                    self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                        session_id: session_id.clone(),
                        from: Arc::from(entity_labels[idx].as_str()),
                        to: Arc::from(entity_labels[anchor_idx].as_str()),
                        rel_type: Arc::from("discovered"),
                        source: Arc::from("co-extracted_seed_text"),
                    });
                }
            }
        }

        // --- Step 5: Classify unresolved co-occurrence pairs ---
        // Compare discovered pairs against SPARQL-typed relations.
        // Pairs with SPARQL types are done; remaining go to GLiNER2.
        {
            let sparql_pairs: std::collections::HashSet<(usize, usize)> = all_relations.iter()
                .map(|r| (r.head_idx.min(r.tail_idx), r.head_idx.max(r.tail_idx)))
                .collect();

            let unresolved: Vec<DiscoveredPair> = discovered_pairs.into_iter()
                .filter(|p| {
                    let key = (p.head_idx.min(p.tail_idx), p.head_idx.max(p.tail_idx));
                    !sparql_pairs.contains(&key)
                })
                .collect();

            if !unresolved.is_empty() {
                if let Some(ref gliner2) = self.gliner2_backend {
                    let gliner_text = if input.text.is_empty() { article_text.clone() } else { input.text.clone() };
                    let gliner_input = RelationExtractionInput {
                        text: gliner_text,
                        entities: input.entities.clone(),
                        language: input.language.clone(),
                        area_of_interest: input.area_of_interest.clone(),
                    };
                    let gliner_results = gliner2.extract_relations(&gliner_input);

                    // Index GLiNER2 results by (min_idx, max_idx)
                    let gliner_map: HashMap<(usize, usize), &CandidateRelation> = gliner_results.iter()
                        .map(|r| ((r.head_idx.min(r.tail_idx), r.head_idx.max(r.tail_idx)), r))
                        .collect();

                    for pair in &unresolved {
                        let key = (pair.head_idx.min(pair.tail_idx), pair.head_idx.max(pair.tail_idx));
                        if let Some(gliner_rel) = gliner_map.get(&key) {
                            // GLiNER2 found a typed relation for this pair
                            all_relations.push(CandidateRelation {
                                head_idx: gliner_rel.head_idx,
                                tail_idx: gliner_rel.tail_idx,
                                rel_type: gliner_rel.rel_type.clone(),
                                confidence: gliner_rel.confidence,
                                method: ExtractionMethod::NeuralZeroShot,
                            });

                            self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                                session_id: session_id.clone(),
                                from: Arc::from(entity_labels[pair.head_idx].as_str()),
                                to: Arc::from(entity_labels[pair.tail_idx].as_str()),
                                rel_type: Arc::from(gliner_rel.rel_type.as_str()),
                                source: Arc::from("gliner2"),
                            });
                        } else {
                            // GLiNER2 found NO_RELATION -- use "related_to" as fallback
                            all_relations.push(CandidateRelation {
                                head_idx: pair.head_idx,
                                tail_idx: pair.tail_idx,
                                rel_type: "related_to".to_string(),
                                confidence: pair.confidence * 0.5,
                                method: ExtractionMethod::KnowledgeBase,
                            });

                            self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                                session_id: session_id.clone(),
                                from: Arc::from(entity_labels[pair.head_idx].as_str()),
                                to: Arc::from(entity_labels[pair.tail_idx].as_str()),
                                rel_type: Arc::from("related_to"),
                                source: Arc::from("no_relation"),
                            });
                        }
                    }
                } else {
                    // No GLiNER2 available -- all unresolved become "related_to" with downgraded confidence
                    for pair in &unresolved {
                        all_relations.push(CandidateRelation {
                            head_idx: pair.head_idx,
                            tail_idx: pair.tail_idx,
                            rel_type: "related_to".to_string(),
                            confidence: pair.confidence * 0.5,
                            method: ExtractionMethod::KnowledgeBase,
                        });

                        self.emit(engram_core::events::GraphEvent::SeedConnectionFound {
                            session_id: session_id.clone(),
                            from: Arc::from(entity_labels[pair.head_idx].as_str()),
                            to: Arc::from(entity_labels[pair.tail_idx].as_str()),
                            rel_type: Arc::from("related_to"),
                            source: Arc::from("unresolved_cooccurrence"),
                        });
                    }
                }
            }
        }

        // --- Post-process: upgrade "related_to" to typed relations ---
        // If SPARQL (Step 3) discovered a typed relation for the same entity pair,
        // upgrade the co-occurrence's "related_to" to the typed relation.
        {
            // Build map of (min_idx, max_idx) -> typed_rel_type from non-related_to relations
            let mut typed_map: HashMap<(usize, usize), String> = HashMap::new();
            for r in all_relations.iter().filter(|r| r.rel_type != "related_to") {
                typed_map.insert((r.head_idx.min(r.tail_idx), r.head_idx.max(r.tail_idx)), r.rel_type.clone());
            }

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
        let elapsed = total_stats.lookup_ms;
        *self.last_stats.lock().unwrap() = Some(total_stats);

        tracing::info!(relations = all_relations.len(), ms = elapsed, "KB extract_relations complete");
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

/// Check if a P39 position title is generic (no specific context).
/// Generic positions like "party leader", "chairperson", "recruiter" create
/// meaningless nodes. Specific ones like "President of Russia" contain a
/// proper noun context (typically via " of ").
pub(super) fn is_generic_position(value_label: &str) -> bool {
    // Positions containing " of " have specific context: "President of Russia"
    if value_label.contains(" of ") {
        return false;
    }
    // Check for proper nouns after the first word (uppercase words = specific)
    // "Prime Minister" is title-case generic, but "NATO Secretary General" has "NATO"
    let words: Vec<&str> = value_label.split_whitespace().collect();
    if words.len() <= 1 {
        return true; // single word like "recruiter" or "chairperson"
    }
    // If any word after the first is fully uppercase (acronym like NATO, CIS),
    // it's specific
    for w in &words[1..] {
        if w.len() >= 2 && w.chars().all(|c| c.is_uppercase()) {
            return false;
        }
    }
    // No " of ", no acronyms = generic role title
    true
}

/// Map org leadership role labels to relation types.
pub(super) fn org_role_to_rel_type(role_label: &str) -> String {
    let label = if role_label.contains('/') {
        uri_to_label(role_label)
    } else {
        role_label.to_lowercase().replace(' ', "_")
    };
    match label.as_str() {
        "p488" | "chairperson" => "leads".to_string(),
        "p169" | "chief_executive_officer" => "leads".to_string(),
        "p112" | "founded_by" => "founded".to_string(),
        "p6" | "head_of_government" => "governed_by".to_string(),
        "p35" | "head_of_state" => "head_of_state".to_string(),
        _ => "leads".to_string(),
    }
}

/// Map Wikidata property labels to engram relation types.
pub(crate) fn wikidata_prop_to_rel_type(prop_label: &str) -> String {
    if prop_label.is_empty() {
        return "related_to".to_string();
    }

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
        "p102" | "member_of_political_party" => "member_of".to_string(),
        "p108" | "employer" => "employed_by".to_string(),
        "p463" | "member_of" => "member_of".to_string(),
        "p69" | "educated_at" => "educated_at".to_string(),
        _ => label,
    }
}
