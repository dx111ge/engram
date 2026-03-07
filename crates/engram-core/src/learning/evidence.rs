/// Evidence surfacing — return statistical evidence alongside query results.
///
/// When a user queries engram, the response includes:
/// - Co-occurrence statistics (how often related events co-occur)
/// - Confidence provenance (why the confidence is what it is)
/// - Supporting and contradicting facts
///
/// This enables pattern recognition without engram making assumptions.

/// Evidence associated with a query result.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// Co-occurrence patterns related to this result.
    pub cooccurrences: Vec<CooccurrenceEvidence>,
    /// Related facts that support this result.
    pub supporting: Vec<SupportingFact>,
    /// Known contradictions with this result.
    pub contradictions: Vec<ContradictingFact>,
}

impl Evidence {
    pub fn empty() -> Self {
        Evidence {
            cooccurrences: Vec::new(),
            supporting: Vec::new(),
            contradictions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cooccurrences.is_empty()
            && self.supporting.is_empty()
            && self.contradictions.is_empty()
    }
}

/// A co-occurrence pattern surfaced as evidence.
#[derive(Debug, Clone)]
pub struct CooccurrenceEvidence {
    /// What precedes/triggers.
    pub antecedent: String,
    /// What follows.
    pub consequent: String,
    /// How many times observed.
    pub count: u32,
    /// Conditional probability P(consequent | antecedent).
    pub probability: f32,
}

/// A fact that supports the queried result.
#[derive(Debug, Clone)]
pub struct SupportingFact {
    pub slot: u64,
    pub label: String,
    pub confidence: f32,
    pub relationship: String,
}

/// A fact that contradicts the queried result.
#[derive(Debug, Clone)]
pub struct ContradictingFact {
    pub slot: u64,
    pub label: String,
    pub confidence: f32,
    pub reason: String,
}

/// An enriched search result that includes evidence.
#[derive(Debug)]
pub struct EnrichedResult {
    pub slot: u64,
    pub node_id: u64,
    pub label: String,
    pub confidence: f32,
    pub score: f64,
    pub evidence: Evidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_evidence() {
        let e = Evidence::empty();
        assert!(e.is_empty());
    }

    #[test]
    fn evidence_with_cooccurrences() {
        let e = Evidence {
            cooccurrences: vec![CooccurrenceEvidence {
                antecedent: "deploy".into(),
                consequent: "error".into(),
                count: 3,
                probability: 0.6,
            }],
            supporting: Vec::new(),
            contradictions: Vec::new(),
        };
        assert!(!e.is_empty());
    }
}
