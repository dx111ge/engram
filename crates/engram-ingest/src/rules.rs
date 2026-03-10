/// Rule-based NER: regex patterns and per-language rule files.
///
/// Rules are defined in TOML format and compiled to regex patterns.
/// Each rule maps a pattern to an entity type with a confidence score.

use std::collections::HashMap;

use crate::traits::Extractor;
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// A single NER rule: pattern -> entity type.
#[derive(Debug, Clone)]
pub struct NerRule {
    /// Human-readable rule name.
    pub name: String,
    /// Compiled regex pattern.
    pattern: regex::Regex,
    /// Entity type to assign (PERSON, ORG, LOC, DATE, etc.).
    pub entity_type: String,
    /// Confidence for matches from this rule.
    pub confidence: f32,
    /// Languages this rule applies to. Empty = all.
    pub languages: Vec<String>,
    /// Named capture group to use as entity text (default: entire match).
    pub capture_group: Option<String>,
}

/// A set of NER rules, optionally grouped by language.
pub struct RuleBasedNer {
    /// Rules that apply to all languages.
    global_rules: Vec<NerRule>,
    /// Language-specific rules (language code -> rules).
    lang_rules: HashMap<String, Vec<NerRule>>,
}

/// Builder for constructing rule definitions before compilation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuleDef {
    pub name: String,
    pub pattern: String,
    pub entity_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub languages: Vec<String>,
    pub capture_group: Option<String>,
}

fn default_confidence() -> f32 {
    0.80
}

/// TOML rule file format.
#[derive(Debug, serde::Deserialize)]
pub struct RuleFile {
    #[serde(default)]
    pub rules: Vec<RuleDef>,
}

impl RuleBasedNer {
    /// Create an empty rule set.
    pub fn new() -> Self {
        Self {
            global_rules: Vec::new(),
            lang_rules: HashMap::new(),
        }
    }

    /// Add a single compiled rule.
    pub fn add_rule(&mut self, rule: NerRule) {
        if rule.languages.is_empty() {
            self.global_rules.push(rule);
        } else {
            for lang in &rule.languages {
                self.lang_rules
                    .entry(lang.clone())
                    .or_default()
                    .push(rule.clone());
            }
        }
    }

    /// Compile and add a rule from a definition.
    pub fn add_rule_def(&mut self, def: &RuleDef) -> Result<(), crate::IngestError> {
        let pattern = regex::Regex::new(&def.pattern).map_err(|e| {
            crate::IngestError::Config(format!("invalid regex in rule '{}': {}", def.name, e))
        })?;

        let rule = NerRule {
            name: def.name.clone(),
            pattern,
            entity_type: def.entity_type.clone(),
            confidence: def.confidence,
            languages: def.languages.clone(),
            capture_group: def.capture_group.clone(),
        };

        self.add_rule(rule);
        Ok(())
    }

    /// Load rules from a TOML string.
    pub fn load_toml(&mut self, toml_str: &str) -> Result<usize, crate::IngestError> {
        let file: RuleFile = toml::from_str(toml_str).map_err(|e| {
            crate::IngestError::Config(format!("invalid TOML rule file: {}", e))
        })?;

        let count = file.rules.len();
        for def in &file.rules {
            self.add_rule_def(def)?;
        }

        Ok(count)
    }

    /// Create a rule set with built-in patterns for common entity types.
    pub fn with_defaults() -> Self {
        let mut ner = Self::new();

        // Email addresses
        let _ = ner.add_rule_def(&RuleDef {
            name: "email".into(),
            pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".into(),
            entity_type: "EMAIL".into(),
            confidence: 0.95,
            languages: vec![],
            capture_group: None,
        });

        // URLs
        let _ = ner.add_rule_def(&RuleDef {
            name: "url".into(),
            pattern: r#"https?://[^\s<>"']+"#.into(),
            entity_type: "URL".into(),
            confidence: 0.95,
            languages: vec![],
            capture_group: None,
        });

        // IP addresses
        let _ = ner.add_rule_def(&RuleDef {
            name: "ipv4".into(),
            pattern: r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b".into(),
            entity_type: "IP_ADDRESS".into(),
            confidence: 0.90,
            languages: vec![],
            capture_group: None,
        });

        // ISO dates (YYYY-MM-DD)
        let _ = ner.add_rule_def(&RuleDef {
            name: "iso_date".into(),
            pattern: r"\b\d{4}-\d{2}-\d{2}\b".into(),
            entity_type: "DATE".into(),
            confidence: 0.90,
            languages: vec![],
            capture_group: None,
        });

        // Monetary values ($1,234.56 or €1.234,56)
        let _ = ner.add_rule_def(&RuleDef {
            name: "monetary".into(),
            pattern: r"[$€£¥]\s?\d[\d.,]*\d".into(),
            entity_type: "MONETARY".into(),
            confidence: 0.85,
            languages: vec![],
            capture_group: None,
        });

        // Percentages
        let _ = ner.add_rule_def(&RuleDef {
            name: "percentage".into(),
            pattern: r"\b\d+(?:\.\d+)?%".into(),
            entity_type: "PERCENTAGE".into(),
            confidence: 0.90,
            languages: vec![],
            capture_group: None,
        });

        ner
    }

    /// Get rules applicable to a given language.
    fn rules_for_lang(&self, lang: &str) -> impl Iterator<Item = &NerRule> {
        let lang_specific = self.lang_rules.get(lang).into_iter().flatten();
        self.global_rules.iter().chain(lang_specific)
    }

