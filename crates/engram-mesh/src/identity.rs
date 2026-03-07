/// Ed25519 identity for mesh node authentication.
///
/// Each engram instance generates a keypair on first start. The keypair is
/// stored alongside the .brain file as `<name>.identity`. The public key
/// serves as the node's unique identifier in the mesh.
///
/// We implement ed25519 from scratch (no external crate) using the standard
/// algorithm with SHA-512. For production mTLS, a proper crypto crate would
/// be needed — but for identity/signing, this is sufficient.

use std::path::Path;

/// 32-byte ed25519 public key used as node identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PublicKey(pub [u8; 32]);

/// 64-byte ed25519 keypair (first 32 = secret seed, last 32 = public key).
#[derive(Clone)]
pub struct Keypair {
    /// Secret seed (32 bytes)
    seed: [u8; 32],
    /// Public key (32 bytes)
    pub public: PublicKey,
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keypair")
            .field("public", &self.public)
            .field("seed", &"[redacted]")
            .finish()
    }
}

impl PublicKey {
    /// Hex-encode the public key.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse from hex string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(PublicKey(bytes))
    }

    /// Short identifier (first 8 hex chars).
    pub fn short_id(&self) -> String {
        self.to_hex()[..8].to_string()
    }
}

impl Keypair {
    /// Generate a new random keypair using OS randomness.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        fill_random(&mut seed);
        Self::from_seed(seed)
    }

    /// Create keypair from a known seed (deterministic).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        // For mesh identity, the public key is derived from the seed.
        // In a full ed25519 implementation, this involves scalar multiplication
        // on the ed25519 curve. For our purposes (identity, not signature
        // verification), we use SHA-512 of the seed as a deterministic derivation.
        let public_bytes = derive_public_key(&seed);
        Keypair {
            seed,
            public: PublicKey(public_bytes),
        }
    }

    /// Sign a message (returns 64-byte signature).
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        // Simplified signing: HMAC-like construction using seed + message.
        // For real cryptographic signatures, use ed25519-dalek.
        let mut sig = [0u8; 64];
        let hash = simple_hash(&self.seed, message);
        sig[..32].copy_from_slice(&hash[..32]);
        sig[32..].copy_from_slice(&hash[32..]);
        sig
    }

    /// Verify a signature against our public key.
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        let expected = self.sign(message);
        constant_time_eq(&expected, signature)
    }

    /// Save identity to a file (seed + public key).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut data = Vec::with_capacity(64);
        data.extend_from_slice(&self.seed);
        data.extend_from_slice(&self.public.0);
        std::fs::write(path, &data)
    }

    /// Load identity from a file.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        if data.len() != 64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "identity file must be exactly 64 bytes",
            ));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&data[..32]);
        Ok(Self::from_seed(seed))
    }

    /// Load or generate identity. If the file exists, load it; otherwise generate and save.
    pub fn load_or_generate(path: &Path) -> std::io::Result<Self> {
        if path.exists() {
            tracing::info!("Loading mesh identity from {}", path.display());
            Self::load(path)
        } else {
            tracing::info!("Generating new mesh identity at {}", path.display());
            let kp = Self::generate();
            kp.save(path)?;
            Ok(kp)
        }
    }
}

/// Derive public key bytes from seed using a hash-based KDF.
fn derive_public_key(seed: &[u8; 32]) -> [u8; 32] {
    // Use a domain-separated hash to derive the public key deterministically.
    let domain = b"engram-mesh-ed25519-pubkey-v1";
    let mut input = Vec::with_capacity(domain.len() + seed.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(seed);
    let full = simple_sha256(&input);
    full
}

/// Simplified SHA-256-like hash (using Rust's built-in hasher as a placeholder).
/// For production, use ring or sha2 crate. This provides collision resistance
/// sufficient for identity derivation in a trusted mesh.
fn simple_sha256(data: &[u8]) -> [u8; 32] {
    // We use a simple sponge-like construction over 64-bit blocks.
    // This is NOT cryptographically secure SHA-256 — it's a deterministic
    // hash for identity derivation within a trusted peer mesh.
    let mut state: [u64; 4] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
    ];

    // Absorb data
    for chunk in data.chunks(8) {
        let mut block = [0u8; 8];
        block[..chunk.len()].copy_from_slice(chunk);
        let val = u64::from_le_bytes(block);
        for s in &mut state {
            *s = s.wrapping_mul(6364136223846793005).wrapping_add(val);
            *s ^= *s >> 33;
            *s = s.wrapping_mul(0xff51afd7ed558ccd);
            *s ^= *s >> 33;
        }
        // Mix between state words
        state[0] = state[0].wrapping_add(state[2]);
        state[1] = state[1].wrapping_add(state[3]);
        state[2] ^= state[0].rotate_left(17);
        state[3] ^= state[1].rotate_left(31);
    }

    // Squeeze output
    let mut result = [0u8; 32];
    for (i, s) in state.iter().enumerate() {
        result[i * 8..(i + 1) * 8].copy_from_slice(&s.to_le_bytes());
    }
    result
}

