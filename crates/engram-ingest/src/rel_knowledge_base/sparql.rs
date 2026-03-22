//! SPARQL queries: direct lookups, batch relation discovery, property expansion,
//! and shortest-path intermediate entity discovery.

use super::*;

impl KbRelationExtractor {
    /// Execute a SPARQL query against an endpoint, returning parsed JSON results.
    pub(super) fn sparql_query(
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

    /// Batch relation discovery: find ALL direct connections between a set of QIDs
    /// in a single SPARQL query.
    pub(super) fn batch_relation_lookup(
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

        // Build QID -> entity_index map
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
    /// Returns (from_label, rel_type, to_label, to_qid, valid_from, valid_to) tuples.
    pub(super) fn property_expansion(
        &self,
        endpoint: &KbEndpoint,
        qids: &[(&str, usize)], // (QID URI, entity_index)
        entity_labels: &[String],
        language: &str,
    ) -> Vec<(String, String, String, String, Option<String>, Option<String>)> {
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
            r#"SELECT ?entity ?entityLabel ?propNodeLabel ?value ?valueLabel ?startTime ?endTime ?qualifierLabel WHERE {{
                VALUES ?entity {{ {values} }}
                VALUES ?propNode {{ wd:P39 wd:P27 wd:P17 wd:P159 wd:P176 wd:P495 wd:P36 wd:P102 wd:P108 wd:P463 wd:P69 }}
                ?propNode wikibase:claim ?propclaim .
                ?propNode wikibase:statementProperty ?stmtprop .
                ?entity ?propclaim ?stmt .
                ?stmt ?stmtprop ?value .
                FILTER(isIRI(?value))
                OPTIONAL {{ ?stmt pq:P580 ?startTime . }}
                OPTIONAL {{ ?stmt pq:P582 ?endTime . }}
                OPTIONAL {{ ?stmt pq:P642 ?qualifier . }}
                SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
            }} LIMIT 300"#,
            values = values,
            lang = language,
        );

