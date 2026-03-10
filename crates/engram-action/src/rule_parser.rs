/// TOML rule parser for action rules.
///
/// Rules are defined in TOML format and parsed into `ActionRule` structs.
/// Supports both single-rule and multi-rule files.

use crate::error::ActionError;
use crate::types::ActionRule;

/// Parse action rules from a TOML string.
///
/// Supports two formats:
/// 1. Single rule: `[rule]` section
/// 2. Multiple rules: `[[rules]]` array
pub fn parse_rules(toml_str: &str) -> Result<Vec<ActionRule>, ActionError> {
    // Try multi-rule format first
    #[derive(serde::Deserialize)]
    struct MultiRules {
        rules: Vec<ActionRule>,
    }

    if let Ok(multi) = toml::from_str::<MultiRules>(toml_str) {
        return Ok(multi.rules);
    }

    // Try single rule format
    #[derive(serde::Deserialize)]
    struct SingleRule {
        rule: ActionRule,
    }

    if let Ok(single) = toml::from_str::<SingleRule>(toml_str) {
        return Ok(vec![single.rule]);
    }

    // Try direct ActionRule (no wrapper)
    if let Ok(rule) = toml::from_str::<ActionRule>(toml_str) {
        return Ok(vec![rule]);
    }

    Err(ActionError::RuleParse(
        "failed to parse TOML as action rules".into(),
    ))
}

/// Validate a parsed rule for consistency.
pub fn validate_rule(rule: &ActionRule) -> Result<(), ActionError> {
    if rule.id.is_empty() {
        return Err(ActionError::RuleParse("rule id cannot be empty".into()));
    }

    if rule.triggers.is_empty() {
        return Err(ActionError::RuleParse(format!(
            "rule '{}' has no triggers",
            rule.id
        )));
    }

    if rule.effects.is_empty() {
        return Err(ActionError::RuleParse(format!(
            "rule '{}' has no effects",
            rule.id
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_rule() {
        let toml = r#"
            id = "test-rule"
            description = "A test rule"

            [[triggers]]
            type = "fact_stored"
            entity_type = "PERSON"

            [[conditions]]
            type = "confidence_above"
            threshold = 0.5

            [[effects]]
            type = "log"
            message = "New person stored"
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "test-rule");
        assert_eq!(rules[0].triggers.len(), 1);
        assert_eq!(rules[0].conditions.len(), 1);
        assert_eq!(rules[0].effects.len(), 1);
    }

    #[test]
    fn parse_multi_rule() {
        let toml = r#"
            [[rules]]
            id = "rule-1"
            [[rules.triggers]]
            type = "fact_stored"
            [[rules.effects]]
            type = "log"
            message = "rule 1 fired"

            [[rules]]
            id = "rule-2"
            [[rules.triggers]]
            type = "edge_created"
            [[rules.effects]]
            type = "log"
            message = "rule 2 fired"
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "rule-1");
        assert_eq!(rules[1].id, "rule-2");
    }

    #[test]
    fn parse_rule_with_safety() {
        let toml = r#"
            id = "safe-rule"
            [[triggers]]
            type = "fact_updated"
            [[effects]]
            type = "confidence_cascade"
            rel_types = ["works_at"]
            depth = 2
            decay_factor = 0.8

            [safety]
            cooldown_secs = 60
            max_chain_depth = 3
            max_effects = 10
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules[0].safety.cooldown_secs, 60);
        assert_eq!(rules[0].safety.max_chain_depth, 3);
        assert_eq!(rules[0].safety.max_effects, 10);
    }

    #[test]
    fn validate_empty_id_fails() {
        let rule = ActionRule {
            id: String::new(),
            description: None,
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            effects: vec![],
            safety: Default::default(),
            priority: 0,
        };
        assert!(validate_rule(&rule).is_err());
    }

    #[test]
    fn parse_webhook_effect() {
        let toml = r#"
            id = "webhook-rule"
            [[triggers]]
            type = "conflict_detected"
            [[effects]]
            type = "webhook"
            url = "https://example.com/hook"
            [effects.headers]
            Authorization = "Bearer token123"
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn parse_timer_trigger() {
        let toml = r#"
            id = "timer-rule"
            [[triggers]]
            type = "timer"
            interval_secs = 300
            [[effects]]
            type = "log"
            message = "timer tick"
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn parse_ingest_job_effect() {
        let toml = r#"
            id = "enrich-rule"
            [[triggers]]
            type = "fact_stored"
            entity_type = "PERSON"
            [[effects]]
            type = "create_ingest_job"
            query_template = "{entity} biography"
            source = "web-search"
            reconcile = "merge"
        "#;

        let rules = parse_rules(toml).unwrap();
        assert_eq!(rules.len(), 1);
    }
}
