/// Core types for the action engine.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An action rule: event pattern -> condition -> effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRule {
    /// Unique rule identifier.
    pub id: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Whether the rule is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Event patterns that trigger this rule.
    pub triggers: Vec<Trigger>,
    /// Conditions that must be met (AND logic).
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Effects to execute when triggered and conditions pass.
    pub effects: Vec<Effect>,
    /// Safety constraints.
    #[serde(default)]
    pub safety: SafetyConfig,
    /// Priority (higher = fires first). Default: 0.
    #[serde(default)]
    pub priority: i32,
}

fn default_true() -> bool {
    true
}

/// What triggers a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Fires when a new fact is stored.
    FactStored {
        /// Optional label pattern (glob-style).
        label_pattern: Option<String>,
        /// Optional entity type filter.
        entity_type: Option<String>,
    },
    /// Fires when a fact's confidence changes.
    FactUpdated {
        label_pattern: Option<String>,
        /// Only fire if confidence crosses a threshold.
        threshold: Option<f32>,
        /// Direction of crossing: "up", "down", or "any".
        direction: Option<String>,
    },
    /// Fires when an edge is created.
    EdgeCreated {
        rel_type: Option<String>,
    },
    /// Fires when a property changes.
    PropertyChanged {
        key: Option<String>,
    },
    /// Fires when a confidence threshold is crossed.
    ThresholdCrossed {
        direction: Option<String>,
    },
    /// Fires when a conflict is detected.
    ConflictDetected,
    /// Fires on a timer schedule.
    Timer {
        /// Interval in seconds.
        interval_secs: u64,
    },
}

/// Conditions evaluated against the event and graph state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Node confidence is above a threshold.
    ConfidenceAbove { threshold: f32 },
    /// Node confidence is below a threshold.
    ConfidenceBelow { threshold: f32 },
    /// Node has a specific property.
    HasProperty { key: String, value: Option<String> },
    /// Node is of a specific type.
    HasType { entity_type: String },
    /// Node has a specific edge.
    HasEdge { rel_type: String, direction: Option<String> },
    /// Custom expression (simple key-op-value comparison).
    Expression { left: String, op: String, right: String },
}

/// Effects executed when a rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Cascade confidence change to connected nodes.
    ConfidenceCascade {
        /// Relationship types to follow.
        rel_types: Vec<String>,
        /// Cascade depth limit.
        depth: Option<u32>,
        /// Decay factor per hop (multiplied each step).
        decay_factor: Option<f32>,
    },
    /// Create a new edge.
    CreateEdge {
        from: String,
        to: String,
        rel_type: String,
        confidence: Option<f32>,
    },
    /// Change a node's memory tier.
    TierChange {
        /// Target tier: "core", "active", "archival".
        tier: String,
    },
    /// Set a property on the triggering node.
    SetProperty {
        key: String,
        value: String,
    },
    /// Set a flag on the triggering node.
    Flag {
        message: String,
    },
    /// Send a webhook notification.
    Webhook {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    /// Send a notification message (logged, future: push notification).
    Notify {
        channel: String,
        message: String,
    },
    /// Create an ingest job (dynamic fetch + pipeline).
    CreateIngestJob {
        /// Query template (use `{entity}` for substitution).
        query_template: String,
        /// Source to query.
        source: String,
        /// Reconcile strategy: "merge", "replace", "skip".
        reconcile: Option<String>,
    },
    /// Log a message (always available, no external deps).
    Log {
        level: Option<String>,
        message: String,
    },
}

/// Safety constraints for a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    /// Minimum seconds between firings of this rule.
    #[serde(default)]
    pub cooldown_secs: u64,
    /// Maximum chain depth (prevents infinite cascades).
    #[serde(default = "default_chain_depth")]
    pub max_chain_depth: u32,
    /// Maximum effects per execution.
    #[serde(default = "default_effect_budget")]
    pub max_effects: u32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            cooldown_secs: 0,
            max_chain_depth: default_chain_depth(),
            max_effects: default_effect_budget(),
        }
    }
}

fn default_chain_depth() -> u32 {
    5
}

fn default_effect_budget() -> u32 {
    100
}

/// Result of evaluating and executing a rule.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RuleResult {
    pub rule_id: String,
    pub triggered: bool,
    pub conditions_passed: bool,
    pub effects_executed: u32,
    pub effects_skipped: u32,
    pub errors: Vec<String>,
}

/// Summary of an action engine run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ActionReport {
    pub events_processed: u32,
    pub rules_evaluated: u32,
    pub rules_fired: u32,
    pub effects_executed: u32,
    pub effects_skipped: u32,
    pub safety_violations: u32,
    pub errors: Vec<String>,
}

/// Dry run result: what would happen without actually executing.
#[derive(Debug, Clone, Serialize)]
pub struct DryRunResult {
    pub rule_id: String,
    pub would_fire: bool,
    pub conditions: Vec<ConditionResult>,
    pub effects: Vec<EffectPreview>,
}

/// Result of a single condition check in dry run.
#[derive(Debug, Clone, Serialize)]
pub struct ConditionResult {
    pub condition: String,
    pub passed: bool,
    pub detail: Option<String>,
}

/// Preview of an effect in dry run.
#[derive(Debug, Clone, Serialize)]
pub struct EffectPreview {
    pub effect_type: String,
    pub description: String,
    pub would_execute: bool,
}
