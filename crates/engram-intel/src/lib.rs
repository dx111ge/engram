//! engram-intel: Geopolitical intelligence engine compiled to WASM.
//!
//! All analysis logic lives here -- probability calculation, source tier
//! assessment, article classification, query generation. The dashboard
//! HTML calls these functions; the methodology stays in compiled binary.

mod classifier;
mod license;
mod probability;
mod queries;
mod tiers;

use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Probability engine
// ---------------------------------------------------------------------------

/// Calculate prediction probability from evidence arrays.
/// `evidence_for`: JSON array of confidence floats [0.85, 0.80, ...]
/// `evidence_against`: JSON array of confidence floats
/// Returns probability as f64.
#[wasm_bindgen]
pub fn calc_probability(evidence_for: &str, evidence_against: &str) -> f64 {
    let ef: Vec<f32> = serde_json::from_str(evidence_for).unwrap_or_default();
    let ea: Vec<f32> = serde_json::from_str(evidence_against).unwrap_or_default();
    probability::calculate(&ef, &ea) as f64
}

/// Calculate probability shift when adding new evidence.
/// Returns JSON: {"probability": 0.65, "shift": 0.03}
#[wasm_bindgen]
pub fn calc_shift(
    existing_for: &str,
    existing_against: &str,
    new_for: &str,
    new_against: &str,
) -> String {
    let ef: Vec<f32> = serde_json::from_str(existing_for).unwrap_or_default();
    let ea: Vec<f32> = serde_json::from_str(existing_against).unwrap_or_default();
    let nf: Vec<f32> = serde_json::from_str(new_for).unwrap_or_default();
    let na: Vec<f32> = serde_json::from_str(new_against).unwrap_or_default();

    let (prob, shift) = probability::calculate_with_shift(&ef, &ea, &nf, &na);
    serde_json::json!({
        "probability": (prob * 1000.0).round() / 1000.0,
        "shift": (shift * 1000.0).round() / 1000.0,
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Source tier assessment
// ---------------------------------------------------------------------------

/// Look up domain confidence and tier.
/// Returns JSON: {"confidence": 0.85, "tier": 2}
#[wasm_bindgen]
pub fn assess_source(url: &str) -> String {
    let (conf, tier) = tiers::domain_confidence(url);
    serde_json::json!({
        "confidence": (conf * 100.0).round() / 100.0,
        "tier": tier,
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Article classification
// ---------------------------------------------------------------------------

/// Classify a news article by title.
/// Returns JSON: {"topics": ["sanctions", "economic"], "confidence": 0.85, "tier": 2}
#[wasm_bindgen]
pub fn classify_article(title: &str, domain: &str) -> String {
    let result = classifier::classify_full(title, domain);
    serde_json::to_string(&result).unwrap_or_default()
}

/// Get the relationship type (supports/weakens) for a topic-prediction pair.
#[wasm_bindgen]
pub fn topic_relationship(topic: &str, prediction_category: &str) -> String {
    classifier::topic_prediction_relationship(topic, prediction_category).to_string()
}

// ---------------------------------------------------------------------------
// Query generation
// ---------------------------------------------------------------------------

/// Generate GDELT queries for a target country.
/// Returns JSON array of {query, topics} objects.
#[wasm_bindgen]
pub fn generate_queries(country: &str) -> String {
    let queries = queries::gdelt_queries(country);
    serde_json::to_string(&queries).unwrap_or_default()
}

/// Generate targeted search queries for a specific hypothesis.
/// Returns JSON array of query strings tailored to the assessment.
#[wasm_bindgen]
pub fn hypothesis_queries(country: &str, title: &str, category: &str) -> String {
    let mut queries = Vec::new();
    let c = country;

    // Category-specific queries
    match category {
        "political" => {
            queries.push(format!("{c} regime change"));
            queries.push(format!("{c} protests demonstrations"));
            queries.push(format!("{c} opposition movement"));
            queries.push(format!("{c} political crisis"));
            queries.push(format!("{c} crackdown dissidents"));
            queries.push(format!("{c} reform revolution"));
            queries.push(format!("{c} rally unrest riot"));
            queries.push(format!("{c} government stability"));
            queries.push(format!("{c} coup uprising"));
            queries.push(format!("{c} authoritarian repression"));
        }
        "military" => {
            queries.push(format!("{c} military buildup"));
            queries.push(format!("{c} troop deployment"));
            queries.push(format!("{c} arms weapons"));
            queries.push(format!("{c} military exercise"));
            queries.push(format!("{c} defense spending"));
        }
        "economic" => {
            queries.push(format!("{c} economy GDP"));
            queries.push(format!("{c} sanctions impact"));
            queries.push(format!("{c} inflation recession"));
            queries.push(format!("{c} trade deficit"));
            queries.push(format!("{c} currency crisis"));
            queries.push(format!("{c} economic reform"));
        }
        "conflict" => {
            queries.push(format!("{c} war offensive"));
            queries.push(format!("{c} battle frontline"));
            queries.push(format!("{c} casualties attack"));
            queries.push(format!("{c} conflict escalation"));
            queries.push(format!("{c} ceasefire talks"));
        }
        "diplomatic" => {
            queries.push(format!("{c} diplomacy negotiations"));
            queries.push(format!("{c} peace talks summit"));
            queries.push(format!("{c} international relations"));
            queries.push(format!("{c} ambassador sanctions"));
            queries.push(format!("{c} treaty agreement"));
        }
        "nuclear" => {
            queries.push(format!("{c} nuclear weapons"));
            queries.push(format!("{c} nuclear program enrichment"));
            queries.push(format!("{c} IAEA inspection"));
            queries.push(format!("{c} nuclear escalation"));
            queries.push(format!("{c} missile test"));
        }
        "energy" => {
            queries.push(format!("{c} oil gas pipeline"));
            queries.push(format!("{c} energy exports"));
            queries.push(format!("{c} OPEC production"));
            queries.push(format!("{c} energy crisis"));
            queries.push(format!("{c} renewable transition"));
        }
        "territorial" => {
            queries.push(format!("{c} territorial dispute"));
            queries.push(format!("{c} sovereignty border"));
            queries.push(format!("{c} annexation occupation"));
            queries.push(format!("{c} maritime claims"));
            queries.push(format!("{c} separatist movement"));
        }
        "humanitarian" => {
            queries.push(format!("{c} humanitarian crisis"));
            queries.push(format!("{c} refugees civilians"));
            queries.push(format!("{c} war crimes tribunal"));
            queries.push(format!("{c} human rights"));
            queries.push(format!("{c} aid humanitarian"));
        }
        "hybrid" => {
            queries.push(format!("{c} hybrid warfare"));
            queries.push(format!("{c} disinformation propaganda"));
            queries.push(format!("{c} cyber attack"));
            queries.push(format!("{c} election interference"));
            queries.push(format!("{c} influence operation"));
        }
        _ => {
            queries.push(format!("{c} {category}"));
        }
    }

    // Also extract key terms from the hypothesis title itself
    let title_lower = title.to_lowercase();
    let stop_words = ["the","a","an","in","of","for","and","or","to","will","be","is","are",
                       "was","were","has","have","been","with","from","that","this","by","at",
                       "on","about","would","could","should","may","might"];
    let key_words: Vec<&str> = title_lower.split_whitespace()
        .filter(|w| w.len() > 3 && !stop_words.contains(w) && *w != &country.to_lowercase())
        .collect();
    if key_words.len() >= 2 {
        queries.push(format!("{c} {}", key_words.join(" ")));
    }

    serde_json::to_string(&queries).unwrap_or_default()
}

/// Get ISO-3 country code for World Bank API.
#[wasm_bindgen]
pub fn country_iso3(name: &str) -> String {
    queries::country_to_iso3(name).unwrap_or("").to_string()
}

/// Get Wikidata entity ID for a country.
#[wasm_bindgen]
pub fn country_wikidata_id(name: &str) -> String {
    queries::country_to_wikidata(name).unwrap_or("").to_string()
}

/// Generate Wikidata SPARQL query for country borders.
#[wasm_bindgen]
pub fn sparql_borders(wikidata_id: &str) -> String {
    queries::wikidata_country_query(wikidata_id)
}

/// Generate Wikidata SPARQL query for organization memberships.
#[wasm_bindgen]
pub fn sparql_memberships(wikidata_id: &str) -> String {
    queries::wikidata_memberships_query(wikidata_id)
}

/// Generate Wikidata SPARQL query for leaders.
#[wasm_bindgen]
pub fn sparql_leaders(wikidata_id: &str) -> String {
    queries::wikidata_leaders_query(wikidata_id)
}

/// Get World Bank indicator codes as JSON.
#[wasm_bindgen]
pub fn world_bank_indicators() -> String {
    let indicators: Vec<serde_json::Value> = queries::WORLD_BANK_INDICATORS
        .iter()
        .map(|i| {
            serde_json::json!({
                "code": i.code,
                "name": i.name,
                "format": i.format,
            })
        })
        .collect();
    serde_json::to_string(&indicators).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// License & geo-restriction
// ---------------------------------------------------------------------------

/// Validate a license key. Returns JSON: {"valid": true/false, "reason": "..."}
#[wasm_bindgen]
pub fn validate_license(key: &str) -> String {
    let status = license::validate_license(key);
    let (valid, reason) = match status {
        license::LicenseStatus::Valid => (true, "valid"),
        license::LicenseStatus::Expired => (false, "license expired"),
        license::LicenseStatus::InvalidKey => (false, "invalid license key"),
        license::LicenseStatus::BlockedCountry => (false, "blocked region"),
        license::LicenseStatus::NoLicense => (false, "no license provided"),
    };
    serde_json::json!({"valid": valid, "reason": reason}).to_string()
}

/// Check if the current locale/region is blocked.
/// Pass navigator.language or Intl locale string.
#[wasm_bindgen]
pub fn check_locale(locale: &str) -> String {
    let blocked = license::is_locale_blocked(locale);
    serde_json::json!({"blocked": blocked, "locale": locale}).to_string()
}

/// Check if a country code is blocked from analysis.
#[wasm_bindgen]
pub fn is_blocked_country(country_code: &str) -> bool {
    license::is_country_blocked(country_code)
}

// ---------------------------------------------------------------------------
// Ingestion engine (runs in WASM -- logic hidden)
// ---------------------------------------------------------------------------

/// Process a batch of raw GDELT articles and classify them.
/// Input: JSON array of {title, url, domain, date} objects.
/// Output: JSON array of {label, type, confidence, source, topics, relationship} objects
/// ready to be stored in engram via /store and /relate.
#[wasm_bindgen]
pub fn process_articles(articles_json: &str, target_country: &str) -> String {
    let articles: Vec<serde_json::Value> =
        serde_json::from_str(articles_json).unwrap_or_default();

    let mut results = Vec::new();
    let _country_lower = target_country.to_lowercase();

    for art in &articles {
        let title = art["title"].as_str().unwrap_or("");
        let url = art["url"].as_str().unwrap_or("");
        let domain = art["domain"].as_str().unwrap_or("");
        let date = art["seendate"].as_str().unwrap_or("").get(..8).unwrap_or("");

        if title.is_empty() || url.is_empty() {
            continue;
        }

        // Classify
        let classification = classifier::classify_full(title, domain);
        if classification.topics.is_empty() {
            continue;
        }

        // Generate a deterministic short hash for the label
        let hash = simple_hash(url);
        let label = format!("News:{date}:{hash}");

        // Sanitize title (ASCII-safe for storage)
        let safe_title: String = title.chars()
            .map(|c| if c.is_ascii() { c } else { '?' })
            .take(80)
            .collect();

        results.push(serde_json::json!({
            "label": label,
            "type": "news_article",
            "confidence": classification.confidence,
            "source": format!("Source:{domain}"),
            "properties": {
                "title": safe_title,
                "url": url,
                "date": date,
                "domain": domain,
                "topics": classification.topics.join(","),
                "target_country": target_country,
            },
            "topics": classification.topics,
            "tier": classification.tier,
        }));
    }

    serde_json::to_string(&results).unwrap_or_default()
}

fn simple_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:012x}", h & 0xffffffffffff)
}
