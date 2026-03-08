//! engram-mesh: peer-to-peer knowledge synchronization.
//!
//! Implements the knowledge mesh protocol for syncing facts between engram instances:
//! - [`identity`] -- ed25519 keypair management and peer authentication
//! - [`peer`] -- peer registry with trust scores and access control
//! - [`sync`] -- delta synchronization with bloom filter digests
//! - [`gossip`] -- peer discovery and heartbeat protocol
//! - [`conflict`] -- deterministic conflict resolution for concurrent edits
//! - [`trust`] -- trust scoring and reputation tracking
//! - [`bloom`] -- bloom filter for efficient set membership testing
//! - [`audit`] -- audit trail for all sync operations

pub mod audit;
pub mod bloom;
pub mod conflict;
pub mod gossip;
pub mod identity;
pub mod peer;
pub mod sync;
pub mod trust;