    /// Total number of compiled rules.
    pub fn rule_count(&self) -> usize {
        let lang_count: usize = self.lang_rules.values().map(|v| v.len()).sum();
        self.global_rules.len() + lang_count
    }
}

impl Default for RuleBasedNer {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for RuleBasedNer {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();

        for rule in self.rules_for_lang(&lang.code) {
            for mat in rule.pattern.find_iter(text) {
                let matched_text = if let Some(ref group_name) = rule.capture_group {
                    // Use named capture group if specified
                    rule.pattern
                        .captures(mat.as_str())
                        .and_then(|caps| caps.name(group_name))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_else(|| mat.as_str().to_string())
                } else {
                    mat.as_str().to_string()
                };

                entities.push(ExtractedEntity {
                    text: matched_text,
                    entity_type: rule.entity_type.clone(),
                    span: (mat.start(), mat.end()),
                    confidence: rule.confidence,
                    method: ExtractionMethod::RuleBased,
                    language: lang.code.clone(),
                    resolved_to: None,
                });
            }
        }

        // Deduplicate overlapping spans (keep higher confidence)
        entities.sort_by(|a, b| {
            a.span
                .0
                .cmp(&b.span.0)
                .then(b.confidence.partial_cmp(&a.confidence).unwrap())
        });

        let mut deduped: Vec<ExtractedEntity> = Vec::new();
        for entity in entities {
            let overlaps = deduped
                .iter()
                .any(|e| e.span.0 <= entity.span.0 && e.span.1 > entity.span.0);
            if !overlaps {
                deduped.push(entity);
            }
        }

        deduped
    }

    fn name(&self) -> &str {
        "rule-based-ner"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::RuleBased
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // global rules apply to all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_lang() -> DetectedLanguage {
        DetectedLanguage {
            code: "en".into(),
            confidence: 1.0,
        }
    }

    #[test]
    fn default_rules_extract_emails() {
        let ner = RuleBasedNer::with_defaults();
        let entities = ner.extract("Contact us at hello@example.com for info", &en_lang());

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "hello@example.com");
        assert_eq!(entities[0].entity_type, "EMAIL");
    }

    #[test]
    fn default_rules_extract_urls() {
        let ner = RuleBasedNer::with_defaults();
        let entities = ner.extract("Visit https://example.com/path for details", &en_lang());

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "https://example.com/path");
        assert_eq!(entities[0].entity_type, "URL");
    }

    #[test]
    fn default_rules_extract_dates() {
        let ner = RuleBasedNer::with_defaults();
        let entities = ner.extract("The event is on 2026-03-15 in Berlin", &en_lang());

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "2026-03-15");
        assert_eq!(entities[0].entity_type, "DATE");
    }

    #[test]
    fn default_rules_extract_monetary() {
        let ner = RuleBasedNer::with_defaults();
        let entities = ner.extract("Revenue was $1,234.56 this quarter", &en_lang());

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "MONETARY");
    }

    #[test]
    fn default_rules_extract_percentages() {
        let ner = RuleBasedNer::with_defaults();
        let entities = ner.extract("Growth was 12.5% year over year", &en_lang());

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "12.5%");
        assert_eq!(entities[0].entity_type, "PERCENTAGE");
    }

    #[test]
    fn custom_rule_from_toml() {
        let toml = r#"
[[rules]]
name = "ticker"
pattern = '\b[A-Z]{1,5}\b'
entity_type = "TICKER"
confidence = 0.70
languages = ["en"]
"#;

        let mut ner = RuleBasedNer::new();
        let count = ner.load_toml(toml).unwrap();
        assert_eq!(count, 1);

        let entities = ner.extract("AAPL reported earnings today", &en_lang());
        assert!(entities.iter().any(|e| e.text == "AAPL" && e.entity_type == "TICKER"));
    }

    #[test]
    fn language_specific_rules() {
        let mut ner = RuleBasedNer::new();
        ner.add_rule_def(&RuleDef {
            name: "german_plz".into(),
            pattern: r"\b\d{5}\b".into(),
            entity_type: "POSTAL_CODE".into(),
            confidence: 0.80,
            languages: vec!["de".into()],
            capture_group: None,
        })
        .unwrap();

        // Should match for German text
        let de = DetectedLanguage {
            code: "de".into(),
            confidence: 1.0,
        };
        let entities = ner.extract("PLZ 10115 Berlin", &de);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "POSTAL_CODE");

        // Should NOT match for English (rule is de-only)
        let entities = ner.extract("ZIP 10115 somewhere", &en_lang());
        assert!(entities.is_empty());
    }

    #[test]
    fn multiple_matches_in_text() {
        let ner = RuleBasedNer::with_defaults();
        let text = "Send to user@test.com or admin@test.com before 2026-12-31";
        let entities = ner.extract(text, &en_lang());

        let types: Vec<&str> = entities.iter().map(|e| e.entity_type.as_str()).collect();
        assert!(types.contains(&"EMAIL"));
        assert!(types.contains(&"DATE"));
        assert!(entities.len() >= 3); // 2 emails + 1 date
    }

    #[test]
    fn invalid_regex_returns_error() {
        let mut ner = RuleBasedNer::new();
        let result = ner.add_rule_def(&RuleDef {
            name: "bad".into(),
            pattern: r"[invalid".into(),
            entity_type: "X".into(),
            confidence: 0.5,
            languages: vec![],
            capture_group: None,
        });
        assert!(result.is_err());
    }
}
