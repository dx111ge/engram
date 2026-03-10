# Engram Knowledge Mesh

Peer-to-peer federation for engram instances. No master node, no central server -- every instance is a peer. Knowledge propagates selectively based on trust and topic relevance.

## Architecture

```
+---------------+         +---------------+         +---------------+
|  Engram A     |<------->|  Engram B     |<------->|  Engram C     |
|  Personal     |         |  Team         |         |  Org-wide     |
|  laptop       |         |  server       |         |  datacenter   |
|  .brain       |         |  .brain       |         |  .brain       |
+---------------+         +---------------+         +---------------+
       ^                                                   ^
       |                                                   |
       +---------------------------------------------------+
                    All peers equal
                    Gossip protocol
                    Selective sync
```

## Concepts

### Identity

Each engram instance has an ed25519 keypair generated on first start. The public key is the node's unique identifier in the mesh.

- **Keypair** is stored as a 64-byte `.identity` file alongside the `.brain` file
- **Public key** (32 bytes, hex-encoded) serves as the peer ID
- **Short ID** is the first 8 hex characters (e.g., `a3f8c21b`)
- Keys are generated from OS randomness (`BCryptGenRandom` on Windows, `/dev/urandom` on Linux/macOS)

```rust
use engram_mesh::identity::Keypair;

// Generate or load identity
let kp = Keypair::load_or_generate(Path::new("my.identity"))?;
println!("Node ID: {}", kp.public.to_hex());
println!("Short:   {}", kp.public.short_id());
```

### Peers

Peers are added by **mutual approval** -- both sides must register each other's public key and endpoint before communication begins. No automatic discovery.

```rust
use engram_mesh::peer::{PeerConfig, PeerRegistry, SyncPolicy};

let mut registry = PeerRegistry::new();

// Register a peer
let peer = PeerConfig {
    public_key: other_node_public_key,
    name: "team-server".to_string(),
    endpoint: "192.168.1.50:7432".to_string(),
    trust: 0.8,
    approved: false,  // requires explicit approval
    subscribed_topics: vec!["security".to_string(), "networking".to_string()],
    share_policy: SyncPolicy::default(),
    accept_policy: SyncPolicy::default(),
    last_sync: 0,
    online: false,
};
registry.register(peer);

// Approve the peer (enables communication)
registry.approve(&other_node_public_key);

// Adjust trust later based on experience
registry.set_trust(&other_node_public_key, 0.9);

// Persist registry
registry.save(Path::new("peers.json"))?;
```

### Access Levels

Every fact has an access level controlling how it propagates through the mesh:

| Level      | Value | Behavior |
|------------|-------|----------|
| **Private**  | 0     | Never leaves this node (personal notes, credentials) |
| **Team**     | 1     | Shared within a defined peer group |
| **Public**   | 2     | Propagated to all peers |
| **Redacted** | 3     | Structure shared, values hidden ("I know about X but can't share details") |

Access level is encoded in bits 30-31 of the node flags field:

```rust
use engram_mesh::peer::AccessLevel;

let flags = AccessLevel::Public.to_flags(existing_flags);
let level = AccessLevel::from_flags(flags);
assert!(level.is_shareable()); // true for Team, Public, Redacted
```

## Sync Model

Not full replication -- selective knowledge propagation based on relevance and trust.

### Push (Broadcast)

Node learns something new and pushes to interested peers, filtered by their topic subscriptions.

### Pull (Query)

Node needs knowledge it doesn't have and asks peers. Peers respond with matching subgraphs.

### Gossip (Protocol)

Periodic heartbeat with a bloom filter knowledge digest. Peers compare digests to discover knowledge gaps, then request deltas.

## Sync Policies

Each peer relationship has independent share and accept policies:

```rust
use engram_mesh::peer::SyncPolicy;

let policy = SyncPolicy {
    topics: vec!["security".to_string()],  // empty = all topics
    min_confidence: 0.3,                    // skip low-confidence facts
    exclude_labels: vec!["password".to_string()], // never share these
    max_depth: 3,                           // subgraph depth limit
    interval_secs: 60,                      // sync every minute
    max_batch_size: 1000,                   // max facts per sync
};
```

## Gossip Protocol

### Messages

The wire protocol uses JSON-serialized messages:

