/// Gossip protocol for knowledge mesh.
///
/// Peers exchange heartbeats with bloom filter knowledge digests to discover
/// what knowledge each peer has. When a peer detects another has knowledge
/// it lacks (via bloom filter comparison), it initiates a delta sync.
///
/// Messages follow the wire protocol defined in DESIGN.md:
/// - Heartbeat: periodic announcement of what I know
/// - SyncRequest: ask for specific knowledge delta
/// - SyncResponse: deliver facts matching a request
/// - QueryBroadcast: ask the mesh a question (with TTL)
/// - QueryResponse: answer to a broadcast query

use crate::bloom::BloomFilter;
use crate::identity::PublicKey;

/// Heartbeat message sent periodically to all peers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Heartbeat {
    /// Sender's public key
    pub sender: PublicKey,
    /// Compact summary of sender's knowledge
    pub knowledge_digest: BloomFilter,
    /// Topics the sender has knowledge about
    pub topic_subscriptions: Vec<String>,
    /// Total number of facts at the sender
    pub fact_count: u64,
    /// Timestamp of last fact update (unix millis)
    pub last_updated: u64,
    /// Protocol version
    pub protocol_version: u8,
}

/// Request for a delta sync from a peer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncRequest {
    /// Requester's public key
    pub sender: PublicKey,
    /// Topics to sync
    pub topics: Vec<String>,
    /// Only facts updated since this timestamp (unix millis)
    pub since: u64,
    /// Maximum number of facts to return
    pub max_facts: u32,
    /// Minimum confidence threshold
    pub min_confidence: f32,
}

/// A fact transferred between peers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncFact {
    /// Node label
    pub label: String,
    /// Confidence at the source
    pub confidence: f32,
    /// Who created this fact originally
    pub provenance: String,
    /// Relationships (from, rel_type, to, confidence)
    pub edges: Vec<SyncEdge>,
    /// When this fact was created (unix millis)
    pub created_at: u64,
    /// When this fact was last updated (unix millis)
    pub updated_at: u64,
    /// Topic tags
    pub topics: Vec<String>,
    /// Access level
    pub access_level: u8,
    /// Properties as key-value pairs
    pub properties: Vec<(String, String)>,
}

/// An edge transferred between peers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncEdge {
    /// Source node label
    pub from: String,
    /// Target node label
    pub to: String,
    /// Relationship type
    pub relationship: String,
    /// Confidence
    pub confidence: f32,
}

/// Response to a sync request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncResponse {
    /// Responder's public key
    pub sender: PublicKey,
    /// Facts matching the request
    pub facts: Vec<SyncFact>,
    /// Whether more facts are available (pagination)
    pub has_more: bool,
    /// Chain of peers this data passed through (loop prevention)
    pub peer_chain: Vec<PublicKey>,
}

/// Query broadcast — ask the mesh a question.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryBroadcast {
    /// The query string
    pub query: String,
    /// Time-to-live: decrements per hop, dropped at 0
    pub ttl: u8,
    /// Original requester
    pub origin: PublicKey,
    /// Unique request ID to prevent duplicate processing
    pub request_id: String,
    /// Chain of peers this query passed through
    pub peer_chain: Vec<PublicKey>,
}

/// Response to a query broadcast.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryResponse {
    /// The request this responds to
    pub request_id: String,
    /// Matching facts
    pub results: Vec<SyncFact>,
    /// Who is responding
    pub source_peer: PublicKey,
    /// How many hops from origin
    pub hops: u8,
}

/// All message types in the gossip protocol.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Message {
    Heartbeat(Heartbeat),
    SyncRequest(SyncRequest),
    SyncResponse(SyncResponse),
    QueryBroadcast(QueryBroadcast),
    QueryResponse(QueryResponse),
}

impl Message {
    /// Serialize to JSON bytes for wire transmission.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}

/// Deduplication tracker for query broadcasts.
/// Prevents processing the same query multiple times.
pub struct QueryDedup {
    /// Recently seen request IDs with their expiry time
    seen: std::collections::HashMap<String, u64>,
    /// Maximum age before cleanup (millis)
    max_age_ms: u64,
}

impl QueryDedup {
    pub fn new(max_age_ms: u64) -> Self {
        QueryDedup {
            seen: std::collections::HashMap::new(),
            max_age_ms,
        }
    }

    /// Check if we've already seen this request. Returns true if new (not seen before).
    pub fn check_and_mark(&mut self, request_id: &str, now_ms: u64) -> bool {
        // Cleanup old entries
        self.seen.retain(|_, &mut expiry| expiry > now_ms);

        if self.seen.contains_key(request_id) {
            false
        } else {
            self.seen.insert(request_id.to_string(), now_ms + self.max_age_ms);
            true
        }
    }

