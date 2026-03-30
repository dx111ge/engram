/// LLM-based fact extraction (Layer 2 of document provenance).
///
/// Extracts semantic claims from document text using a local or remote LLM.
/// Each claim becomes a Fact node linked to entities and documents.
///
/// Uses the KGGen two-stage pattern:
/// 1. Entity extraction (done by NER pipeline, passed in)
/// 2. Claim extraction (this module) with delimiter-based output
///
/// Configurable: always / on-demand / never via pipeline config.

use std::collections::HashMap;

/// An extracted claim from a document chunk.
#[derive(Debug, Clone)]
pub struct ExtractedClaim {
    /// SPO subject (entity name).
    pub subject: String,
    /// SPO predicate (verb phrase).
    pub predicate: String,
    /// SPO object (entity or phrase).
    pub object: String,
    /// Derived claim text: "{subject} {predicate} {object}".
    pub claim: String,
    /// Entity names mentioned in this claim.
    pub entities: Vec<String>,
    /// Event date if extractable (ISO-8601).
    pub date: Option<String>,
    /// Confidence: high=0.85, medium=0.60, low=0.30.
    pub confidence: f32,
    /// Index of the chunk within the document (0-based).
    pub chunk_index: usize,
    /// The source passage (chunk text) this claim was extracted from.
    pub source_passage: String,
}

/// Configuration for fact extraction.
#[derive(Debug, Clone)]
pub struct FactExtractConfig {
    /// LLM endpoint (OpenAI-compatible chat completions).
    pub llm_endpoint: String,
    /// LLM model name.
    pub llm_model: String,
    /// Whether to run a gleaning pass (re-prompt for missed claims).
    pub gleaning: bool,
    /// Max tokens for LLM response.
    pub max_tokens: u32,
    /// Temperature (low = more deterministic).
    pub temperature: f32,
}