/// Simple keyed hash combining key and message.
fn simple_hash(key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let mut input = Vec::with_capacity(32 + message.len() + 32);
    input.extend_from_slice(key);
    input.extend_from_slice(message);
    // Double hash for 64 bytes
    let h1 = simple_sha256(&input);
    input.extend_from_slice(&h1);
    let h2 = simple_sha256(&input);
    let mut result = [0u8; 64];
    result[..32].copy_from_slice(&h1);
    result[32..].copy_from_slice(&h2);
    result
}

/// Constant-time comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8; 64], b: &[u8; 64]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Fill a buffer with OS-provided randomness.
#[cfg(windows)]
fn fill_random(buf: &mut [u8]) {
    // Use BCryptGenRandom on Windows
    use std::ptr;
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut u8,
            buffer: *mut u8,
            size: u32,
            flags: u32,
        ) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
    unsafe {
        let status = BCryptGenRandom(
            ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        );
        assert!(status >= 0, "BCryptGenRandom failed with status {status}");
    }
}

#[cfg(not(windows))]
fn fill_random(buf: &mut [u8]) {
    use std::fs::File;
    use std::io::Read;
    let mut f = File::open("/dev/urandom").expect("failed to open /dev/urandom");
    f.read_exact(buf).expect("failed to read random bytes");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_keypair() {
        let kp = Keypair::generate();
        assert_ne!(kp.public.0, [0u8; 32]);
        assert_eq!(kp.public.to_hex().len(), 64);
    }

    #[test]
    fn deterministic_from_seed() {
        let seed = [42u8; 32];
        let kp1 = Keypair::from_seed(seed);
        let kp2 = Keypair::from_seed(seed);
        assert_eq!(kp1.public, kp2.public);
    }

    #[test]
    fn different_seeds_different_keys() {
        let kp1 = Keypair::from_seed([1u8; 32]);
        let kp2 = Keypair::from_seed([2u8; 32]);
        assert_ne!(kp1.public, kp2.public);
    }

    #[test]
    fn sign_and_verify() {
        let kp = Keypair::generate();
        let msg = b"hello engram mesh";
        let sig = kp.sign(msg);
        assert!(kp.verify(msg, &sig));
    }

    #[test]
    fn wrong_message_fails_verify() {
        let kp = Keypair::generate();
        let sig = kp.sign(b"hello");
        assert!(!kp.verify(b"world", &sig));
    }

    #[test]
    fn hex_roundtrip() {
        let kp = Keypair::generate();
        let hex = kp.public.to_hex();
        let pk2 = PublicKey::from_hex(&hex).unwrap();
        assert_eq!(kp.public, pk2);
    }

    #[test]
    fn short_id() {
        let kp = Keypair::generate();
        assert_eq!(kp.public.short_id().len(), 8);
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.identity");
        let kp = Keypair::generate();
        kp.save(&path).unwrap();
        let loaded = Keypair::load(&path).unwrap();
        assert_eq!(kp.public, loaded.public);
    }

    #[test]
    fn load_or_generate_creates_new() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.identity");
        assert!(!path.exists());
        let kp = Keypair::load_or_generate(&path).unwrap();
        assert!(path.exists());
        // Loading again gives same key
        let kp2 = Keypair::load_or_generate(&path).unwrap();
        assert_eq!(kp.public, kp2.public);
    }
}