        let json = match self.sparql_query(endpoint, &query) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        // Build QID -> entity label map
        let qid_to_label: HashMap<String, String> = qids.iter()
            .map(|(qid, idx)| (extract_qid(qid).to_string(), entity_labels[*idx].clone()))
            .collect();

        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(bindings) = json.pointer("/results/bindings").and_then(|b| b.as_array()) {
            for binding in bindings {
                let entity_qid = binding.pointer("/entity/value").and_then(|v| v.as_str()).unwrap_or("");
                let prop_label = binding.pointer("/propNodeLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let value_label = binding.pointer("/valueLabel/value").and_then(|v| v.as_str()).unwrap_or("");

                // Skip self-references, empty values, and empty property labels
                if value_label.is_empty() || value_label.starts_with("http://") {
                    continue;
                }
                if prop_label.is_empty() || prop_label.starts_with("http://") {
                    continue;
                }
                // Skip QID values where label service failed (e.g. "Q38715852")
                if value_label.starts_with('Q') && value_label[1..].chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }

                let entity_label = match qid_to_label.get(extract_qid(entity_qid)) {
                    Some(l) => l.clone(),
                    None => continue,
                };

                let rel_type = wikidata_prop_to_rel_type(prop_label);

                // P39 (holds_position): skip generic role titles that lack context.
                // "President of Russia" is useful (contains "of [proper noun]").
                // "party leader", "chairperson", "recruiter" are generic -- they'd
                // create meaningless nodes. The actual org/party is found via P102.
                if rel_type == "holds_position" && is_generic_position(value_label) {
                    tracing::debug!(value = value_label, "skipping generic P39 position");
                    continue;
                }

                let node_label = value_label.to_string();
                let value_uri = binding.pointer("/value/value").and_then(|v| v.as_str()).unwrap_or("");
                let to_qid = extract_qid(value_uri).to_string();

                // Deduplicate
                let key = (entity_label.clone(), node_label.clone());
                if seen.contains(&key) { continue; }
                seen.insert(key);

                // Extract temporal qualifiers
                let start_time = binding.pointer("/startTime/value")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect::<String>()); // YYYY-MM-DD
                let end_time = binding.pointer("/endTime/value")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).collect::<String>()); // YYYY-MM-DD

                results.push((entity_label, rel_type, node_label, to_qid, start_time, end_time));
            }
        }

        results
    }

    /// Batch shortest path: find 1-hop intermediate entities between ALL entity pairs
    /// in a single SPARQL query. Returns (from_label, rel_type, to_label) triples
    /// including the intermediate node.
    pub(super) fn batch_shortest_paths(
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

        // 1-hop: A -> ?mid -> B (single SPARQL for ALL pairs)
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

    /// Org leadership enrichment: for discovered organizations/parties/countries,
    /// fetch their current leaders via P488 (chairperson), P169 (CEO), P112 (founder),
    /// P6 (head of government), P35 (head of state).
    /// Only returns CURRENT leaders (no end date).
    pub(super) fn org_leadership_enrichment(
        &self,
        endpoint: &KbEndpoint,
        org_qids: &[(&str, &str)], // (QID, org_label)
        language: &str,
    ) -> Vec<(String, String, String, String)> {
        if org_qids.is_empty() {
            return Vec::new();
        }

        let values: String = org_qids.iter()
            .map(|(qid, _)| format!("wd:{}", extract_qid(qid)))
            .collect::<Vec<_>>()
            .join(" ");

        let query = format!(
            r#"SELECT ?org ?orgLabel ?rolePropLabel ?leader ?leaderLabel WHERE {{
                VALUES ?org {{ {values} }}
                VALUES ?roleProp {{ wd:P488 wd:P169 wd:P112 wd:P6 wd:P35 }}
                ?roleProp wikibase:claim ?propclaim .
                ?roleProp wikibase:statementProperty ?stmtprop .
                ?org ?propclaim ?stmt .
                ?stmt ?stmtprop ?leader .
                FILTER(isIRI(?leader))
                FILTER NOT EXISTS {{ ?stmt pq:P582 ?endTime . }}
                SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en,{lang}" }}
            }} LIMIT 200"#,
            values = values,
            lang = language,
        );

        let json = match self.sparql_query(endpoint, &query) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("org leadership SPARQL failed: {e}");
                return Vec::new();
            }
        };

        let qid_to_org: HashMap<String, String> = org_qids.iter()
            .map(|(qid, label)| (extract_qid(qid).to_string(), label.to_string()))
            .collect();

        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(bindings) = json.pointer("/results/bindings").and_then(|b| b.as_array()) {
            for binding in bindings {
                let org_uri = binding.pointer("/org/value").and_then(|v| v.as_str()).unwrap_or("");
                let role_label = binding.pointer("/rolePropLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let leader_label = binding.pointer("/leaderLabel/value").and_then(|v| v.as_str()).unwrap_or("");
                let leader_uri = binding.pointer("/leader/value").and_then(|v| v.as_str()).unwrap_or("");

                if leader_label.is_empty() || leader_label.starts_with("http://") {
                    continue;
                }
                if leader_label.starts_with('Q') && leader_label[1..].chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }

                let org_label = match qid_to_org.get(extract_qid(org_uri)) {
                    Some(l) => l.clone(),
                    None => continue,
                };

                let key = (org_label.clone(), leader_label.to_string());
                if seen.contains(&key) { continue; }
                seen.insert(key);

                let role_rel = org_role_to_rel_type(role_label);
                let leader_qid = extract_qid(leader_uri).to_string();

                results.push((org_label, role_rel, leader_label.to_string(), leader_qid));
            }
        }

        tracing::info!(orgs = org_qids.len(), leaders = results.len(), "org leadership enrichment");
        results
    }
}
