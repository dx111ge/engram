/// Conflict resolution across peers.
///
/// When two peers disagree about a fact, we don't force consensus.
/// Each node decides locally based on its own trust model:
///
/// 1. Both facts stored with provenance
/// 2. Confidence comparison (including peer trust weights)
/// 3. Recency check — newer observations may override
/// 4. If unresolvable: both kept, flagged as "disputed"
/// 5. Local inference engine can apply domain rules to resolve

use crate::gossip::SyncFact;
use crate::trust;

/// Outcome of a conflict resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Accept the incoming fact (higher confidence or more recent)
    AcceptIncoming,
    /// Keep the local fact (higher confidence or more recent)
    KeepLocal,
    /// Both are valid — store both, flag as disputed
    Disputed,
    /// Merge: the incoming fact supplements the local fact
    Merge,
}

/// A conflict between a local fact and an incoming fact from a peer.
#[derive(Debug)]
pub struct Conflict {
    /// The label in question
    pub label: String,
    /// Local confidence
    pub local_confidence: f32,
    /// Local last update timestamp (unix millis)
    pub local_updated_at: u64,
    /// Local provenance
    pub local_provenance: String,
    /// Incoming fact
    pub incoming: SyncFact,
    /// Peer trust level
    pub peer_trust: f32,
    /// Hops from original source
    pub hops: u8,
}

impl Conflict {
    /// Resolve the conflict using the default strategy.
    pub fn resolve(&self) -> Resolution {
        // Calculate the effective confidence of the incoming fact
        let incoming_effective = match trust::propagated_confidence(
            self.incoming.confidence,
            self.peer_trust,
            self.hops,
        ) {
            Some(c) => c,
            None => return Resolution::KeepLocal, // too degraded
        };

        let confidence_diff = (incoming_effective - self.local_confidence).abs();

        // If confidences are very close, use recency
        if confidence_diff < 0.05 {
            if self.incoming.updated_at > self.local_updated_at {
                // More recent observation wins in a tie
                Resolution::AcceptIncoming
            } else if self.local_updated_at > self.incoming.updated_at {
                Resolution::KeepLocal
            } else {
                // Same timestamp, same confidence — disputed
                Resolution::Disputed
            }
        } else if incoming_effective > self.local_confidence {
            // Clear confidence winner
            Resolution::AcceptIncoming
        } else {
            Resolution::KeepLocal
        }
    }
}

/// Resolve a batch of conflicts, returning resolutions paired with the original conflicts.
pub fn resolve_batch(conflicts: Vec<Conflict>) -> Vec<(Conflict, Resolution)> {
    conflicts
        .into_iter()
        .map(|c| {
            let resolution = c.resolve();
            (c, resolution)
        })
        .collect()
}