**Heartbeat** -- periodic announcement of what a node knows:
```json
{
  "Heartbeat": {
    "sender": { "0": [/* 32 bytes */] },
    "knowledge_digest": { /* bloom filter */ },
    "topic_subscriptions": ["security", "networking"],
    "fact_count": 50000,
    "last_updated": 1741340400000,
    "protocol_version": 1
  }
}
```

**SyncRequest** -- ask for a knowledge delta:
```json
{
  "SyncRequest": {
    "sender": { "0": [/* 32 bytes */] },
    "topics": ["security"],
    "since": 1741300000000,
    "max_facts": 100,
    "min_confidence": 0.5
  }
}
```

**QueryBroadcast** -- ask the mesh a question:
```json
{
  "QueryBroadcast": {
    "query": "What is CVE-2026-1234?",
    "ttl": 3,
    "origin": { "0": [/* 32 bytes */] },
    "request_id": "q-abc123",
    "peer_chain": []
  }
}
```

### Loop Prevention

- Each message carries a `peer_chain` (list of peers it passed through)
- If the local node's key is already in the chain, the message is dropped
- TTL decrements per hop, dropped at 0
- Query deduplication tracker prevents reprocessing the same `request_id`

```rust
use engram_mesh::gossip::{should_forward, prepare_forward};

if should_forward(&query, &my_key) {
    let forwarded = prepare_forward(&query, &my_key);
    // send forwarded to other peers
}
```

### Bloom Filter

Compact probabilistic data structure for knowledge digests. Allows peers to quickly check "does this node probably know about X?" without transferring the actual data.

```rust
use engram_mesh::bloom::BloomFilter;

// Create a filter sized for 10,000 items with 1% false positive rate
let mut filter = BloomFilter::new(10_000, 0.01);

// Insert knowledge labels
filter.insert_str("Rust");
filter.insert_str("memory safety");

// Check membership
assert!(filter.might_contain_str("Rust"));       // true (definitely or FP)
assert!(!filter.might_contain_str("JavaScript")); // false (definitely not)

// Merge two filters (e.g., combine knowledge from multiple sources)
filter.merge(&other_filter);
```

## Trust Model

### Propagated Confidence

When a fact arrives from a peer, its local confidence is calculated as:

```
local_confidence = fact.confidence * peer.trust * propagation_decay^hops
```

Where `propagation_decay = 0.9` per hop.

| Scenario              | Calculation              | Result |
|-----------------------|--------------------------|--------|
| Direct observation    | 0.90 * 1.0 * 0.9^0      | 0.90   |
| Trusted peer (1 hop)  | 0.90 * 0.80 * 0.9^1     | 0.65   |
| Friend of friend      | 0.90 * 0.80 * 0.9^2     | 0.58   |
| 3 hops away           | 0.90 * 0.80 * 0.9^3     | 0.52   |

Facts below confidence 0.05 are dropped entirely. Maximum 10 hops.

```rust
use engram_mesh::trust::propagated_confidence;

let conf = propagated_confidence(0.90, 0.80, 2);
// Some(0.5832) -- knowledge degrades with distance
```

### Adaptive Trust

Trust scores adjust based on peer behavior over time:

```rust
use engram_mesh::trust::TrustScore;

let mut trust = TrustScore::new(0.5); // neutral initial trust

// As facts from this peer are confirmed or contradicted:
trust.record_confirmation();  // increases trust
trust.record_contradiction(); // decreases trust

// Trust adjusts toward observed reliability after enough data (5+ evaluations)
// Blend: 30% initial trust + 70% observed accuracy (capped at 50 evaluations)
```

## Conflict Resolution

When peer A says "X is true" and peer B says "X is false":

1. Both facts stored with provenance
2. Confidence comparison (including peer trust weights)
3. Recency check -- newer observations may override in a tie
4. If unresolvable: both kept, flagged as "disputed"

No single peer can force consensus. Each node decides locally.

```rust
use engram_mesh::conflict::{Conflict, Resolution};

let resolution = conflict.resolve();
match resolution {
    Resolution::AcceptIncoming => { /* replace local fact */ }
    Resolution::KeepLocal      => { /* ignore incoming */ }
    Resolution::Disputed       => { /* store both, flag conflict */ }
    Resolution::Merge          => { /* combine information */ }
}
```

