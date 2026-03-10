/// Action engine: event subscriber, rule evaluator, effect executor.
///
/// Subscribes to the graph's EventBus and evaluates rules against
/// incoming events. When rules fire, executes their effects with
/// safety constraints enforced.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use engram_core::events::GraphEvent;
use engram_core::graph::Graph;

use crate::condition::evaluate_conditions;
use crate::effects::{execute_effect, preview_effect};
use crate::types::{ActionReport, ActionRule, DryRunResult, RuleResult, Trigger};

/// The action engine.
pub struct ActionEngine {
    /// Registered rules, sorted by priority (descending).
    rules: Vec<ActionRule>,
    /// Graph reference for condition evaluation and effects.
    graph: Arc<RwLock<Graph>>,
    /// Cooldown tracking: rule_id -> last fired instant.
    cooldowns: HashMap<String, Instant>,
    /// Current chain depth (for cascade safety).
    chain_depth: u32,
    /// Global effect budget per execution cycle.
    effect_budget: u32,
    /// Effects executed in current cycle.
    effects_this_cycle: u32,
}

impl ActionEngine {
    pub fn new(graph: Arc<RwLock<Graph>>) -> Self {
        Self {
            rules: Vec::new(),
            graph,
            cooldowns: HashMap::new(),
            chain_depth: 0,
            effect_budget: 1000,
            effects_this_cycle: 0,
        }
    }

    /// Load rules, replacing existing ones.
    pub fn load_rules(&mut self, rules: Vec<ActionRule>) {
        let mut rules = rules;
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        self.rules = rules;
    }

    /// Append rules to the existing set.
    pub fn append_rules(&mut self, rules: Vec<ActionRule>) {
        self.rules.extend(rules);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < before
    }