/// Check if an incoming fact conflicts with existing knowledge.
/// Returns None if no conflict (fact is new), Some(Conflict) if there's a clash.
pub fn detect_conflict(
    label: &str,
    local_confidence: Option<f32>,
    local_updated_at: Option<u64>,
    local_provenance: Option<&str>,
    incoming: &SyncFact,
    peer_trust: f32,
    hops: u8,
) -> Option<Conflict> {
    // If we don't have this fact locally, no conflict
    let local_conf = local_confidence?;
    let local_updated = local_updated_at.unwrap_or(0);
    let local_prov = local_provenance.unwrap_or("unknown").to_string();

    Some(Conflict {
        label: label.to_string(),
        local_confidence: local_conf,
        local_updated_at: local_updated,
        local_provenance: local_prov,
        incoming: incoming.clone(),
        peer_trust,
        hops,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fact(label: &str, confidence: f32, updated_at: u64) -> SyncFact {
        SyncFact {
            label: label.to_string(),
            confidence,
            provenance: "peer".to_string(),
            edges: vec![],
            created_at: updated_at,
            updated_at,
            topics: vec![],
            access_level: 2, // public
            properties: vec![],
        }
    }

    #[test]
    fn higher_confidence_incoming_wins() {
        let conflict = Conflict {
            label: "X".to_string(),
            local_confidence: 0.5,
            local_updated_at: 1000,
            local_provenance: "local".to_string(),
            incoming: make_fact("X", 0.95, 2000),
            peer_trust: 0.9,
            hops: 0,
        };
        // incoming effective: 0.95 * 0.9 * 1.0 = 0.855 > 0.5
        assert_eq!(conflict.resolve(), Resolution::AcceptIncoming);
    }

    #[test]
    fn higher_confidence_local_wins() {
        let conflict = Conflict {
            label: "Y".to_string(),
            local_confidence: 0.9,
            local_updated_at: 1000,
            local_provenance: "local".to_string(),
            incoming: make_fact("Y", 0.6, 2000),
            peer_trust: 0.8,
            hops: 1,
        };
        // incoming effective: 0.6 * 0.8 * 0.9 = 0.432 < 0.9
        assert_eq!(conflict.resolve(), Resolution::KeepLocal);
    }

    #[test]
    fn close_confidence_newer_wins() {
        let conflict = Conflict {
            label: "Z".to_string(),
            local_confidence: 0.70,
            local_updated_at: 1000,
            local_provenance: "local".to_string(),
            incoming: make_fact("Z", 0.78, 2000), // 0.78 * 1.0 * 0.9 = 0.702 ≈ 0.70
            peer_trust: 1.0,
            hops: 1,
        };
        // Close confidence, incoming is newer
        assert_eq!(conflict.resolve(), Resolution::AcceptIncoming);
    }

    #[test]
    fn close_confidence_older_keeps_local() {
        let conflict = Conflict {
            label: "Z".to_string(),
            local_confidence: 0.70,
            local_updated_at: 3000, // local is newer
            local_provenance: "local".to_string(),
            incoming: make_fact("Z", 0.78, 1000),
            peer_trust: 1.0,
            hops: 1,
        };
        assert_eq!(conflict.resolve(), Resolution::KeepLocal);
    }

    #[test]
    fn same_everything_is_disputed() {
        let conflict = Conflict {
            label: "W".to_string(),
            local_confidence: 0.70,
            local_updated_at: 1000,
            local_provenance: "local".to_string(),
            incoming: make_fact("W", 0.778, 1000), // 0.778 * 1.0 * 0.9 = 0.7002 ≈ 0.70
            peer_trust: 1.0,
            hops: 1,
        };
        assert_eq!(conflict.resolve(), Resolution::Disputed);
    }

    #[test]
    fn degraded_incoming_dropped() {
        let conflict = Conflict {
            label: "V".to_string(),
            local_confidence: 0.5,
            local_updated_at: 1000,
            local_provenance: "local".to_string(),
            incoming: make_fact("V", 0.1, 2000),
            peer_trust: 0.1,
            hops: 5,
        };
        // 0.1 * 0.1 * 0.9^5 = 0.00059 — below threshold
        assert_eq!(conflict.resolve(), Resolution::KeepLocal);
    }

    #[test]
    fn no_conflict_for_new_fact() {
        let fact = make_fact("NEW", 0.8, 1000);
        let conflict = detect_conflict("NEW", None, None, None, &fact, 0.9, 0);
        assert!(conflict.is_none());
    }

    #[test]
    fn detects_conflict_for_existing_fact() {
        let fact = make_fact("OLD", 0.8, 2000);
        let conflict = detect_conflict("OLD", Some(0.6), Some(1000), Some("local"), &fact, 0.9, 0);
        assert!(conflict.is_some());
    }

    #[test]
    fn batch_resolution() {
        let conflicts = vec![
            Conflict {
                label: "A".to_string(),
                local_confidence: 0.3,
                local_updated_at: 1000,
                local_provenance: "local".to_string(),
                incoming: make_fact("A", 0.9, 2000),
                peer_trust: 1.0,
                hops: 0,
            },
            Conflict {
                label: "B".to_string(),
                local_confidence: 0.95,
                local_updated_at: 1000,
                local_provenance: "local".to_string(),
                incoming: make_fact("B", 0.3, 2000),
                peer_trust: 0.5,
                hops: 2,
            },
        ];
        let results = resolve_batch(conflicts);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, Resolution::AcceptIncoming);
        assert_eq!(results[1].1, Resolution::KeepLocal);
    }
}
