/// Delta sync engine — push/pull knowledge between peers.
///
/// The sync engine coordinates the full sync lifecycle:
/// 1. Build a bloom filter digest of local knowledge
/// 2. Exchange heartbeats with peers
/// 3. Compare digests to find knowledge gaps
/// 4. Request missing knowledge via SyncRequest
/// 5. Apply received facts with conflict resolution
/// 6. Track what was synced for the audit trail

use crate::bloom::BloomFilter;
use crate::conflict::{self, Resolution};
use crate::gossip::{Heartbeat, SyncFact, SyncRequest, SyncResponse};
use crate::identity::PublicKey;
use crate::peer::{AccessLevel, PeerConfig, SyncPolicy};
use crate::trust;

/// Result of a sync operation.
#[derive(Debug)]
pub struct SyncResult {
    /// Peer we synced with
    pub peer_key: PublicKey,
    /// Facts accepted (new or updated)
    pub accepted: u32,
    /// Facts rejected (lower confidence, policy filter)
    pub rejected: u32,
    /// Facts disputed (stored with conflict flag)
    pub disputed: u32,
    /// Facts skipped (already known)
    pub skipped: u32,
    /// Whether there are more facts available
    pub has_more: bool,
}

/// Determines which local facts to include in a sync response.
pub fn filter_facts_for_peer(
    facts: &[SyncFact],
    policy: &SyncPolicy,
) -> Vec<SyncFact> {
    facts
        .iter()
        .filter(|f| {
            // Check confidence threshold
            if f.confidence < policy.min_confidence {
                return false;
            }
            // Check access level
            let access = match f.access_level {
                0 => AccessLevel::Private,
                1 => AccessLevel::Team,
                2 => AccessLevel::Public,
                3 => AccessLevel::Redacted,
                _ => AccessLevel::Private,
            };
            if !access.is_shareable() {
                return false;
            }
            // Check exclude labels
            if policy.exclude_labels.iter().any(|e| f.label.eq_ignore_ascii_case(e)) {
                return false;
            }
            // Check topic filter (empty = all topics)
            if !policy.topics.is_empty() {
                if !f.topics.iter().any(|t| policy.topics.contains(t)) {
                    return false;
                }
            }
            true
        })
        .take(policy.max_batch_size as usize)
        .cloned()
        .collect()
}

/// Process incoming facts from a peer, applying trust and conflict resolution.
///
/// Returns a SyncResult summarizing what happened, plus the list of facts
/// to be stored locally (caller handles actual storage).
pub fn process_incoming(
    response: &SyncResponse,
    peer: &PeerConfig,
    local_facts: &dyn Fn(&str) -> Option<(f32, u64, String)>,
) -> (SyncResult, Vec<(SyncFact, f32)>) {
    let hops = response.peer_chain.len() as u8;
    let mut accepted = Vec::new();
    let mut result = SyncResult {
        peer_key: peer.public_key.clone(),
        accepted: 0,
        rejected: 0,
        disputed: 0,
        skipped: 0,
        has_more: response.has_more,
    };

    for fact in &response.facts {
        // Calculate propagated confidence
        let local_conf = match trust::propagated_confidence(fact.confidence, peer.trust, hops) {
            Some(c) => c,
            None => {
                result.rejected += 1;
                continue;
            }
        };

        // Check accept policy
        if local_conf < peer.accept_policy.min_confidence {
            result.rejected += 1;
            continue;
        }

        // Check if we already have this fact
        if let Some((existing_conf, existing_updated, existing_prov)) = local_facts(&fact.label) {
            let conflict = conflict::Conflict {
                label: fact.label.clone(),
                local_confidence: existing_conf,
                local_updated_at: existing_updated,
                local_provenance: existing_prov,
                incoming: fact.clone(),
                peer_trust: peer.trust,
                hops,
            };
            match conflict.resolve() {
                Resolution::AcceptIncoming => {
                    accepted.push((fact.clone(), local_conf));
                    result.accepted += 1;
                }
                Resolution::KeepLocal => {
                    result.skipped += 1;
                }
                Resolution::Disputed => {
                    // Store with conflict flag — caller handles this
                    accepted.push((fact.clone(), local_conf));
                    result.disputed += 1;
                }
                Resolution::Merge => {
                    accepted.push((fact.clone(), local_conf));
                    result.accepted += 1;
                }
            }
        } else {
            // New fact — accept it
            accepted.push((fact.clone(), local_conf));
            result.accepted += 1;
        }
    }

    (result, accepted)
}

/// Build a knowledge digest from a list of labels.
pub fn build_digest(labels: &[String]) -> BloomFilter {
    let mut bf = BloomFilter::new(labels.len().max(64) as u32, 0.01);
    for label in labels {
        bf.insert_str(label);
    }
    bf
}

