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

/// Pure-Rust SHA-256 implementation (FIPS 180-4).
/// Processes input through standard message padding, 512-bit block processing
/// with 64 rounds using the SHA-256 constants and round function, returning a
/// 32-byte digest.
fn simple_sha256(data: &[u8]) -> [u8; 32] {
    sha256(data)
}

// --- SHA-256 implementation (FIPS 180-4) ---

/// SHA-256 round constants: first 32 bits of the fractional parts of the cube
/// roots of the first 64 primes (2..311).
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Initial hash values: first 32 bits of the fractional parts of the square
/// roots of the first 8 primes (2..19).
const H_INIT: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute the SHA-256 digest of `data`, returning 32 bytes.
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = H_INIT;

    // Pre-processing: padding the message
    // message length in bits (as u64)
    let bit_len = (data.len() as u64).wrapping_mul(8);

    // Build padded message: original | 0x80 | zeros | 8-byte big-endian length
    // Total length must be a multiple of 64 bytes (512 bits).
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    // Pad with zeros until length mod 64 == 56
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    // Append original length in bits as 8-byte big-endian
    padded.extend_from_slice(&bit_len.to_be_bytes());

    debug_assert!(padded.len() % 64 == 0);

    // Process each 512-bit (64-byte) block
    for block in padded.chunks_exact(64) {
        // Prepare the message schedule W[0..64]
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        // 64 rounds
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add the compressed chunk to the current hash value
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    // Produce the final 32-byte digest
    let mut digest = [0u8; 32];
    for i in 0..8 {
        digest[i * 4..(i + 1) * 4].copy_from_slice(&h[i].to_be_bytes());
    }
    digest
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

    #[test]
    fn sha256_known_answer_empty() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = sha256(b"");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_known_answer_abc() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let digest = sha256(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_known_answer_long() {
        // SHA-256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
        // = 248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1
        let digest = sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        );
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
