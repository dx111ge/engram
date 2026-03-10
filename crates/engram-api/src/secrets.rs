/// Encrypted secrets storage for engram.
///
/// Stores API keys, auth tokens, and other credentials in an AES-256-GCM
/// encrypted sidecar file (`.brain.secrets`). Master password is always
/// prompted on server startup.
///
/// File format: [magic 8B "ENGSEC\0\0"][version 4B][salt 16B][nonce 12B][ciphertext...][auth_tag 16B]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use argon2::Argon2;

const MAGIC: &[u8; 8] = b"ENGSEC\0\0";
const VERSION: u32 = 1;

/// Encrypted secrets store backed by a `.brain.secrets` sidecar file.
pub struct SecretStore {
    secrets: HashMap<String, String>,
    path: PathBuf,
    salt: [u8; 16],
    master_key: [u8; 32],
    dirty: AtomicBool,
}

impl SecretStore {
    /// Create a new secrets file with the given master password.
    pub fn create(path: &Path, password: &str) -> Result<Self, SecretStoreError> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);

        let master_key = derive_key(password, &salt)?;

        let store = Self {
            secrets: HashMap::new(),
            path: path.to_path_buf(),
            salt,
            master_key,
            dirty: AtomicBool::new(true),
        };

        store.save()?;
        Ok(store)
    }

    /// Open an existing secrets file with the given master password.
    pub fn open(path: &Path, password: &str) -> Result<Self, SecretStoreError> {
        let data = std::fs::read(path)
            .map_err(|e| SecretStoreError::Io(e.to_string()))?;

        if data.len() < 8 + 4 + 16 + 12 + 16 {
            return Err(SecretStoreError::InvalidFormat("file too short".into()));
        }

        // Verify magic
        if &data[0..8] != MAGIC {
            return Err(SecretStoreError::InvalidFormat("bad magic bytes".into()));
        }

        // Read version
        let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if version != VERSION {
            return Err(SecretStoreError::InvalidFormat(
                format!("unsupported version: {version}")
            ));
        }

        // Read salt
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&data[12..28]);

        // Read nonce
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&data[28..40]);

        // Ciphertext + auth tag
        let ciphertext = &data[40..];

        // Derive key
        let master_key = derive_key(password, &salt)?;

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(&master_key)
            .map_err(|e| SecretStoreError::Crypto(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|_| SecretStoreError::WrongPassword)?;

        let secrets: HashMap<String, String> = serde_json::from_slice(&plaintext)
            .map_err(|e| SecretStoreError::InvalidFormat(e.to_string()))?;

        Ok(Self {
            secrets,
            path: path.to_path_buf(),
            salt,
            master_key,
            dirty: AtomicBool::new(false),
        })
    }

    /// Get a secret value by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.secrets.get(key).map(|s| s.as_str())
    }

    /// Set a secret value. Marks as dirty.
    pub fn set(&mut self, key: &str, value: String) {
        self.secrets.insert(key.to_string(), value);
        self.dirty.store(true, Ordering::Release);
    }

    /// Remove a secret. Returns true if it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        let removed = self.secrets.remove(key).is_some();
        if removed {
            self.dirty.store(true, Ordering::Release);
        }
        removed
    }

    /// List all secret keys (never values).
    pub fn keys(&self) -> Vec<&str> {
        self.secrets.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }

    /// Number of stored secrets.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Encrypt and write to disk.
    pub fn save(&self) -> Result<(), SecretStoreError> {
        let plaintext = serde_json::to_vec(&self.secrets)
            .map_err(|e| SecretStoreError::Io(e.to_string()))?;

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| SecretStoreError::Crypto(e.to_string()))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, plaintext.as_ref())
            .map_err(|e| SecretStoreError::Crypto(e.to_string()))?;

        // Build file: magic + version + salt + nonce + ciphertext (includes auth tag)
        let mut out = Vec::with_capacity(8 + 4 + 16 + 12 + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);

        std::fs::write(&self.path, &out)
            .map_err(|e| SecretStoreError::Io(e.to_string()))?;

        self.dirty.store(false, Ordering::Release);
        Ok(())
    }

    /// Save only if dirty. Returns true if flushed.
    pub fn checkpoint_if_dirty(&self) -> bool {
        if self.dirty.swap(false, Ordering::AcqRel) {
            if let Err(e) = self.save() {
                tracing::warn!("secrets checkpoint failed: {}", e);
                self.dirty.store(true, Ordering::Release);
                return false;
            }
            return true;
        }
        false
    }

    /// Change the master password. Re-derives key, re-encrypts, saves.
    pub fn change_password(&mut self, new_password: &str) -> Result<(), SecretStoreError> {
        let mut new_salt = [0u8; 16];
        OsRng.fill_bytes(&mut new_salt);

        let new_key = derive_key(new_password, &new_salt)?;
        self.salt = new_salt;
        self.master_key = new_key;
        self.dirty.store(true, Ordering::Release);
        self.save()
    }
}

/// Derive a 256-bit key from password + salt using Argon2id.
fn derive_key(password: &str, salt: &[u8; 16]) -> Result<[u8; 32], SecretStoreError> {
    let mut key = [0u8; 32];
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(65536, 3, 1, Some(32))
            .map_err(|e| SecretStoreError::Crypto(e.to_string()))?,
    );
    argon2.hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| SecretStoreError::Crypto(e.to_string()))?;
    Ok(key)
}

/// Errors from the secret store.
#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("invalid file format: {0}")]
    InvalidFormat(String),
    #[error("wrong password or corrupted data")]
    WrongPassword,
    #[error("crypto error: {0}")]
    Crypto(String),
}

impl From<SecretStoreError> for std::io::Error {
    fn from(e: SecretStoreError) -> Self {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.secrets");

        // Create
        let mut store = SecretStore::create(&path, "test-password").unwrap();
        store.set("llm.api_key", "sk-test123".to_string());
        store.save().unwrap();

        // Open
        let loaded = SecretStore::open(&path, "test-password").unwrap();
        assert_eq!(loaded.get("llm.api_key"), Some("sk-test123"));
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn wrong_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.secrets");

        SecretStore::create(&path, "correct").unwrap();
        let result = SecretStore::open(&path, "wrong");
        assert!(matches!(result, Err(SecretStoreError::WrongPassword)));
    }

    #[test]
    fn key_management() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.secrets");

        let mut store = SecretStore::create(&path, "pw").unwrap();
        store.set("a", "1".to_string());
        store.set("b", "2".to_string());
        assert_eq!(store.len(), 2);
        assert!(store.has("a"));

        store.remove("a");
        assert_eq!(store.len(), 1);
        assert!(!store.has("a"));

        let keys = store.keys();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"b"));
    }

    #[test]
    fn change_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.secrets");

        let mut store = SecretStore::create(&path, "old-pw").unwrap();
        store.set("key", "value".to_string());
        store.change_password("new-pw").unwrap();

        // Old password should fail
        assert!(SecretStore::open(&path, "old-pw").is_err());

        // New password should work
        let loaded = SecretStore::open(&path, "new-pw").unwrap();
        assert_eq!(loaded.get("key"), Some("value"));
    }
}
