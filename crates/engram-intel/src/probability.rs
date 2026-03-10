/// Bayesian-inspired probability calculation for intelligence assessments.
///
/// Each prediction has weighted supporting and contradicting evidence.
/// The formula accounts for both strength (confidence) and volume of evidence.

/// Calculate prediction probability from evidence chains.
///
/// `evidence_for`: Vec of confidence values for supporting evidence
/// `evidence_against`: Vec of confidence values for contradicting evidence
///
/// Returns probability in [0.05, 0.95] range.
pub fn calculate(evidence_for: &[f32], evidence_against: &[f32]) -> f32 {
    if evidence_for.is_empty() && evidence_against.is_empty() {
        return 0.50;
    }

    let weighted_for = if evidence_for.is_empty() {
        0.0
    } else {
        let total: f32 = evidence_for.iter().sum();
        evidence_for.iter().map(|c| c * c).sum::<f32>() / total
    };

    let weighted_against = if evidence_against.is_empty() {
        0.0
    } else {
        let total: f32 = evidence_against.iter().sum();
        evidence_against.iter().map(|c| c * c).sum::<f32>() / total
    };

    let n_total = evidence_for.len() + evidence_against.len();
    let discount = if n_total > 0 {
        evidence_against.len() as f32 / n_total as f32
    } else {
        0.0
    };

    let prob = weighted_for * (1.0 - weighted_against * discount);
    prob.clamp(0.05, 0.95)
}

/// Calculate the shift impact of adding new evidence to an existing prediction.
///
/// Returns (new_probability, shift_from_old).
pub fn calculate_with_shift(
    existing_for: &[f32],
    existing_against: &[f32],
    new_for: &[f32],
    new_against: &[f32],
) -> (f32, f32) {
    let old_prob = calculate(existing_for, existing_against);

    let mut all_for = existing_for.to_vec();
    all_for.extend_from_slice(new_for);

    let mut all_against = existing_against.to_vec();
    all_against.extend_from_slice(new_against);

    let new_prob = calculate(&all_for, &all_against);
    (new_prob, new_prob - old_prob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_evidence() {
        let p = calculate(&[0.80, 0.85], &[0.80, 0.85]);
        assert!(p < 0.55 && p > 0.35, "balanced should be near 0.45: {p}");
    }

    #[test]
    fn strong_support() {
        let p = calculate(&[0.90, 0.88, 0.85, 0.82, 0.80], &[0.85]);
        assert!(p > 0.55, "strong support should be > 0.55: {p}");
    }

    #[test]
    fn strong_contradiction() {
        let p = calculate(&[0.70], &[0.92, 0.88, 0.85, 0.82]);
        assert!(p < 0.40, "strong contradiction should be < 0.40: {p}");
    }

    #[test]
    fn no_evidence() {
        let p = calculate(&[], &[]);
        assert!((p - 0.50).abs() < 0.01);
    }

    #[test]
    fn shift_calculation() {
        let (new_p, shift) = calculate_with_shift(
            &[0.85, 0.80],
            &[0.90],
            &[0.78],  // new supporting
            &[],      // no new contradicting
        );
        assert!(shift > 0.0, "adding support should increase: shift={shift}");
        assert!(new_p > calculate(&[0.85, 0.80], &[0.90]));
    }
}
