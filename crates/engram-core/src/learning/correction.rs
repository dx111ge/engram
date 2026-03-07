/// Correction handling — "this is wrong" with distrust propagation.
///
/// When a fact is explicitly corrected:
/// 1. The corrected node's confidence drops to 0.0
/// 2. Edges from the corrected node get a confidence penalty
/// 3. Nodes derived from the corrected node get a propagated penalty
///
/// Propagation uses a damping factor so distrust weakens with distance.

/// Damping factor for distrust propagation per hop.
/// Each hop reduces the penalty by this multiplier.
pub const PROPAGATION_DAMPING: f32 = 0.5;

/// Result of a correction operation.
#[derive(Debug)]
pub struct CorrectionResult {
    /// The slot that was directly corrected.
    pub corrected_slot: u64,
    /// Slots that received propagated distrust (slot, old_confidence, new_confidence).
    pub propagated: Vec<(u64, f32, f32)>,
}

/// Calculate propagated confidence reduction.
/// `distance` is the number of hops from the corrected node (1 = direct neighbor).
pub fn propagated_penalty(base_penalty: f32, distance: u32) -> f32 {
    base_penalty * PROPAGATION_DAMPING.powi(distance as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagation_weakens_with_distance() {
        let p1 = propagated_penalty(0.20, 1);
        let p2 = propagated_penalty(0.20, 2);
        let p3 = propagated_penalty(0.20, 3);

        assert!(p1 > p2);
        assert!(p2 > p3);
        // hop 1: 0.20 * 0.5 = 0.10
        assert!((p1 - 0.10).abs() < f32::EPSILON);
        // hop 2: 0.20 * 0.25 = 0.05
        assert!((p2 - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_penalty_propagation() {
        let p = propagated_penalty(0.0, 1);
        assert_eq!(p, 0.0);
    }
}
