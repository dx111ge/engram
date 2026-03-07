/// Bloom filter for compact knowledge digests.
///
/// Used in gossip protocol to exchange "what I know" summaries between peers.
/// A peer can quickly check if another peer might have knowledge about a topic
/// without transferring the actual data.

/// Bloom filter with configurable size and hash count.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BloomFilter {
    /// Bit array stored as bytes
    bits: Vec<u8>,
    /// Number of bits
    num_bits: u32,
    /// Number of hash functions
    num_hashes: u8,
    /// Number of items inserted
    count: u32,
}

impl BloomFilter {
    /// Create a new bloom filter sized for expected items with target false positive rate.
    ///
    /// # Arguments
    /// * `expected_items` - Expected number of items
    /// * `fp_rate` - Target false positive rate (e.g., 0.01 for 1%)
    pub fn new(expected_items: u32, fp_rate: f64) -> Self {
        let fp_rate = fp_rate.max(0.0001).min(0.5);
        // Optimal number of bits: -n * ln(p) / (ln(2)^2)
        let num_bits = (-(expected_items as f64) * fp_rate.ln() / (2.0_f64.ln().powi(2)))
            .ceil() as u32;
        let num_bits = num_bits.max(64); // minimum 64 bits
        // Optimal number of hashes: (m/n) * ln(2)
        let num_hashes = ((num_bits as f64 / expected_items as f64) * 2.0_f64.ln())
            .ceil() as u8;
        let num_hashes = num_hashes.max(1).min(16);

        let byte_count = ((num_bits + 7) / 8) as usize;
        BloomFilter {
            bits: vec![0u8; byte_count],
            num_bits,
            num_hashes,
            count: 0,
        }
    }

    /// Create with explicit parameters.
    pub fn with_params(num_bits: u32, num_hashes: u8) -> Self {
        let byte_count = ((num_bits + 7) / 8) as usize;
        BloomFilter {
            bits: vec![0u8; byte_count],
            num_bits,
            num_hashes: num_hashes.max(1),
            count: 0,
        }
    }

    /// Insert an item into the filter.
    pub fn insert(&mut self, item: &[u8]) {
        let (h1, h2) = self.hash_pair(item);
        for i in 0..self.num_hashes as u64 {
            let bit = (h1.wrapping_add(i.wrapping_mul(h2))) % self.num_bits as u64;
            self.set_bit(bit as u32);
        }
        self.count += 1;
    }

    /// Insert a string label.
    pub fn insert_str(&mut self, s: &str) {
        self.insert(s.as_bytes());
    }

    /// Check if an item might be in the filter.
    /// Returns true if possibly present, false if definitely not present.
    pub fn might_contain(&self, item: &[u8]) -> bool {
        let (h1, h2) = self.hash_pair(item);
        for i in 0..self.num_hashes as u64 {
            let bit = (h1.wrapping_add(i.wrapping_mul(h2))) % self.num_bits as u64;
            if !self.get_bit(bit as u32) {
                return false;
            }
        }
        true
    }

    /// Check if a string label might be in the filter.
    pub fn might_contain_str(&self, s: &str) -> bool {
        self.might_contain(s.as_bytes())
    }

    /// Number of items inserted.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Estimated false positive rate at current fill level.
    pub fn estimated_fp_rate(&self) -> f64 {
        let k = self.num_hashes as f64;
        let m = self.num_bits as f64;
        let n = self.count as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }

    /// Size of the filter in bytes.
    pub fn size_bytes(&self) -> usize {
        self.bits.len()
    }

