/// Peer management — registration, trust configuration, and sync policies.
///
/// Peers are added by mutual approval: both sides must register each other's
/// public key and endpoint before communication begins. No automatic discovery.

use crate::identity::PublicKey;

/// A registered peer in the mesh.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerConfig {
    /// Peer's public key (identity)
    pub public_key: PublicKey,
    /// Human-readable name
    pub name: String,
    /// Network endpoint (e.g., "192.168.1.50:7432")
    pub endpoint: String,
    /// Trust level: 0.0 (no trust) to 1.0 (full trust)
    pub trust: f32,
    /// Whether this peer is approved for communication
    pub approved: bool,
    /// Topics this peer subscribes to (what they want from us)
    pub subscribed_topics: Vec<String>,
    /// What we share with this peer
    pub share_policy: SyncPolicy,
    /// What we accept from this peer
    pub accept_policy: SyncPolicy,
    /// Last successful sync timestamp (unix millis)
    pub last_sync: u64,
    /// Whether peer is currently reachable
    pub online: bool,
}

/// Policy controlling what knowledge flows between peers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncPolicy {
    /// Topic filters — only sync facts matching these topics (empty = all)
    pub topics: Vec<String>,
    /// Minimum confidence to propagate
    pub min_confidence: f32,
    /// Labels to never share (privacy)
    pub exclude_labels: Vec<String>,
    /// Maximum subgraph depth to share
    pub max_depth: u32,
    /// Sync interval in seconds
    pub interval_secs: u64,
    /// Maximum facts per sync batch
    pub max_batch_size: u32,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        SyncPolicy {
            topics: Vec::new(),
            min_confidence: 0.3,
            exclude_labels: Vec::new(),
            max_depth: 3,
            interval_secs: 60,
            max_batch_size: 1000,
        }
    }
}

/// Access level for facts in the mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AccessLevel {
    /// Never leaves this node
    Private = 0,
    /// Shared within a defined peer group
    Team = 1,
    /// Propagated to all peers
    Public = 2,
    /// Structure shared, values hidden
    Redacted = 3,
}

impl AccessLevel {
    /// Decode from the 2-bit flags field.
    pub fn from_flags(flags: u32) -> Self {
        match (flags >> 30) & 0x3 {
            0 => AccessLevel::Private,
            1 => AccessLevel::Team,
            2 => AccessLevel::Public,
            3 => AccessLevel::Redacted,
            _ => AccessLevel::Private,
        }
    }

    /// Encode into a flags field (sets bits 30-31).
    pub fn to_flags(self, existing_flags: u32) -> u32 {
        (existing_flags & 0x3FFFFFFF) | ((self as u32) << 30)
    }

    /// Whether this access level allows sharing with peers.
    pub fn is_shareable(&self) -> bool {
        matches!(self, AccessLevel::Team | AccessLevel::Public | AccessLevel::Redacted)
    }
}

/// Peer registry holding all known peers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerRegistry {
    /// Known peers indexed by public key hex
    pub peers: std::collections::HashMap<String, PeerConfig>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        PeerRegistry {
            peers: std::collections::HashMap::new(),
        }
    }

    /// Register a new peer. Returns false if already registered.
    pub fn register(&mut self, peer: PeerConfig) -> bool {
        let key = peer.public_key.to_hex();
        if self.peers.contains_key(&key) {
            return false;
        }
        self.peers.insert(key, peer);
        true
    }

    /// Approve a peer for communication.
    pub fn approve(&mut self, public_key: &PublicKey) -> bool {
        if let Some(peer) = self.peers.get_mut(&public_key.to_hex()) {
            peer.approved = true;
            true
        } else {
            false
        }
    }

    /// Revoke a peer's approval.
    pub fn revoke(&mut self, public_key: &PublicKey) -> bool {
        if let Some(peer) = self.peers.get_mut(&public_key.to_hex()) {
            peer.approved = false;
            true
        } else {
            false
        }
    }

    /// Remove a peer entirely.
    pub fn remove(&mut self, public_key: &PublicKey) -> Option<PeerConfig> {
        self.peers.remove(&public_key.to_hex())
    }

    /// Get a peer by public key.
    pub fn get(&self, public_key: &PublicKey) -> Option<&PeerConfig> {
        self.peers.get(&public_key.to_hex())
    }

    /// Get a mutable peer by public key.
    pub fn get_mut(&mut self, public_key: &PublicKey) -> Option<&mut PeerConfig> {
        self.peers.get_mut(&public_key.to_hex())
    }

    /// List all approved peers.
    pub fn approved_peers(&self) -> Vec<&PeerConfig> {
        self.peers.values().filter(|p| p.approved).collect()
    }

    /// Update a peer's trust level.
    pub fn set_trust(&mut self, public_key: &PublicKey, trust: f32) -> bool {
        let trust = trust.clamp(0.0, 1.0);
        if let Some(peer) = self.peers.get_mut(&public_key.to_hex()) {
            peer.trust = trust;
            true
        } else {
            false
        }
    }

    /// Update a peer's last sync timestamp.
    pub fn update_last_sync(&mut self, public_key: &PublicKey, timestamp: u64) {
        if let Some(peer) = self.peers.get_mut(&public_key.to_hex()) {
            peer.last_sync = timestamp;
        }
    }

    /// Save registry to JSON file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load registry from JSON file.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Load or create empty registry.
    pub fn load_or_new(path: &std::path::Path) -> Self {
        if path.exists() {
            Self::load(path).unwrap_or_else(|_| Self::new())
        } else {
            Self::new()
        }
    }
}

impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    fn make_peer(name: &str, trust: f32) -> PeerConfig {
        let kp = Keypair::generate();
        PeerConfig {
            public_key: kp.public,
            name: name.to_string(),
            endpoint: format!("localhost:{}", 7000 + name.len()),
            trust,
            approved: false,
            subscribed_topics: vec!["general".to_string()],
            share_policy: SyncPolicy::default(),
            accept_policy: SyncPolicy::default(),
            last_sync: 0,
            online: false,
        }
    }

    #[test]
    fn register_and_approve() {
        let mut reg = PeerRegistry::new();
        let peer = make_peer("alice", 0.8);
        let pk = peer.public_key.clone();
        assert!(reg.register(peer));
        assert!(reg.approved_peers().is_empty());
        assert!(reg.approve(&pk));
        assert_eq!(reg.approved_peers().len(), 1);
    }

    #[test]
    fn no_duplicate_registration() {
        let mut reg = PeerRegistry::new();
        let peer = make_peer("bob", 0.5);
        let peer2 = peer.clone();
        assert!(reg.register(peer));
        assert!(!reg.register(peer2));
    }

    #[test]
    fn revoke_and_remove() {
        let mut reg = PeerRegistry::new();
        let peer = make_peer("carol", 0.9);
        let pk = peer.public_key.clone();
        reg.register(peer);
        reg.approve(&pk);
        assert_eq!(reg.approved_peers().len(), 1);
        reg.revoke(&pk);
        assert!(reg.approved_peers().is_empty());
        assert!(reg.remove(&pk).is_some());
        assert!(reg.get(&pk).is_none());
    }

    #[test]
    fn trust_clamping() {
        let mut reg = PeerRegistry::new();
        let peer = make_peer("dave", 0.5);
        let pk = peer.public_key.clone();
        reg.register(peer);
        reg.set_trust(&pk, 1.5); // over max
        assert_eq!(reg.get(&pk).unwrap().trust, 1.0);
        reg.set_trust(&pk, -0.5); // under min
        assert_eq!(reg.get(&pk).unwrap().trust, 0.0);
    }

    #[test]
    fn access_level_flags() {
        assert_eq!(AccessLevel::from_flags(0), AccessLevel::Private);
        assert_eq!(AccessLevel::from_flags(AccessLevel::Public.to_flags(0)), AccessLevel::Public);
        assert_eq!(AccessLevel::from_flags(AccessLevel::Team.to_flags(0)), AccessLevel::Team);
        assert_eq!(AccessLevel::from_flags(AccessLevel::Redacted.to_flags(0)), AccessLevel::Redacted);
        // Preserves lower flags
        let flags = AccessLevel::Public.to_flags(0xFF);
        assert_eq!(flags & 0xFF, 0xFF);
        assert_eq!(AccessLevel::from_flags(flags), AccessLevel::Public);
    }

    #[test]
    fn save_and_load_registry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.json");
        let mut reg = PeerRegistry::new();
        let peer = make_peer("eve", 0.7);
        let pk = peer.public_key.clone();
        reg.register(peer);
        reg.approve(&pk);
        reg.save(&path).unwrap();

        let loaded = PeerRegistry::load(&path).unwrap();
        assert_eq!(loaded.approved_peers().len(), 1);
        assert_eq!(loaded.get(&pk).unwrap().name, "eve");
    }

    #[test]
    fn shareable_access_levels() {
        assert!(!AccessLevel::Private.is_shareable());
        assert!(AccessLevel::Team.is_shareable());
        assert!(AccessLevel::Public.is_shareable());
        assert!(AccessLevel::Redacted.is_shareable());
    }
}
