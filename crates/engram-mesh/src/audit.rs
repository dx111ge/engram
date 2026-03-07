/// Audit trail for all received facts from peers.
///
/// Every fact that enters this node through the mesh is logged in an
/// append-only audit log. This provides:
/// - Traceability: who sent what, when
/// - Accountability: dispute resolution evidence
/// - Debugging: sync issue investigation

use crate::conflict::Resolution;
use crate::identity::PublicKey;

/// A single audit entry recording a received fact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// When this entry was created (unix millis)
    pub timestamp: u64,
    /// Peer that sent the fact
    pub peer_key: PublicKey,
    /// Peer name (for readability)
    pub peer_name: String,
    /// The fact label
    pub label: String,
    /// Original confidence at source
    pub source_confidence: f32,
    /// Effective local confidence after trust/decay
    pub local_confidence: f32,
    /// Number of hops from origin
    pub hops: u8,
    /// Resolution outcome
    pub resolution: String,
    /// Whether the fact was accepted
    pub accepted: bool,
    /// Reason for rejection (if applicable)
    pub rejection_reason: Option<String>,
}

/// Append-only audit log stored alongside the .brain file.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
    /// Maximum entries before rotation (0 = unlimited)
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        AuditLog {
            entries: Vec::new(),
            max_entries,
        }
    }

    /// Record a fact acceptance.
    pub fn record_accepted(
        &mut self,
        peer_key: &PublicKey,
        peer_name: &str,
        label: &str,
        source_confidence: f32,
        local_confidence: f32,
        hops: u8,
        resolution: Resolution,
        timestamp: u64,
    ) {
        self.add(AuditEntry {
            timestamp,
            peer_key: peer_key.clone(),
            peer_name: peer_name.to_string(),
            label: label.to_string(),
            source_confidence,
            local_confidence,
            hops,
            resolution: format!("{resolution:?}"),
            accepted: true,
            rejection_reason: None,
        });
    }

    /// Record a fact rejection.
    pub fn record_rejected(
        &mut self,
        peer_key: &PublicKey,
        peer_name: &str,
        label: &str,
        source_confidence: f32,
        hops: u8,
        reason: &str,
        timestamp: u64,
    ) {
        self.add(AuditEntry {
            timestamp,
            peer_key: peer_key.clone(),
            peer_name: peer_name.to_string(),
            label: label.to_string(),
            source_confidence,
            local_confidence: 0.0,
            hops,
            resolution: "Rejected".to_string(),
            accepted: false,
            rejection_reason: Some(reason.to_string()),
        });
    }

    fn add(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
        // Rotate if over limit
        if self.max_entries > 0 && self.entries.len() > self.max_entries {
            let drain_count = self.entries.len() - self.max_entries;
            self.entries.drain(..drain_count);
        }
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get entries for a specific peer.
    pub fn entries_for_peer(&self, peer_key: &PublicKey) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| &e.peer_key == peer_key)
            .collect()
    }

    /// Get entries for a specific label.
    pub fn entries_for_label(&self, label: &str) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.label == label)
            .collect()
    }

    /// Get entries in a time range.
    pub fn entries_in_range(&self, from: u64, to: u64) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.timestamp >= from && e.timestamp <= to)
            .collect()
    }

    /// Get the most recent N entries.
    pub fn recent(&self, n: usize) -> Vec<&AuditEntry> {
        let start = self.entries.len().saturating_sub(n);
        self.entries[start..].iter().collect()
    }

    /// Count of accepted vs rejected facts.
    pub fn stats(&self) -> (u64, u64) {
        let accepted = self.entries.iter().filter(|e| e.accepted).count() as u64;
        let rejected = self.entries.iter().filter(|e| !e.accepted).count() as u64;
        (accepted, rejected)
    }

    /// Save audit log to a file (JSON lines format).
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load audit log from a file.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Load or create new audit log.
    pub fn load_or_new(path: &std::path::Path, max_entries: usize) -> Self {
        if path.exists() {
            Self::load(path).unwrap_or_else(|_| Self::new(max_entries))
        } else {
            Self::new(max_entries)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    fn make_log() -> (AuditLog, PublicKey) {
        let kp = Keypair::generate();
        (AuditLog::new(0), kp.public)
    }

    #[test]
    fn record_accepted() {
        let (mut log, pk) = make_log();
        log.record_accepted(&pk, "alice", "fact1", 0.9, 0.72, 1, Resolution::AcceptIncoming, 1000);
        assert_eq!(log.len(), 1);
        assert!(log.entries[0].accepted);
    }

    #[test]
    fn record_rejected() {
        let (mut log, pk) = make_log();
        log.record_rejected(&pk, "alice", "fact2", 0.1, 5, "below threshold", 1000);
        assert_eq!(log.len(), 1);
        assert!(!log.entries[0].accepted);
        assert_eq!(log.entries[0].rejection_reason.as_deref(), Some("below threshold"));
    }

    #[test]
    fn stats_counting() {
        let (mut log, pk) = make_log();
        log.record_accepted(&pk, "a", "f1", 0.9, 0.7, 1, Resolution::AcceptIncoming, 1000);
        log.record_accepted(&pk, "a", "f2", 0.8, 0.6, 1, Resolution::AcceptIncoming, 1001);
        log.record_rejected(&pk, "a", "f3", 0.1, 5, "too weak", 1002);
        let (accepted, rejected) = log.stats();
        assert_eq!(accepted, 2);
        assert_eq!(rejected, 1);
    }

    #[test]
    fn rotation() {
        let (mut log, pk) = (AuditLog::new(3), Keypair::generate().public);
        for i in 0..5 {
            log.record_accepted(&pk, "a", &format!("f{i}"), 0.9, 0.7, 1, Resolution::AcceptIncoming, i);
        }
        assert_eq!(log.len(), 3);
        // Should keep the last 3
        assert_eq!(log.entries[0].label, "f2");
    }

    #[test]
    fn query_by_peer() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let mut log = AuditLog::new(0);
        log.record_accepted(&kp1.public, "alice", "f1", 0.9, 0.7, 1, Resolution::AcceptIncoming, 1000);
        log.record_accepted(&kp2.public, "bob", "f2", 0.8, 0.6, 1, Resolution::AcceptIncoming, 1001);
        assert_eq!(log.entries_for_peer(&kp1.public).len(), 1);
        assert_eq!(log.entries_for_peer(&kp2.public).len(), 1);
    }

    #[test]
    fn query_by_label() {
        let (mut log, pk) = make_log();
        log.record_accepted(&pk, "a", "rust", 0.9, 0.7, 1, Resolution::AcceptIncoming, 1000);
        log.record_accepted(&pk, "a", "python", 0.8, 0.6, 1, Resolution::AcceptIncoming, 1001);
        log.record_accepted(&pk, "a", "rust", 0.7, 0.5, 2, Resolution::Disputed, 1002);
        assert_eq!(log.entries_for_label("rust").len(), 2);
    }

    #[test]
    fn time_range_query() {
        let (mut log, pk) = make_log();
        log.record_accepted(&pk, "a", "f1", 0.9, 0.7, 1, Resolution::AcceptIncoming, 1000);
        log.record_accepted(&pk, "a", "f2", 0.8, 0.6, 1, Resolution::AcceptIncoming, 2000);
        log.record_accepted(&pk, "a", "f3", 0.7, 0.5, 1, Resolution::AcceptIncoming, 3000);
        assert_eq!(log.entries_in_range(1500, 2500).len(), 1);
    }

    #[test]
    fn recent_entries() {
        let (mut log, pk) = make_log();
        for i in 0..10 {
            log.record_accepted(&pk, "a", &format!("f{i}"), 0.9, 0.7, 1, Resolution::AcceptIncoming, i);
        }
        let recent = log.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].label, "f7");
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.json");
        let (mut log, pk) = make_log();
        log.record_accepted(&pk, "alice", "f1", 0.9, 0.7, 1, Resolution::AcceptIncoming, 1000);
        log.save(&path).unwrap();
        let loaded = AuditLog::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries[0].label, "f1");
    }
}