    /// Merge another bloom filter into this one (union).
    /// Both filters must have the same parameters.
    pub fn merge(&mut self, other: &BloomFilter) -> bool {
        if self.num_bits != other.num_bits || self.num_hashes != other.num_hashes {
            return false;
        }
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a |= b;
        }
        // Count becomes approximate after merge
        self.count = self.count.saturating_add(other.count);
        true
    }

    /// Clear the filter.
    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.count = 0;
    }

    fn set_bit(&mut self, pos: u32) {
        let byte_idx = (pos / 8) as usize;
        let bit_idx = pos % 8;
        if byte_idx < self.bits.len() {
            self.bits[byte_idx] |= 1 << bit_idx;
        }
    }

    fn get_bit(&self, pos: u32) -> bool {
        let byte_idx = (pos / 8) as usize;
        let bit_idx = pos % 8;
        if byte_idx < self.bits.len() {
            (self.bits[byte_idx] >> bit_idx) & 1 == 1
        } else {
            false
        }
    }

    /// Double hashing: produce two independent hashes for Kirsch-Mitzenmacher.
    fn hash_pair(&self, item: &[u8]) -> (u64, u64) {
        // FNV-1a variant for h1
        let mut h1: u64 = 0xcbf29ce484222325;
        for &b in item {
            h1 ^= b as u64;
            h1 = h1.wrapping_mul(0x100000001b3);
        }

        // Different seed for h2
        let mut h2: u64 = 0x517cc1b727220a95;
        for &b in item {
            h2 = h2.wrapping_mul(6364136223846793005).wrapping_add(b as u64);
            h2 ^= h2 >> 33;
        }

        (h1, h2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_check() {
        let mut bf = BloomFilter::new(100, 0.01);
        bf.insert_str("hello");
        bf.insert_str("world");
        assert!(bf.might_contain_str("hello"));
        assert!(bf.might_contain_str("world"));
        assert!(!bf.might_contain_str("missing"));
    }

    #[test]
    fn empty_filter_never_matches() {
        let bf = BloomFilter::new(100, 0.01);
        assert!(!bf.might_contain_str("anything"));
        assert_eq!(bf.count(), 0);
    }

    #[test]
    fn false_positive_rate() {
        let mut bf = BloomFilter::new(1000, 0.01);
        for i in 0..1000 {
            bf.insert_str(&format!("item_{i}"));
        }
        // Check items we never inserted
        let mut false_positives = 0;
        let test_count = 10000;
        for i in 0..test_count {
            if bf.might_contain_str(&format!("other_{i}")) {
                false_positives += 1;
            }
        }
        let fp_rate = false_positives as f64 / test_count as f64;
        // Should be roughly under 5% (allowing headroom over target 1%)
        assert!(fp_rate < 0.05, "FP rate {fp_rate} is too high");
    }

    #[test]
    fn merge_filters() {
        let mut bf1 = BloomFilter::new(100, 0.01);
        let mut bf2 = BloomFilter::new(100, 0.01);
        bf1.insert_str("alpha");
        bf2.insert_str("beta");
        assert!(!bf1.might_contain_str("beta"));
        assert!(bf1.merge(&bf2));
        assert!(bf1.might_contain_str("alpha"));
        assert!(bf1.might_contain_str("beta"));
    }

    #[test]
    fn merge_incompatible_fails() {
        let mut bf1 = BloomFilter::new(100, 0.01);
        let bf2 = BloomFilter::new(200, 0.01);
        assert!(!bf1.merge(&bf2));
    }

    #[test]
    fn clear_filter() {
        let mut bf = BloomFilter::new(100, 0.01);
        bf.insert_str("test");
        assert!(bf.might_contain_str("test"));
        bf.clear();
        assert!(!bf.might_contain_str("test"));
        assert_eq!(bf.count(), 0);
    }

    #[test]
    fn with_params() {
        let bf = BloomFilter::with_params(256, 3);
        assert_eq!(bf.size_bytes(), 32);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut bf = BloomFilter::new(100, 0.01);
        bf.insert_str("serialize_me");
        let json = serde_json::to_string(&bf).unwrap();
        let bf2: BloomFilter = serde_json::from_str(&json).unwrap();
        assert!(bf2.might_contain_str("serialize_me"));
        assert!(!bf2.might_contain_str("not_here"));
    }
}