    /// List all rule IDs.
    pub fn list_rules(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.id.as_str()).collect()
    }

    /// Get a rule by ID.
    pub fn get_rule(&self, id: &str) -> Option<&ActionRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Number of loaded rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Set the global effect budget per cycle.
    pub fn set_effect_budget(&mut self, budget: u32) {
        self.effect_budget = budget;
    }

    /// Process a single event against all rules.
    pub fn process_event(&mut self, event: &GraphEvent) -> ActionReport {
        let mut report = ActionReport::default();
        report.events_processed = 1;

        self.effects_this_cycle = 0;

        for rule in &self.rules.clone() {
            if !rule.enabled {
                continue;
            }

            report.rules_evaluated += 1;
            let result = self.evaluate_rule(rule, event);

            if result.triggered && result.conditions_passed {
                report.rules_fired += 1;
                report.effects_executed += result.effects_executed;
                report.effects_skipped += result.effects_skipped;
            }

            report.errors.extend(result.errors);
        }

        report
    }

    /// Evaluate a single rule against an event.
    fn evaluate_rule(&mut self, rule: &ActionRule, event: &GraphEvent) -> RuleResult {
        let mut result = RuleResult {
            rule_id: rule.id.clone(),
            ..Default::default()
        };

        // Check if any trigger matches
        if !rule.triggers.iter().any(|t| trigger_matches(t, event)) {
            return result;
        }
        result.triggered = true;

        // Check cooldown
        if rule.safety.cooldown_secs > 0 {
            if let Some(last) = self.cooldowns.get(&rule.id) {
                if last.elapsed().as_secs() < rule.safety.cooldown_secs {
                    result.effects_skipped = rule.effects.len() as u32;
                    return result;
                }
            }
        }

        // Check chain depth
        if self.chain_depth >= rule.safety.max_chain_depth {
            result.errors.push(format!(
                "rule '{}': max chain depth {} exceeded",
                rule.id, rule.safety.max_chain_depth
            ));
            return result;
        }

        // Evaluate conditions
        let graph = match self.graph.read() {
            Ok(g) => g,
            Err(_) => {
                result.errors.push("graph lock poisoned".into());
                return result;
            }
        };

        let (passed, _) = evaluate_conditions(&rule.conditions, event, &graph);
        drop(graph);

        result.conditions_passed = passed;
        if !passed {
            return result;
        }

        // Execute effects
        self.chain_depth += 1;
        self.cooldowns.insert(rule.id.clone(), Instant::now());

        for effect in &rule.effects {
            // Check per-rule effect budget
            if result.effects_executed >= rule.safety.max_effects {
                result.effects_skipped += 1;
                continue;
            }

            // Check global effect budget
            if self.effects_this_cycle >= self.effect_budget {
                result.effects_skipped += 1;
                result.errors.push("global effect budget exhausted".into());
                continue;
            }

            match execute_effect(effect, event, &self.graph) {
                Ok(()) => {
                    result.effects_executed += 1;
                    self.effects_this_cycle += 1;
                }
                Err(e) => {
                    result.errors.push(format!("effect error: {}", e));
                }
            }
        }

        self.chain_depth -= 1;
        result
    }

    /// Dry run: evaluate rules without executing effects.
    pub fn dry_run(&self, event: &GraphEvent) -> Vec<DryRunResult> {
        let graph = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };

        self.rules
            .iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let would_fire = rule.triggers.iter().any(|t| trigger_matches(t, event));
                let (conditions_passed, condition_results) = if would_fire {
                    evaluate_conditions(&rule.conditions, event, &graph)
                } else {
                    (false, vec![])
                };

                let effects = if would_fire && conditions_passed {
                    rule.effects
                        .iter()
                        .map(|e| preview_effect(e, event))
                        .collect()
                } else {
                    vec![]
                };

                DryRunResult {
                    rule_id: rule.id.clone(),
                    would_fire: would_fire && conditions_passed,
                    conditions: condition_results,
                    effects,
                }
            })
            .collect()
    }

    /// Get timer-based rules and their intervals.
    pub fn timer_rules(&self) -> Vec<(&str, u64)> {
        self.rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                r.triggers.iter().find_map(|t| {
                    if let Trigger::Timer { interval_secs } = t {
                        Some((r.id.as_str(), *interval_secs))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

/// Check if a trigger matches an event.
fn trigger_matches(trigger: &Trigger, event: &GraphEvent) -> bool {
    match (trigger, event) {
        (
            Trigger::FactStored {
                label_pattern,
                entity_type,
            },
            GraphEvent::FactStored {
                label,
                entity_type: evt_type,
                ..
            },
        ) => {
            if let Some(pattern) = label_pattern {
                if !glob_match(pattern, label) {
                    return false;
                }
            }
            if let Some(etype) = entity_type {
                match evt_type {
                    Some(t) => t.eq_ignore_ascii_case(etype),
                    None => false,
                }
            } else {
                true
            }
        }

        (
            Trigger::FactUpdated {
                label_pattern,
                threshold,
                direction,
            },
            GraphEvent::FactUpdated {
                label,
                old_confidence,
                new_confidence,
                ..
            },
        ) => {
            if let Some(pattern) = label_pattern {
                if !glob_match(pattern, label) {
                    return false;
                }
            }
            if let Some(thresh) = threshold {
                let crossed_up = *old_confidence < *thresh && *new_confidence >= *thresh;
                let crossed_down = *old_confidence >= *thresh && *new_confidence < *thresh;
                match direction.as_deref() {
                    Some("up") => crossed_up,
                    Some("down") => crossed_down,
                    _ => crossed_up || crossed_down,
                }
            } else {
                true
            }
        }

        (Trigger::EdgeCreated { rel_type }, GraphEvent::EdgeCreated { rel_type: evt_rt, .. }) => {
            rel_type.as_ref().map_or(true, |rt| evt_rt.eq_ignore_ascii_case(rt))
        }

        (Trigger::PropertyChanged { key }, GraphEvent::PropertyChanged { key: evt_key, .. }) => {
            key.as_ref().map_or(true, |k| evt_key.eq_ignore_ascii_case(k))
        }

        (
            Trigger::ThresholdCrossed { direction },
            GraphEvent::ThresholdCrossed {
                direction: evt_dir,
                ..
            },
        ) => {
            use engram_core::events::ThresholdDirection;
            direction.as_ref().map_or(true, |d| match d.as_str() {
                "up" => *evt_dir == ThresholdDirection::Up,
                "down" => *evt_dir == ThresholdDirection::Down,
                _ => true,
            })
        }

        (Trigger::ConflictDetected, GraphEvent::ConflictDetected { .. }) => true,

        (Trigger::Timer { .. }, GraphEvent::TimerTick { .. }) => true,

        _ => false,
    }
}

/// Simple glob matching: supports `*` as wildcard.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return text.ends_with(suffix);
    }

    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return text.starts_with(parts[0]) && text.ends_with(parts[1]);
        }
    }

    pattern == text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn test_graph() -> (tempfile::TempDir, Arc<RwLock<Graph>>) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = Graph::create(&path).unwrap();
        (dir, Arc::new(RwLock::new(graph)))
    }

    fn make_log_rule(id: &str) -> ActionRule {
        ActionRule {
            id: id.into(),
            description: None,
            enabled: true,
            triggers: vec![Trigger::FactStored {
                label_pattern: None,
                entity_type: None,
            }],
            conditions: vec![],
            effects: vec![Effect::Log {
                level: None,
                message: "rule fired for {entity}".into(),
            }],
            safety: SafetyConfig::default(),
            priority: 0,
        }
    }

    #[test]
    fn engine_processes_event() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);
        engine.load_rules(vec![make_log_rule("test")]);

        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("Apple"),
            confidence: 0.8,
            source: Arc::from("test"),
            entity_type: None,
        };

        let report = engine.process_event(&event);
        assert_eq!(report.rules_evaluated, 1);
        assert_eq!(report.rules_fired, 1);
        assert_eq!(report.effects_executed, 1);
    }

    #[test]
    fn disabled_rules_skipped() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);
        let mut rule = make_log_rule("test");
        rule.enabled = false;
        engine.load_rules(vec![rule]);

        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("X"),
            confidence: 0.5,
            source: Arc::from("test"),
            entity_type: None,
        };

        let report = engine.process_event(&event);
        assert_eq!(report.rules_evaluated, 0);
        assert_eq!(report.rules_fired, 0);
    }

    #[test]
    fn condition_filters_rule() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);

        let mut rule = make_log_rule("high-conf");
        rule.conditions = vec![Condition::ConfidenceAbove { threshold: 0.9 }];
        engine.load_rules(vec![rule]);

        // Low confidence event should not fire the rule
        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("X"),
            confidence: 0.3,
            source: Arc::from("test"),
            entity_type: None,
        };

        let report = engine.process_event(&event);
        assert_eq!(report.rules_evaluated, 1);
        assert_eq!(report.rules_fired, 0);
    }

    #[test]
    fn entity_type_trigger_filter() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);

        let rule = ActionRule {
            id: "person-only".into(),
            description: None,
            enabled: true,
            triggers: vec![Trigger::FactStored {
                label_pattern: None,
                entity_type: Some("PERSON".into()),
            }],
            conditions: vec![],
            effects: vec![Effect::Log {
                level: None,
                message: "person found".into(),
            }],
            safety: SafetyConfig::default(),
            priority: 0,
        };
        engine.load_rules(vec![rule]);

        // ORG should not trigger
        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("Apple"),
            confidence: 0.8,
            source: Arc::from("test"),
            entity_type: Some(Arc::from("ORG")),
        };

        let report = engine.process_event(&event);
        assert_eq!(report.rules_fired, 0);

        // PERSON should trigger
        let event = GraphEvent::FactStored {
            node_id: 2,
            label: Arc::from("Tim Cook"),
            confidence: 0.8,
            source: Arc::from("test"),
            entity_type: Some(Arc::from("PERSON")),
        };

        let report = engine.process_event(&event);
        assert_eq!(report.rules_fired, 1);
    }

    #[test]
    fn effect_budget_enforced() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);
        engine.set_effect_budget(1); // only allow 1 effect

        let rule = ActionRule {
            id: "multi-effect".into(),
            description: None,
            enabled: true,
            triggers: vec![Trigger::FactStored {
                label_pattern: None,
                entity_type: None,
            }],
            conditions: vec![],
            effects: vec![
                Effect::Log { level: None, message: "1".into() },
                Effect::Log { level: None, message: "2".into() },
                Effect::Log { level: None, message: "3".into() },
            ],
            safety: SafetyConfig::default(),
            priority: 0,
        };
        engine.load_rules(vec![rule]);

        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("X"),
            confidence: 0.5,
            source: Arc::from("test"),
            entity_type: None,
        };

        let report = engine.process_event(&event);
        assert_eq!(report.effects_executed, 1);
        assert_eq!(report.effects_skipped, 2);
    }

    #[test]
    fn dry_run_does_not_execute() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);
        engine.load_rules(vec![make_log_rule("test")]);

        let event = GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("Apple"),
            confidence: 0.8,
            source: Arc::from("test"),
            entity_type: None,
        };

        let results = engine.dry_run(&event);
        assert_eq!(results.len(), 1);
        assert!(results[0].would_fire);
        assert_eq!(results[0].effects.len(), 1);
    }

    #[test]
    fn priority_ordering() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);

        let low = ActionRule {
            priority: 0,
            ..make_log_rule("low")
        };
        let high = ActionRule {
            priority: 10,
            ..make_log_rule("high")
        };

        engine.load_rules(vec![low, high]);
        let rules = engine.list_rules();
        assert_eq!(rules[0], "high"); // high priority first
        assert_eq!(rules[1], "low");
    }

    #[test]
    fn rule_management() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);

        engine.load_rules(vec![make_log_rule("a")]);
        assert_eq!(engine.rule_count(), 1);

        engine.append_rules(vec![make_log_rule("b")]);
        assert_eq!(engine.rule_count(), 2);

        assert!(engine.remove_rule("a"));
        assert_eq!(engine.rule_count(), 1);
        assert_eq!(engine.list_rules(), vec!["b"]);
    }

    #[test]
    fn timer_rules_extracted() {
        let (_dir, graph) = test_graph();
        let mut engine = ActionEngine::new(graph);

        let rule = ActionRule {
            id: "timer-rule".into(),
            description: None,
            enabled: true,
            triggers: vec![Trigger::Timer { interval_secs: 60 }],
            conditions: vec![],
            effects: vec![Effect::Log { level: None, message: "tick".into() }],
            safety: SafetyConfig::default(),
            priority: 0,
        };
        engine.load_rules(vec![rule]);

        let timers = engine.timer_rules();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0], ("timer-rule", 60));
    }

    #[test]
    fn glob_matching() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("Apple*", "Apple Inc"));
        assert!(!glob_match("Apple*", "Google"));
        assert!(glob_match("*Inc", "Apple Inc"));
        assert!(glob_match("App*Inc", "Apple Inc"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "different"));
    }
}