/// Split text into paragraph-based chunks of approximately `max_tokens` tokens.
/// Uses 10% overlap to avoid splitting claims at boundaries.
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks = Vec::new();
    let mut current = String::new();

    for para in &paragraphs {
        if current.len() + para.len() + 2 > max_chars && !current.is_empty() {
            chunks.push(current.clone());
            // 10% overlap: keep last ~10% of current chunk
            let overlap_start = current.len().saturating_sub(max_chars / 10);
            current = current[overlap_start..].to_string();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    // If no paragraph splits produced multiple chunks, split by sentences
    if chunks.len() <= 1 && text.len() > max_chars {
        chunks.clear();
        current = String::new();
        for sentence in text.split(". ") {
            if current.len() + sentence.len() + 2 > max_chars && !current.is_empty() {
                chunks.push(current.clone());
                let overlap_start = current.len().saturating_sub(max_chars / 10);
                current = current[overlap_start..].to_string();
            }
            if !current.is_empty() {
                current.push_str(". ");
            }
            current.push_str(sentence);
        }
        if !current.is_empty() {
            chunks.push(current);
        }
    }

    chunks
}

/// Extract factual claims from a text chunk using the LLM.
pub fn extract_claims(
    client: &reqwest::blocking::Client,
    config: &FactExtractConfig,
    chunk: &str,
    entity_names: &[String],
    chunk_index: usize,
) -> Vec<ExtractedClaim> {
    let entity_list = entity_names.join(", ");

    let prompt = format!(
        "You are a knowledge graph fact extraction engine. Extract Subject-Predicate-Object triples from this text.\n\n\
         Known entities in scope: {entity_list}\n\n\
         Rules:\n\
         - Each fact must be a triple: Subject | Predicate | Object\n\
         - Subject and Object should be entity names (use exact names from the entity list)\n\
         - Predicate should be a concise verb phrase (e.g. \"deployed\", \"is president of\", \"signed agreement with\")\n\
         - Each triple must be atomic: one relationship per line\n\
         - Extract dates/time references when present (ISO-8601)\n\
         - Rate confidence: \"stated\" facts = high, \"alleged\"/\"reported\" = medium, \"speculated\"/\"rumored\" = low\n\
         - Do NOT infer facts not stated in the text\n\
         - Do NOT use pronouns (he, she, they, it) as Subject or Object\n\n\
         Examples:\n\
         FACT||Russia||deployed||Shahed drones in Ukraine||Russia,Ukraine||2024-03-15||high\n\
         FACT||NATO||expanded sanctions against||Russian energy sector||NATO,Russia||2024-02-01||medium\n\
         FACT||Zelensky||met with||Biden at White House||Zelensky,Biden||null||high\n\n\
         Text:\n{chunk}\n\n\
         Return using this format (one per line):\n\
         FACT||Subject||Predicate||Object||entity1,entity2||YYYY-MM-DD or null||high/medium/low"
    );

    let body = serde_json::json!({
        "model": config.llm_model,
        "messages": [
            {"role": "system", "content": "You extract Subject-Predicate-Object triples from text for a knowledge graph. Use the exact delimiter format requested. One fact per line."},
            {"role": "user", "content": prompt}
        ],
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
    });

    let chunk_owned = chunk.to_string();
    let mut claims = match call_llm(client, &config.llm_endpoint, &body) {
        Some(text) => parse_claims(&text, chunk_index, &chunk_owned),
        None => return Vec::new(),
    };

    // Gleaning pass: ask for missed facts
    if config.gleaning && !claims.is_empty() {
        let gleaning_prompt = format!(
            "Some facts were missed in the previous extraction. \
             Review the text again and extract any additional Subject-Predicate-Object triples not yet captured.\n\n\
             Already extracted:\n{}\n\n\
             Text:\n{chunk}\n\n\
             Return using this format (one per line):\n\
             FACT||Subject||Predicate||Object||entity1,entity2||YYYY-MM-DD or null||high/medium/low",
            claims.iter().map(|c| c.claim.as_str()).collect::<Vec<_>>().join("\n")
        );

        let gleaning_body = serde_json::json!({
            "model": config.llm_model,
            "messages": [
                {"role": "system", "content": "You extract Subject-Predicate-Object triples from text for a knowledge graph. Use the exact delimiter format requested. One fact per line."},
                {"role": "user", "content": gleaning_prompt}
            ],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        if let Some(text) = call_llm(client, &config.llm_endpoint, &gleaning_body) {
            let extra = parse_claims(&text, chunk_index, &chunk_owned);
            // Dedup: only add claims not already present
            for claim in extra {
                let already = claims.iter().any(|c| {
                    c.claim.to_lowercase() == claim.claim.to_lowercase()
                });
                if !already {
                    claims.push(claim);
                }
            }
        }
    }

    claims
}

/// Compute a quality-adjusted confidence score for an extracted claim.
/// Adjusts the LLM's self-rated confidence based on structural quality signals.
fn compute_quality_score(claim: &ExtractedClaim) -> f32 {
    let mut score = claim.confidence;

    // Both subject AND object appear in source -> keep base
    // Only one appears -> penalize
    if !claim.source_passage.is_empty() {
        let src = claim.source_passage.to_lowercase();
        let subj_in = src.contains(&claim.subject.to_lowercase());
        let obj_in = src.contains(&claim.object.to_lowercase());
        if subj_in && obj_in {
            // Both grounded -- good
        } else {
            score *= 0.8;
        }
    }

    // Specific object (>3 words) -> slight boost
    let obj_words = claim.object.split_whitespace().count();
    if obj_words >= 3 {
        score *= 1.1;
    }

    // Short source passage (likely a snippet, not full article) -> penalize
    if claim.source_passage.len() < 500 {
        score *= 0.7;
    }

    // Has entity links -> slight boost
    if !claim.entities.is_empty() {
        score *= 1.05;
    }

    // Clamp to [0.05, 0.90]
    score.clamp(0.05, 0.90)
}

/// Create Fact nodes in the graph from extracted claims.
pub fn store_facts_in_graph(
    graph: &mut engram_core::graph::Graph,
    claims: &[ExtractedClaim],
    doc_label: &str,
    known_entities: &[String],
) -> u32 {
    let prov = engram_core::graph::Provenance {
        source_type: engram_core::graph::SourceType::Derived,
        source_id: "llm_fact_extract".to_string(),
    };
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut count = 0u32;

    for claim in claims {
        let fact_label = make_fact_label(claim);
        if graph.find_node_id(&fact_label).ok().flatten().is_some() {
            continue; // dedup
        }

        let quality_confidence = compute_quality_score(claim);
        if graph.store_with_confidence(&fact_label, quality_confidence, &prov).is_err() {
            continue;
        }
        let _ = graph.set_node_type(&fact_label, "Fact");
        let _ = graph.set_property(&fact_label, "claim", &claim.claim);
        let _ = graph.set_property(&fact_label, "status", "pending");
        let _ = graph.set_property(&fact_label, "extraction_method", "LLM");
        let _ = graph.set_property(&fact_label, "extraction_confidence", &claim.confidence.to_string());
        let _ = graph.set_property(&fact_label, "confidence_source", "llm");
        let _ = graph.set_property(&fact_label, "extracted_at", &now_ts.to_string());
        let _ = graph.set_property(&fact_label, "chunk_index", &claim.chunk_index.to_string());
        if !claim.subject.is_empty() {
            let _ = graph.set_property(&fact_label, "subject", &claim.subject);
            let _ = graph.set_property(&fact_label, "predicate", &claim.predicate);
            let _ = graph.set_property(&fact_label, "object", &claim.object);
        }
        if !claim.source_passage.is_empty() {
            let _ = graph.set_property(&fact_label, "source_passage", &claim.source_passage);
        }
        if let Some(ref date) = claim.date {
            let _ = graph.set_property(&fact_label, "event_date", date);
        }

        // Edge: Fact -> Document (extracted_from)
        let _ = graph.relate_upsert(&fact_label, doc_label, "extracted_from", &prov);

        // Edge: Entity -> Fact (subject_of) for each matched entity
        let claim_lower = claim.claim.to_lowercase();
        for entity_name in &claim.entities {
            // Try exact match against known entities first
            let matched = known_entities.iter().find(|e| {
                e.to_lowercase() == entity_name.to_lowercase()
            });
            if let Some(entity_label) = matched {
                let _ = graph.relate_upsert(entity_label, &fact_label, "subject_of", &prov);
            } else if claim_lower.contains(&entity_name.to_lowercase()) {
                // Fallback: if entity mentioned in claim text, check if it exists in graph
                if graph.find_node_id(entity_name).ok().flatten().is_some() {
                    let _ = graph.relate_upsert(entity_name, &fact_label, "subject_of", &prov);
                }
            }
        }

        count += 1;
    }

    count
}

/// Generate a human-readable Fact node label from an extracted claim.
/// Uses SPO format "Subject | predicate | Object" when available,
/// falls back to truncated claim text for legacy claims.
fn make_fact_label(claim: &ExtractedClaim) -> String {
    if !claim.subject.is_empty() && !claim.predicate.is_empty() && !claim.object.is_empty() {
        let label = format!("{} | {} | {}", claim.subject, claim.predicate, claim.object);
        if label.len() <= 80 {
            return label;
        }
        // Truncate at word boundary
        let truncated = &label[..80];
        let cut = truncated.rsplit_once(' ').map(|(l, _)| l).unwrap_or(truncated);
        return format!("{}...", cut);
    }
    // Legacy: truncate claim text
    let mut label = String::new();
    for word in claim.claim.split_whitespace() {
        if !label.is_empty() {
            label.push(' ');
        }
        label.push_str(word);
        if label.len() >= 60 {
            break;
        }
    }
    if label.len() < claim.claim.len() {
        label.push_str("...");
    }
    label
}

/// Call the LLM and return the response text.
fn call_llm(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    body: &serde_json::Value,
) -> Option<String> {
    let resp = client
        .post(endpoint)
        .json(body)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .ok()?;

    let json: serde_json::Value = resp.json().ok()?;
    json.pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Parse delimiter-based LLM output into ExtractedClaim structs.
/// Supports both new SPO format (FACT||subj||pred||obj||entities||date||conf)
/// and legacy format (CLAIM||text||entities||date||conf) for backward compatibility.
fn parse_claims(text: &str, chunk_index: usize, source_passage: &str) -> Vec<ExtractedClaim> {
    let mut claims = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("FACT||") {
            // New SPO format: FACT||Subject||Predicate||Object||entities||date||confidence
            let parts: Vec<&str> = line.splitn(8, "||").collect();
            if parts.len() < 4 {
                continue;
            }
            let subject = parts[1].trim().to_string();
            let predicate = parts[2].trim().to_string();
            let object = parts[3].trim().to_string();
            if subject.is_empty() || predicate.is_empty() || object.is_empty() {
                continue;
            }
            let claim = format!("{} {} {}", subject, predicate, object);
            let entities: Vec<String> = if parts.len() > 4 {
                parts[4].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                Vec::new()
            };
            let date = if parts.len() > 5 {
                let d = parts[5].trim();
                if d == "null" || d.is_empty() { None } else { Some(d.to_string()) }
            } else {
                None
            };
            let confidence = if parts.len() > 6 {
                parse_confidence(parts[6].trim())
            } else {
                0.60
            };

            let claim_obj = ExtractedClaim {
                subject, predicate, object, claim, entities, date, confidence,
                chunk_index, source_passage: source_passage.to_string(),
            };
            if validate_claim(&claim_obj, source_passage) {
                claims.push(claim_obj);
            }
        } else if line.starts_with("CLAIM||") {
            // Legacy format: CLAIM||text||entities||date||confidence
            let parts: Vec<&str> = line.splitn(6, "||").collect();
            if parts.len() < 3 {
                continue;
            }
            let claim_text = parts[1].trim().to_string();
            if claim_text.is_empty() {
                continue;
            }
            let entities: Vec<String> = if parts.len() > 2 {
                parts[2].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                Vec::new()
            };
            let date = if parts.len() > 3 {
                let d = parts[3].trim();
                if d == "null" || d.is_empty() { None } else { Some(d.to_string()) }
            } else {
                None
            };
            let confidence = if parts.len() > 4 {
                parse_confidence(parts[4].trim())
            } else {
                0.60
            };

            claims.push(ExtractedClaim {
                subject: String::new(),
                predicate: String::new(),
                object: String::new(),
                claim: claim_text,
                entities, date, confidence, chunk_index,
                source_passage: source_passage.to_string(),
            });
        }
    }

    claims
}

fn parse_confidence(s: &str) -> f32 {
    match s.to_lowercase().as_str() {
        "high" => 0.85,
        "medium" | "med" => 0.60,
        "low" => 0.30,
        _ => 0.60,
    }
}

/// Stopwords that should not be standalone objects or subjects.
const STOP_WORDS: &[&str] = &[
    "in", "by", "the", "a", "an", "to", "for", "of", "on", "at",
    "is", "are", "was", "were", "with", "from", "as",
];

/// Common verbs that should not appear as subjects (sign of garbled SPO).
const VERB_SUBJECTS: &[&str] = &[
    "damaged", "destroyed", "reported", "said", "claimed", "stated",
    "noted", "confirmed", "announced", "described",
];

/// Pronouns rejected as subjects.
const PRONOUNS: &[&str] = &[
    "he", "she", "it", "they", "this", "that", "these", "those", "who", "which",
];

/// Validate an extracted claim for quality.
fn validate_claim(claim: &ExtractedClaim, source_chunk: &str) -> bool {
    let subj = claim.subject.trim();
    let pred = claim.predicate.trim();
    let obj = claim.object.trim();

    // Atomicity: all three must be non-empty
    if subj.is_empty() || pred.is_empty() || obj.is_empty() {
        return false;
    }

    // Minimum meaningful length
    if subj.len() < 2 || obj.len() < 2 {
        return false;
    }

    // Faithfulness: at least subject OR object should appear in source text
    let source_lower = source_chunk.to_lowercase();
    let subj_in = source_lower.contains(&subj.to_lowercase());
    let obj_in = source_lower.contains(&obj.to_lowercase());
    if !subj_in && !obj_in {
        return false;
    }

    // Reject pronouns as subject
    let subj_lower = subj.to_lowercase();
    if PRONOUNS.contains(&subj_lower.as_str()) {
        return false;
    }

    // Reject standalone stopword as object (e.g., "in", "by", "the")
    let obj_lower = obj.to_lowercase();
    if STOP_WORDS.contains(&obj_lower.as_str()) {
        return false;
    }

    // Reject trailing preposition in object (sign of truncated extraction)
    let obj_words: Vec<&str> = obj.split_whitespace().collect();
    if obj_words.len() > 1 {
        let last = obj_words.last().unwrap().to_lowercase();
        if STOP_WORDS.contains(&last.as_str()) {
            return false;
        }
    }

    // Reject common verb as subject (garbled SPO)
    if VERB_SUBJECTS.contains(&subj_lower.as_str()) {
        return false;
    }

    // Reject circular facts (subject == object)
    if subj_lower == obj_lower {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_short() {
        let chunks = chunk_text("Short text.", 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Short text.");
    }

    #[test]
    fn test_chunk_text_paragraphs() {
        let text = "First paragraph about Putin.\n\nSecond paragraph about NATO.\n\nThird paragraph about Ukraine.";
        let chunks = chunk_text(text, 50);
        assert!(chunks.len() >= 2, "should split into multiple chunks, got {}", chunks.len());
    }

    #[test]
    fn test_parse_claims() {
        let text = "CLAIM||Putin warned NATO against intervention||Putin,NATO||2024-02-29||high\nCLAIM||Sanctions failed to weaken Russia||Russia||null||medium\ngarbage line\n";
        let source = "Some source passage about Putin and NATO.";
        let claims = parse_claims(text, 0, source);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].claim, "Putin warned NATO against intervention");
        assert_eq!(claims[0].entities, vec!["Putin", "NATO"]);
        assert_eq!(claims[0].date, Some("2024-02-29".to_string()));
        assert_eq!(claims[0].confidence, 0.85);
        assert_eq!(claims[0].chunk_index, 0);
        assert_eq!(claims[0].source_passage, source);
        assert_eq!(claims[1].claim, "Sanctions failed to weaken Russia");
        assert_eq!(claims[1].date, None);
        assert_eq!(claims[1].confidence, 0.60);
    }

    #[test]
    fn test_make_fact_label_spo() {
        let claim = ExtractedClaim {
            subject: "Russia".into(), predicate: "deployed".into(),
            object: "Shahed drones in Ukraine".into(),
            claim: "Russia deployed Shahed drones in Ukraine".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        let label = make_fact_label(&claim);
        assert_eq!(label, "Russia | deployed | Shahed drones in Ukraine");
    }

    #[test]
    fn test_make_fact_label_legacy() {
        let claim = ExtractedClaim {
            subject: String::new(), predicate: String::new(), object: String::new(),
            claim: "Putin warned NATO against direct intervention in the ongoing conflict".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        let label = make_fact_label(&claim);
        assert!(label.len() <= 70);
        assert!(label.ends_with("..."));
        assert!(label.starts_with("Putin warned"));
    }

    #[test]
    fn test_parse_spo_claims() {
        let text = "FACT||Russia||deployed||Shahed drones in Ukraine||Russia,Ukraine||2024-03-15||high\n\
                    FACT||NATO||expanded sanctions against||Russian energy sector||NATO,Russia||null||medium\n";
        let source = "Russia deployed Shahed drones. NATO expanded sanctions.";
        let claims = parse_claims(text, 0, source);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].subject, "Russia");
        assert_eq!(claims[0].predicate, "deployed");
        assert_eq!(claims[0].object, "Shahed drones in Ukraine");
        assert_eq!(claims[0].claim, "Russia deployed Shahed drones in Ukraine");
        assert_eq!(claims[0].confidence, 0.85);
        assert_eq!(claims[1].subject, "NATO");
        assert_eq!(claims[1].predicate, "expanded sanctions against");
        assert_eq!(claims[1].object, "Russian energy sector");
    }

    #[test]
    fn test_validate_rejects_stopword_object() {
        let claim = ExtractedClaim {
            subject: "Leopard 2".into(), predicate: "failing".into(), object: "in".into(),
            claim: "Leopard 2 failing in".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "Leopard 2 failing in Ukraine."));
    }

    #[test]
    fn test_validate_rejects_trailing_preposition() {
        let claim = ExtractedClaim {
            subject: "Leopard 2".into(), predicate: "echoes".into(),
            object: "German Tiger tank in".into(),
            claim: "Leopard 2 echoes German Tiger tank in".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "Leopard 2 echoes German Tiger tank in WW2."));
    }

    #[test]
    fn test_validate_rejects_verb_subject() {
        let claim = ExtractedClaim {
            subject: "damaged".into(), predicate: "remainder of".into(),
            object: "Leopard 2 tanks".into(),
            claim: "damaged remainder of Leopard 2 tanks".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "damaged remainder of Leopard 2 tanks."));
    }

    #[test]
    fn test_validate_rejects_circular() {
        let claim = ExtractedClaim {
            subject: "Russia".into(), predicate: "is".into(), object: "Russia".into(),
            claim: "Russia is Russia".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "Russia is Russia."));
    }

    #[test]
    fn test_validate_rejects_short() {
        let claim = ExtractedClaim {
            subject: "X".into(), predicate: "is".into(), object: "Y".into(),
            claim: "X is Y".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "X is Y."));
    }

    #[test]
    fn test_validate_rejects_pronouns() {
        let claim = ExtractedClaim {
            subject: "He".into(), predicate: "said".into(), object: "it was wrong".into(),
            claim: "He said it was wrong".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "He said it was wrong."));
    }

    #[test]
    fn test_validate_rejects_empty_subject() {
        let claim = ExtractedClaim {
            subject: String::new(), predicate: "deployed".into(), object: "drones".into(),
            claim: "deployed drones".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "Someone deployed drones."));
    }

    #[test]
    fn test_validate_rejects_unfaithful() {
        let claim = ExtractedClaim {
            subject: "China".into(), predicate: "invaded".into(), object: "Mars".into(),
            claim: "China invaded Mars".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(!validate_claim(&claim, "Russia deployed Shahed drones in Ukraine."));
    }

    #[test]
    fn test_validate_accepts_valid() {
        let claim = ExtractedClaim {
            subject: "Russia".into(), predicate: "deployed".into(), object: "Shahed drones".into(),
            claim: "Russia deployed Shahed drones".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0, source_passage: String::new(),
        };
        assert!(validate_claim(&claim, "Russia deployed Shahed drones in Ukraine."));
    }

    #[test]
    fn test_backward_compat_legacy_format() {
        let text = "CLAIM||Putin warned NATO||Putin,NATO||2024-02-29||high\n";
        let claims = parse_claims(text, 0, "source text");
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].claim, "Putin warned NATO");
        assert!(claims[0].subject.is_empty()); // legacy has no SPO
    }

    #[test]
    fn test_quality_score_full_article() {
        // Long source passage (>500 chars) with both S and O grounded
        let long_passage = "Russia deployed Shahed drones in Ukraine. ".repeat(20); // ~800 chars
        let claim = ExtractedClaim {
            subject: "Russia".into(), predicate: "deployed".into(),
            object: "Shahed drones".into(),
            claim: "Russia deployed Shahed drones".into(),
            entities: vec!["Russia".into()], date: None, confidence: 0.85,
            chunk_index: 0,
            source_passage: long_passage,
        };
        let score = compute_quality_score(&claim);
        // Full article, both grounded, entities -> should be near or above base
        assert!(score > 0.80, "score {score} should be > 0.80 for full article");
    }

    #[test]
    fn test_quality_score_snippet_penalized() {
        let claim = ExtractedClaim {
            subject: "Russia".into(), predicate: "deployed".into(),
            object: "drones".into(),
            claim: "Russia deployed drones".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0,
            source_passage: "Russia deployed drones.".into(), // short snippet
        };
        let score = compute_quality_score(&claim);
        // Short source passage -> penalized by 0.7x
        assert!(score < 0.70, "score {score} should be < 0.70 for snippet");
    }

    #[test]
    fn test_quality_score_snippet_vs_article() {
        // Same claim from snippet vs full article should differ
        let claim_snippet = ExtractedClaim {
            subject: "Russia".into(), predicate: "deployed".into(),
            object: "Shahed drones in Ukraine".into(),
            claim: "Russia deployed Shahed drones in Ukraine".into(),
            entities: vec![], date: None, confidence: 0.85,
            chunk_index: 0,
            source_passage: "Russia deployed Shahed drones.".into(),
        };
        let long_passage = "Russia deployed Shahed drones in Ukraine during the ongoing conflict. ".repeat(15);
        let claim_article = ExtractedClaim {
            source_passage: long_passage,
            ..claim_snippet.clone()
        };
        let snippet_score = compute_quality_score(&claim_snippet);
        let article_score = compute_quality_score(&claim_article);
        assert!(article_score > snippet_score,
            "article score {article_score} should be > snippet score {snippet_score}");
    }

    #[test]
    fn test_chunk_text_large() {
        // Use sentence-splittable text so chunk_text can split on ". "
        let sentence = "The Leopard 2 tank is deployed in Ukraine. ";
        let text = sentence.repeat(250); // ~10500 chars
        let chunks = chunk_text(&text, 3000);
        assert!(chunks.len() >= 3, "10K chars should produce >= 3 chunks at 3000 max, got {}", chunks.len());
    }
}
