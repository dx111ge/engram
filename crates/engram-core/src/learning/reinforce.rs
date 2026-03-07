/// Reinforcement — boost confidence when knowledge is accessed or confirmed.
///
/// Two mechanisms:
/// - **Access reinforcement**: small boost each time a fact is read/used (+0.02)
/// - **Confirmation reinforcement**: larger boost when new evidence confirms a fact (+0.10)
///
/// Both are capped at the source-type maximum confidence.

/// Confidence boost per access (use-it-or-lose-it signal).
pub const ACCESS_BOOST: f32 = 0.02;

/// Confidence boost when explicitly confirmed by new evidence.
pub const CONFIRMATION_BOOST: f32 = 0.10;

/// Apply access reinforcement to a confidence value.
/// Returns the new confidence, capped at `max`.
pub fn reinforce_access(current: f32, max: f32) -> f32 {
    (current + ACCESS_BOOST).min(max)
}

/// Apply confirmation reinforcement to a confidence value.
/// Returns the new confidence, capped at `max`.
pub fn reinforce_confirm(current: f32, max: f32) -> f32 {
    (current + CONFIRMATION_BOOST).min(max)
}

/// Apply contradiction penalty to a confidence value.
pub const CONTRADICTION_PENALTY: f32 = 0.20;

pub fn penalize_contradiction(current: f32) -> f32 {
    (current - CONTRADICTION_PENALTY).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_boosts_confidence() {
        let c = reinforce_access(0.70, 0.95);
        assert!((c - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn access_respects_cap() {
        let c = reinforce_access(0.94, 0.95);
        assert!((c - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn confirmation_boosts_more() {
        let c = reinforce_confirm(0.70, 0.95);
        assert!((c - 0.80).abs() < f32::EPSILON);
    }

    #[test]
    fn confirmation_respects_cap() {
        let c = reinforce_confirm(0.90, 0.95);
        assert!((c - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn contradiction_reduces() {
        let c = penalize_contradiction(0.80);
        assert!((c - 0.60).abs() < f32::EPSILON);
    }

    #[test]
    fn contradiction_floors_at_zero() {
        let c = penalize_contradiction(0.10);
        assert!((c - 0.0).abs() < f32::EPSILON);
    }
}
