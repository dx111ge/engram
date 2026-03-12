/// Contradiction flagging — detect conflicting facts and surface both sides.
///
/// Engram does NOT pick a winner. Both facts are flagged as "disputed" and
/// the human or LLM decides which is correct.
///
/// Contradiction detection strategies:
/// 1. **Property conflict**: same node, same property key, different values
/// 2. **Mutual exclusion**: edges marked as mutually exclusive (e.g. "is_a cat" vs "is_a dog")
/// 3. **Explicit negation**: "X is true" vs "X is false" via negation edges
///
/// Phase 3 implements property-based contradiction detection. NLI-based
/// contradiction (using an ONNX model) is deferred to Phase 5.

/// A detected contradiction between two facts.
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// Slot of the existing (older) fact.
    pub existing_slot: u64,
    /// Slot of the new (conflicting) fact.
    pub new_slot: u64,
    /// Human-readable description of the conflict.
    pub reason: String,
    /// The type of contradiction detected.
    pub kind: ConflictKind,
}

/// Types of contradictions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictKind {
    /// Same property key, different values on the same or related node.
    PropertyConflict,
    /// Two edges from the same source claim mutually exclusive relationships.
    MutualExclusion,
    /// Explicit negation (e.g. correction that contradicts existing fact).
    Negation,
}

/// Result of contradiction checking.
#[derive(Debug)]
pub struct ConflictCheckResult {
    /// All detected contradictions.
    pub contradictions: Vec<Contradiction>,
}

impl ConflictCheckResult {
    pub fn none() -> Self {
        ConflictCheckResult {
            contradictions: Vec::new(),
        }
    }

    pub fn has_conflicts(&self) -> bool {
        !self.contradictions.is_empty()
    }

    pub fn with(contradictions: Vec<Contradiction>) -> Self {
        ConflictCheckResult {
            contradictions,
        }
    }
}

/// Check if two property values conflict.
/// Simple string inequality — more sophisticated semantic comparison is Phase 5.
pub fn values_conflict(existing: &str, new: &str) -> bool {
    existing != new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_check_result_empty() {
        let r = ConflictCheckResult::none();
        assert!(!r.has_conflicts());
        assert!(r.contradictions.is_empty());
    }

    #[test]
    fn conflict_check_result_with_conflicts() {
        let c = Contradiction {
            existing_slot: 0,
            new_slot: 1,
            reason: "test conflict".into(),
            kind: ConflictKind::PropertyConflict,
        };
        let r = ConflictCheckResult::with(vec![c]);
        assert!(r.has_conflicts());
        assert_eq!(r.contradictions.len(), 1);
    }

    #[test]
    fn values_conflict_detection() {
        assert!(values_conflict("10.0.0.1", "10.0.0.2"));
        assert!(!values_conflict("same", "same"));
    }
}