### Resolution Strategy

| Condition | Result |
|-----------|--------|
| Incoming confidence > local + 0.05 | AcceptIncoming |
| Local confidence > incoming + 0.05 | KeepLocal |
| Close confidence, incoming newer | AcceptIncoming |
| Close confidence, local newer | KeepLocal |
| Same confidence, same timestamp | Disputed |
| Incoming too degraded (below 0.05) | KeepLocal |

## Delta Sync Engine

The sync engine coordinates the full lifecycle:

```rust
use engram_mesh::sync;

// 1. Build a knowledge digest
let digest = sync::build_digest(&all_labels);

// 2. Compare heartbeats to find gaps
let missing = sync::needs_sync(&local_heartbeat, &remote_heartbeat, &interesting_labels);

// 3. Build a sync request
let request = sync::build_sync_request(&my_key, missing, last_sync_time, &policy);

// 4. Filter local facts before sending (respects access levels + policies)
let shareable = sync::filter_facts_for_peer(&all_facts, &peer.share_policy);

// 5. Process incoming facts with conflict resolution
let (result, accepted_facts) = sync::process_incoming(&response, &peer, &|label| {
    // lookup local fact by label -> Option<(confidence, updated_at, provenance)>
    local_lookup(label)
});

println!("Accepted: {}, Rejected: {}, Disputed: {}",
    result.accepted, result.rejected, result.disputed);
```

### Filtering Rules

Facts are excluded from sync when:
- Access level is `Private`
- Confidence is below the policy's `min_confidence`
- Label matches any `exclude_labels` entry
- Topics don't match the policy's topic filter (when filter is non-empty)
- Batch would exceed `max_batch_size`

## Audit Trail

Every fact entering through the mesh is logged in an append-only audit trail:

```rust
use engram_mesh::audit::AuditLog;

let mut log = AuditLog::load_or_new(Path::new("audit.json"), 100_000);

// Recorded automatically during sync:
// - Peer identity and name
// - Original and local confidence
// - Hop count
// - Resolution outcome (accepted/rejected/disputed)
// - Rejection reason if applicable

// Query the audit trail
let alice_entries = log.entries_for_peer(&alice_key);
let rust_entries = log.entries_for_label("Rust");
let recent = log.recent(10);
let today = log.entries_in_range(start_of_day, now);

let (accepted, rejected) = log.stats();
println!("Accepted: {accepted}, Rejected: {rejected}");
```

The audit log supports rotation -- when `max_entries` is set, old entries are dropped to stay within the limit.

## Module Reference

| Module | Description |
|--------|-------------|
| `engram_mesh::identity` | Ed25519 keypair generation, persistence, signing |
| `engram_mesh::peer` | Peer registry, sync policies, access levels |
| `engram_mesh::bloom` | Bloom filter for knowledge digests |
| `engram_mesh::gossip` | Wire protocol messages, dedup tracker |
| `engram_mesh::sync` | Delta sync engine, fact filtering |
| `engram_mesh::trust` | Propagated confidence, adaptive trust scoring |
| `engram_mesh::conflict` | Conflict detection and resolution |
| `engram_mesh::audit` | Append-only audit trail |

## Knowledge Profiles

Auto-derived domain coverage summaries computed from graph clusters. Each peer's profile describes what topics it knows about, how deeply, and how fresh that knowledge is.

### Data Model

```rust
use engram_mesh::profile::{KnowledgeProfile, DomainCoverage, NodeCapability};

/// Auto-derived profile for a mesh peer
struct KnowledgeProfile {
    peer_id: PublicKey,
    domains: Vec<DomainCoverage>,
    capabilities: Vec<NodeCapability>,
    last_updated: u64,
}

/// Coverage summary for a single domain/topic
struct DomainCoverage {
    domain: String,         // e.g., "security", "networking", "geopolitics"
    depth: u32,             // max graph depth within this domain
    node_count: u32,        // number of nodes in this cluster
    avg_confidence: f64,    // average confidence of nodes in domain
    freshness: f64,         // 0.0-1.0, based on recency of updates
}

/// Capability a node can provide
struct NodeCapability {
    name: String,           // e.g., "ner", "llm", "gpu_search"
    available: bool,
}
```

### Profile Derivation

