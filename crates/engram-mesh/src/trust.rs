/// Trust model and confidence propagation across peers.
///
/// When a fact arrives from a peer, its local confidence is calculated as:
///   local_confidence = fact.confidence * peer.trust * propagation_decay
///
/// Where propagation_decay = 0.9 per hop. This means knowledge degrades
/// with distance, just like real-world trust in information.

/// Default decay factor per hop (knowledge degrades with distance).
pub const PROPAGATION_DECAY: f32 = 0.9;

/// Minimum confidence below which facts are dropped entirely.
pub const MIN_USEFUL_CONFIDENCE: f32 = 0.05;

/// Maximum number of hops before a fact is considered too remote.
pub const MAX_HOPS: u8 = 10;

/// Calculate the local confidence for a fact received from a peer.
///
/// # Arguments
/// * `source_confidence` - Confidence at the originating node
/// * `peer_trust` - Trust level of the peer delivering this fact (0.0-1.0)
/// * `hops` - Number of hops from the original source
///
/// # Returns
/// Adjusted confidence, or None if below useful threshold
pub fn propagated_confidence(source_confidence: f32, peer_trust: f32, hops: u8) -> Option<f32> {
    if hops > MAX_HOPS {
        return None;
    }
    let decay = PROPAGATION_DECAY.powi(hops as i32);
    let local = source_confidence * peer_trust * decay;
    if local < MIN_USEFUL_CONFIDENCE {
        None
    } else {
        Some(local)
    }
}

/// Trust score for a peer, tracking history of interactions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrustScore {
    /// Current trust value (0.0 to 1.0)
    pub value: f32,
    /// Number of successful syncs
    pub successful_syncs: u64,
    /// Number of facts received that were later confirmed
    pub confirmed_facts: u64,
    /// Number of facts received that were contradicted
    pub contradicted_facts: u64,
    /// Number of facts received total
    pub total_facts_received: u64,
}

impl TrustScore {
    /// Create a new trust score with an initial value.
    pub fn new(initial: f32) -> Self {
        TrustScore {
            value: initial.clamp(0.0, 1.0),
            successful_syncs: 0,
            confirmed_facts: 0,
            contradicted_facts: 0,
            total_facts_received: 0,
        }
    }

    /// Record a successful sync.
    pub fn record_sync(&mut self) {
        self.successful_syncs += 1;
    }

    /// Record that a fact from this peer was confirmed by another source.
    pub fn record_confirmation(&mut self) {
        self.confirmed_facts += 1;
        self.total_facts_received += 1;
        self.recalculate();
    }

    /// Record that a fact from this peer was contradicted.
    pub fn record_contradiction(&mut self) {
        self.contradicted_facts += 1;
        self.total_facts_received += 1;
        self.recalculate();
    }

    /// Record a received fact (neutral — not yet confirmed or contradicted).
    pub fn record_received(&mut self) {
        self.total_facts_received += 1;
    }

    /// Recalculate trust based on confirmation/contradiction ratio.
    fn recalculate(&mut self) {
        let total_evaluated = self.confirmed_facts + self.contradicted_facts;
        if total_evaluated < 5 {
            // Not enough data to adjust trust
            return;
        }
        // Trust is a weighted blend of initial trust and observed reliability
        let reliability = self.confirmed_facts as f32 / total_evaluated as f32;
        // Blend: 30% initial, 70% observed (after enough data)
        let blend_factor = (total_evaluated as f32 / 50.0).min(0.7);
        self.value = self.value * (1.0 - blend_factor) + reliability * blend_factor;
        self.value = self.value.clamp(0.0, 1.0);
    }

    /// Accuracy ratio: confirmed / (confirmed + contradicted). None if no evaluations.
    pub fn accuracy(&self) -> Option<f32> {
        let total = self.confirmed_facts + self.contradicted_facts;
        if total == 0 {
            None
        } else {
            Some(self.confirmed_facts as f32 / total as f32)
        }
    }
}

impl Default for TrustScore {
    fn default() -> Self {
        Self::new(0.5) // neutral trust by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_observation() {
        // Local fact, no peer, no hops
        let conf = propagated_confidence(0.90, 1.0, 0);
        assert_eq!(conf, Some(0.90));
    }

    #[test]
    fn trusted_peer_one_hop() {
        let conf = propagated_confidence(0.90, 0.80, 1).unwrap();
        // 0.90 * 0.80 * 0.9 = 0.648
        assert!((conf - 0.648).abs() < 0.001);
    }

    #[test]
    fn two_hops() {
        let conf = propagated_confidence(0.90, 0.80, 2).unwrap();
        // 0.90 * 0.80 * 0.81 = 0.5832
        assert!((conf - 0.5832).abs() < 0.001);
    }

    #[test]
    fn three_hops() {
        let conf = propagated_confidence(0.90, 0.80, 3).unwrap();
        // 0.90 * 0.80 * 0.729 = 0.52488
        assert!((conf - 0.52488).abs() < 0.001);
    }

    #[test]
    fn low_trust_drops_below_threshold() {
        // Very low trust peer, many hops
        let conf = propagated_confidence(0.30, 0.10, 5);
        // 0.30 * 0.10 * 0.9^5 = 0.03 * 0.59049 = 0.0177
        assert!(conf.is_none()); // below MIN_USEFUL_CONFIDENCE
    }

    #[test]
    fn max_hops_exceeded() {
        let conf = propagated_confidence(1.0, 1.0, MAX_HOPS + 1);
        assert!(conf.is_none());
    }

    #[test]
    fn trust_score_adjusts_with_confirmations() {
        let mut ts = TrustScore::new(0.5);
        // 10 confirmations, 0 contradictions
        for _ in 0..10 {
            ts.record_confirmation();
        }
        // Trust should increase toward 1.0
        assert!(ts.value > 0.5, "trust should increase: {}", ts.value);
        assert_eq!(ts.accuracy(), Some(1.0));
    }

    #[test]
    fn trust_score_decreases_with_contradictions() {
        let mut ts = TrustScore::new(0.8);
        // 2 confirmations, 8 contradictions
        for _ in 0..2 {
            ts.record_confirmation();
        }
        for _ in 0..8 {
            ts.record_contradiction();
        }
        // Trust should decrease
        assert!(ts.value < 0.8, "trust should decrease: {}", ts.value);
        assert_eq!(ts.accuracy(), Some(0.2));
    }

    #[test]
    fn trust_score_no_adjustment_with_few_samples() {
        let mut ts = TrustScore::new(0.5);
        ts.record_contradiction();
        ts.record_contradiction();
        // Only 2 evaluations — trust stays at 0.5
        assert_eq!(ts.value, 0.5);
    }
}