    /// Number of tracked request IDs.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Check if a query broadcast should be forwarded.
/// Returns false if loop detected or TTL expired.
pub fn should_forward(query: &QueryBroadcast, my_key: &PublicKey) -> bool {
    if query.ttl == 0 {
        return false;
    }
    // Loop detection: check if my key is in the peer chain
    if query.peer_chain.contains(my_key) {
        return false;
    }
    true
}

/// Prepare a query for forwarding by decrementing TTL and adding self to chain.
pub fn prepare_forward(query: &QueryBroadcast, my_key: &PublicKey) -> QueryBroadcast {
    let mut forwarded = query.clone();
    forwarded.ttl = forwarded.ttl.saturating_sub(1);
    forwarded.peer_chain.push(my_key.clone());
    forwarded
}

/// Build a heartbeat from current graph state.
pub fn build_heartbeat(
    my_key: &PublicKey,
    labels: &[String],
    topics: &[String],
    fact_count: u64,
    last_updated: u64,
) -> Heartbeat {
    let mut digest = BloomFilter::new(labels.len().max(64) as u32, 0.01);
    for label in labels {
        digest.insert_str(label);
    }
    Heartbeat {
        sender: my_key.clone(),
        knowledge_digest: digest,
        topic_subscriptions: topics.to_vec(),
        fact_count,
        last_updated,
        protocol_version: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    #[test]
    fn heartbeat_serialization() {
        let kp = Keypair::generate();
        let hb = build_heartbeat(
            &kp.public,
            &["fact1".to_string(), "fact2".to_string()],
            &["security".to_string()],
            100,
            1234567890,
        );
        let msg = Message::Heartbeat(hb);
        let bytes = msg.to_bytes();
        let recovered = Message::from_bytes(&bytes).unwrap();
        if let Message::Heartbeat(hb2) = recovered {
            assert_eq!(hb2.fact_count, 100);
            assert!(hb2.knowledge_digest.might_contain_str("fact1"));
        } else {
            panic!("wrong message type");
        }
    }

    #[test]
    fn query_dedup() {
        let mut dedup = QueryDedup::new(5000);
        assert!(dedup.check_and_mark("req-1", 1000));
        assert!(!dedup.check_and_mark("req-1", 2000)); // duplicate
        assert!(dedup.check_and_mark("req-2", 2000));
        // After expiry
        assert!(dedup.check_and_mark("req-1", 7000)); // expired, treated as new
    }

    #[test]
    fn loop_prevention() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let query = QueryBroadcast {
            query: "test".to_string(),
            ttl: 3,
            origin: kp1.public.clone(),
            request_id: "q1".to_string(),
            peer_chain: vec![kp1.public.clone()],
        };
        // kp2 hasn't seen it, should forward
        assert!(should_forward(&query, &kp2.public));
        // kp1 is in the chain, should NOT forward
        assert!(!should_forward(&query, &kp1.public));
    }

    #[test]
    fn ttl_expiry() {
        let kp = Keypair::generate();
        let query = QueryBroadcast {
            query: "test".to_string(),
            ttl: 0,
            origin: kp.public.clone(),
            request_id: "q2".to_string(),
            peer_chain: vec![],
        };
        assert!(!should_forward(&query, &kp.public));
    }

    #[test]
    fn forward_decrements_ttl() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let query = QueryBroadcast {
            query: "test".to_string(),
            ttl: 3,
            origin: kp1.public.clone(),
            request_id: "q3".to_string(),
            peer_chain: vec![kp1.public.clone()],
        };
        let forwarded = prepare_forward(&query, &kp2.public);
        assert_eq!(forwarded.ttl, 2);
        assert_eq!(forwarded.peer_chain.len(), 2);
        assert!(forwarded.peer_chain.contains(&kp2.public));
    }

    #[test]
    fn sync_request_serialization() {
        let kp = Keypair::generate();
        let req = SyncRequest {
            sender: kp.public.clone(),
            topics: vec!["networking".to_string()],
            since: 0,
            max_facts: 100,
            min_confidence: 0.5,
        };
        let msg = Message::SyncRequest(req);
        let bytes = msg.to_bytes();
        let recovered = Message::from_bytes(&bytes).unwrap();
        if let Message::SyncRequest(r) = recovered {
            assert_eq!(r.topics, vec!["networking"]);
            assert_eq!(r.max_facts, 100);
        } else {
            panic!("wrong message type");
        }
    }
}