`derive_profile()` groups nodes by type and domain labels, then calculates coverage metrics per cluster:

```rust
use engram_mesh::profile::derive_profile;

let profile = derive_profile(&graph, &my_keypair.public);
// profile.domains contains one DomainCoverage per detected cluster
```

The derivation process:
1. Groups all nodes by their `type` and domain-related properties
2. For each cluster, calculates depth (longest path within cluster), node count, average confidence, and freshness (weighted recency of last-modified timestamps)
3. Filters out clusters with fewer than 3 nodes (noise suppression)

### Topic Discovery

`covers_topic()` checks if a profile has relevant coverage for a topic. `discover_by_topic()` finds all peers covering a topic, ranked by relevance.

```rust
use engram_mesh::profile::{covers_topic, discover_by_topic};

// Check if a single profile covers a topic
let dominated = covers_topic(&profile, "cybersecurity");
// true if any domain name matches or is semantically related

// Find all peers covering a topic
let peers = discover_by_topic(&all_profiles, "cybersecurity");
// Returns Vec<(PeerId, relevance_score)> sorted by relevance descending
```

### Gossip Broadcast

Profiles are broadcast to peers via the gossip protocol. When a node's graph changes significantly (new cluster, major confidence shift), the profile is re-derived and a `ProfileUpdate` gossip message is sent to all approved peers.

```json
{
  "ProfileUpdate": {
    "sender": { "0": [/* 32 bytes */] },
    "profile": {
      "domains": [
        {"domain": "security", "depth": 3, "node_count": 1200, "avg_confidence": 0.78, "freshness": 0.92}
      ],
      "capabilities": [{"name": "gpu_search", "available": true}]
    },
    "timestamp": 1741340400000
  }
}
```

## Federated Query

Cross-mesh search protocol that fans queries out to relevant peers, collects results, and merges them with deduplication and confidence ranking.

### Query Model

```rust
use engram_mesh::federation::{FederatedQuery, FederatedResult};

/// A query that spans multiple mesh peers
struct FederatedQuery {
    query: String,              // search text
    min_confidence: f64,        // minimum confidence for results
    max_hops: u32,              // max peer-to-peer hops (default: 2)
    sensitivity_clearance: SensitivityClearance,  // ACL clearance level
    origin_peer: PublicKey,     // who initiated the query
    request_id: String,         // deduplication ID
}
```

### Sensitivity Clearance

Federated queries carry a 4-tier clearance level that controls which facts are visible in results:

| Clearance | Can see |
|-----------|---------|
| `public` | Only facts marked `public` |
| `internal` | `public` + `internal` facts |
| `confidential` | `public` + `internal` + `confidential` facts |
| `restricted` | All facts (requires highest trust level) |

The responding peer filters results based on the clearance level before sending. Facts above the requester's clearance are excluded entirely -- not redacted.

### Local Execution

`execute_local()` searches the local graph with access level filtering based on the query's sensitivity clearance:

```rust
use engram_mesh::federation::execute_local;

let local_results = execute_local(&query, &graph);
// Returns only facts that the requester's clearance permits
```

### Result Merging

`merge_results()` combines results from multiple peers, deduplicating by entity label and keeping the highest-confidence version:

```rust
use engram_mesh::federation::merge_results;

let mut all_results: Vec<FederatedResult> = Vec::new();
all_results.extend(local_results);
all_results.extend(peer_a_results);
all_results.extend(peer_b_results);

let merged = merge_results(all_results);
// Deduplicated by label, ranked by confidence descending
// Each result retains source_peer attribution
```

The merge strategy:
1. Group results by entity label
2. For duplicates, keep the result with highest confidence
3. Apply propagated confidence decay based on hop count
4. Sort final results by confidence descending
5. Each result retains `source_peer` attribution for provenance

## Security Notes

- **No automatic discovery**: Peers must be manually registered and approved
- **mTLS**: Transport encryption via ed25519-derived certificates (planned -- requires `rustls`)
- **Access levels**: Facts marked `Private` never leave the node, enforced at the sync layer
- **Trust decay**: Knowledge degrades with distance, preventing untrusted information from propagating with high confidence
- **Audit trail**: All received facts are logged with full provenance for accountability
- **Loop prevention**: Peer chain tracking and TTL prevent infinite message loops
