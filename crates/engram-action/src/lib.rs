/// # engram-action
///
/// Event-driven action engine for engram. Subscribes to graph change events
/// and evaluates rules to trigger effects (confidence cascades, edge creation,
/// webhooks, ingest jobs, notifications).
///
/// ## Architecture
///
/// ```text
/// GraphEvent (from EventBus)
///     -> Trigger matching (which rules apply?)
///     -> Condition evaluation (are conditions met?)
///     -> Effect execution (what happens?)
///     -> Safety checks (cooldown, chain depth, budget)
/// ```
///
/// Rules are defined in TOML and loaded at runtime.

pub mod condition;
pub mod effects;
pub mod engine;
pub mod error;
pub mod rule_parser;
pub mod types;

// Re-exports for convenience.
pub use engine::ActionEngine;
pub use error::ActionError;
pub use rule_parser::{parse_rules, validate_rule};
pub use types::{
    ActionReport, ActionRule, Condition, DryRunResult, Effect, SafetyConfig, Trigger,
};
