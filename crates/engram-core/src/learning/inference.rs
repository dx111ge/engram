/// Inference engine data structures — forward and backward chaining.
///
/// Forward chaining: scan graph for matching rule patterns, fire actions.
/// Backward chaining: given a goal, search backward for supporting evidence.
///
/// Execution happens in Graph (graph.rs) since it needs access to the full graph.
/// This module defines the result types.

/// A binding maps variable names to node labels during rule matching.
pub type Bindings = std::collections::HashMap<String, String>;

/// Result of forward chaining across the entire graph.
#[derive(Debug, Default)]
pub struct InferenceResult {
    /// Number of rules evaluated.
    pub rules_evaluated: u32,
    /// Number of times a rule fired.
    pub rules_fired: u32,
    /// New edges created by rule actions.
    pub edges_created: u32,
    /// Flags raised by rule actions.
    pub flags_raised: u32,
    /// Details of each firing.
    pub firings: Vec<RuleFiring>,
}

/// Record of a single rule firing.
#[derive(Debug, Clone)]
pub struct RuleFiring {
    pub rule_name: String,
    pub bindings: Bindings,
    pub actions_taken: Vec<String>,
}

/// A proof step in backward chaining.
#[derive(Debug, Clone)]
pub struct ProofStep {
    /// The fact being proved.
    pub fact: String,
    /// Confidence of this fact.
    pub confidence: f32,
    /// Supporting evidence (what proves this).
    pub evidence: Vec<String>,
    /// How deep in the proof chain.
    pub depth: u32,
}

/// Result of backward chaining (trying to prove a goal).
#[derive(Debug)]
pub struct ProofResult {
    /// Whether the goal is supported.
    pub supported: bool,
    /// Aggregate confidence of the proof chain.
    pub confidence: f32,
    /// Steps in the proof chain.
    pub chain: Vec<ProofStep>,
}

impl ProofResult {
    pub fn unsupported() -> Self {
        ProofResult {
            supported: false,
            confidence: 0.0,
            chain: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_result_default() {
        let r = InferenceResult::default();
        assert_eq!(r.rules_evaluated, 0);
        assert_eq!(r.rules_fired, 0);
    }

    #[test]
    fn proof_result_unsupported() {
        let p = ProofResult::unsupported();
        assert!(!p.supported);
        assert_eq!(p.confidence, 0.0);
    }
}