/// Compare two heartbeats to determine if a sync is needed.
/// Returns topics that the other peer might have knowledge about that we lack.
pub fn needs_sync(
    local_heartbeat: &Heartbeat,
    remote_heartbeat: &Heartbeat,
    interesting_labels: &[String],
) -> Vec<String> {
    let mut missing = Vec::new();
    for label in interesting_labels {
        if remote_heartbeat.knowledge_digest.might_contain_str(label)
            && !local_heartbeat.knowledge_digest.might_contain_str(label)
        {
            missing.push(label.clone());
        }
    }
    missing
}

/// Build a sync request from a heartbeat comparison.
pub fn build_sync_request(
    my_key: &PublicKey,
    topics: Vec<String>,
    since: u64,
    policy: &SyncPolicy,
) -> SyncRequest {
    SyncRequest {
        sender: my_key.clone(),
        topics,
        since,
        max_facts: policy.max_batch_size,
        min_confidence: policy.min_confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::SyncFact;
    use crate::identity::Keypair;
    use crate::peer::SyncPolicy;

    fn make_fact(label: &str, confidence: f32, access: u8) -> SyncFact {
        SyncFact {
            label: label.to_string(),
            confidence,
            provenance: "test".to_string(),
            edges: vec![],
            created_at: 1000,
            updated_at: 1000,
            topics: vec!["general".to_string()],
            access_level: access,
            properties: vec![],
        }
    }

    #[test]
    fn filter_respects_access_level() {
        let facts = vec![
            make_fact("private", 0.9, 0),  // Private
            make_fact("team", 0.9, 1),     // Team
            make_fact("public", 0.9, 2),   // Public
        ];
        let policy = SyncPolicy::default();
        let filtered = filter_facts_for_peer(&facts, &policy);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|f| f.label != "private"));
    }

    #[test]
    fn filter_respects_confidence() {
        let facts = vec![
            make_fact("low", 0.1, 2),
            make_fact("high", 0.8, 2),
        ];
        let mut policy = SyncPolicy::default();
        policy.min_confidence = 0.5;
        let filtered = filter_facts_for_peer(&facts, &policy);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].label, "high");
    }

    #[test]
    fn filter_respects_exclude_labels() {
        let facts = vec![
            make_fact("secret", 0.9, 2),
            make_fact("ok", 0.9, 2),
        ];
        let mut policy = SyncPolicy::default();
        policy.exclude_labels = vec!["secret".to_string()];
        let filtered = filter_facts_for_peer(&facts, &policy);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].label, "ok");
    }

    #[test]
    fn filter_respects_topic_filter() {
        let mut f1 = make_fact("a", 0.9, 2);
        f1.topics = vec!["security".to_string()];
        let mut f2 = make_fact("b", 0.9, 2);
        f2.topics = vec!["networking".to_string()];
        let mut policy = SyncPolicy::default();
        policy.topics = vec!["security".to_string()];
        let filtered = filter_facts_for_peer(&[f1, f2], &policy);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].label, "a");
    }

    #[test]
    fn process_incoming_new_facts() {
        let kp = Keypair::generate();
        let peer = PeerConfig {
            public_key: kp.public.clone(),
            name: "test".to_string(),
            endpoint: "localhost:7432".to_string(),
            trust: 0.9,
            approved: true,
            subscribed_topics: vec![],
            share_policy: SyncPolicy::default(),
            accept_policy: SyncPolicy::default(),
            last_sync: 0,
            online: true,
        };
        let response = SyncResponse {
            sender: kp.public.clone(),
            facts: vec![make_fact("new1", 0.8, 2), make_fact("new2", 0.7, 2)],
            has_more: false,
            peer_chain: vec![kp.public.clone()],
        };
        let (result, accepted) = process_incoming(&response, &peer, &|_| None);
        assert_eq!(result.accepted, 2);
        assert_eq!(accepted.len(), 2);
    }

    #[test]
    fn process_incoming_rejects_low_confidence() {
        let kp = Keypair::generate();
        let peer = PeerConfig {
            public_key: kp.public.clone(),
            name: "test".to_string(),
            endpoint: "localhost:7432".to_string(),
            trust: 0.1, // very low trust
            approved: true,
            subscribed_topics: vec![],
            share_policy: SyncPolicy::default(),
            accept_policy: SyncPolicy::default(),
            last_sync: 0,
            online: true,
        };
        let response = SyncResponse {
            sender: kp.public.clone(),
            facts: vec![make_fact("weak", 0.1, 2)],
            has_more: false,
            peer_chain: vec![kp.public.clone(), Keypair::generate().public], // 2 hops
        };
        let (result, accepted) = process_incoming(&response, &peer, &|_| None);
        assert_eq!(result.rejected, 1);
        assert!(accepted.is_empty());
    }

    #[test]
    fn needs_sync_detects_gaps() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let local = crate::gossip::build_heartbeat(
            &kp1.public,
            &["alpha".to_string()],
            &[],
            1,
            1000,
        );
        let remote = crate::gossip::build_heartbeat(
            &kp2.public,
            &["alpha".to_string(), "beta".to_string()],
            &[],
            2,
            2000,
        );
        let missing = needs_sync(&local, &remote, &["beta".to_string()]);
        assert!(missing.contains(&"beta".to_string()));
    }
}
