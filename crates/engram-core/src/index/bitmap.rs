/// Bitmap index for fast filtering by node_type, memory_tier, and sensitivity.
///
/// Each attribute value maps to a bitset of node slots.
/// Supports AND/OR/NOT operations for combining filters.

use std::collections::HashMap;

/// A simple growable bitset.
#[derive(Debug, Clone)]
pub struct BitSet {
    bits: Vec<u64>,
}

impl BitSet {
    pub fn new() -> Self {
        BitSet { bits: Vec::new() }
    }

    pub fn with_capacity(max_slot: usize) -> Self {
        let words = (max_slot + 63) / 64;
        BitSet { bits: vec![0; words] }
    }

    fn ensure_capacity(&mut self, slot: u64) {
        let word = (slot / 64) as usize;
        if word >= self.bits.len() {
            self.bits.resize(word + 1, 0);
        }
    }

    pub fn set(&mut self, slot: u64) {
        self.ensure_capacity(slot);
        let word = (slot / 64) as usize;
        let bit = slot % 64;
        self.bits[word] |= 1u64 << bit;
    }

    pub fn clear(&mut self, slot: u64) {
        let word = (slot / 64) as usize;
        if word < self.bits.len() {
            let bit = slot % 64;
            self.bits[word] &= !(1u64 << bit);
        }
    }

    pub fn contains(&self, slot: u64) -> bool {
        let word = (slot / 64) as usize;
        if word >= self.bits.len() {
            return false;
        }
        let bit = slot % 64;
        self.bits[word] & (1u64 << bit) != 0
    }

    /// Iterate over all set bits.
    pub fn iter(&self) -> BitSetIter<'_> {
        BitSetIter {
            bits: &self.bits,
            word_idx: 0,
            current: if self.bits.is_empty() { 0 } else { self.bits[0] },
        }
    }

    /// Count of set bits.
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// AND with another bitset, returning new bitset.
    pub fn and(&self, other: &BitSet) -> BitSet {
        let len = self.bits.len().min(other.bits.len());
        let mut result = BitSet { bits: vec![0; len] };
        for i in 0..len {
            result.bits[i] = self.bits[i] & other.bits[i];
        }
        result
    }

    /// OR with another bitset, returning new bitset.
    pub fn or(&self, other: &BitSet) -> BitSet {
        let len = self.bits.len().max(other.bits.len());
        let mut result = BitSet { bits: vec![0; len] };
        for i in 0..len {
            let a = self.bits.get(i).copied().unwrap_or(0);
            let b = other.bits.get(i).copied().unwrap_or(0);
            result.bits[i] = a | b;
        }
        result
    }

    /// Collect all set bits into a Vec.
    pub fn to_vec(&self) -> Vec<u64> {
        self.iter().collect()
    }
}

pub struct BitSetIter<'a> {
    bits: &'a [u64],
    word_idx: usize,
    current: u64,
}

impl Iterator for BitSetIter<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        while self.current == 0 {
            self.word_idx += 1;
            if self.word_idx >= self.bits.len() {
                return None;
            }
            self.current = self.bits[self.word_idx];
        }
        let bit = self.current.trailing_zeros() as u64;
        self.current &= self.current - 1; // clear lowest set bit
        Some(self.word_idx as u64 * 64 + bit)
    }
}

/// Bitmap index for a single attribute dimension (e.g. node_type).
pub struct BitmapIndex {
    /// attribute_value -> bitset of matching slots
    bitmaps: HashMap<u32, BitSet>,
}

impl BitmapIndex {
    pub fn new() -> Self {
        BitmapIndex {
            bitmaps: HashMap::new(),
        }
    }

    /// Set a slot for a given attribute value.
    pub fn insert(&mut self, value: u32, slot: u64) {
        self.bitmaps.entry(value).or_insert_with(BitSet::new).set(slot);
    }

    /// Remove a slot from a given attribute value.
    pub fn remove(&mut self, value: u32, slot: u64) {
        if let Some(bs) = self.bitmaps.get_mut(&value) {
            bs.clear(slot);
        }
    }

    /// Get the bitset for a specific attribute value.
    pub fn get(&self, value: u32) -> Option<&BitSet> {
        self.bitmaps.get(&value)
    }

    /// Get slots matching a specific value.
    pub fn slots_for(&self, value: u32) -> Vec<u64> {
        self.bitmaps
            .get(&value)
            .map(|bs| bs.to_vec())
            .unwrap_or_default()
    }

    /// Get slots matching any of the given values (OR).
    pub fn slots_for_any(&self, values: &[u32]) -> Vec<u64> {
        let mut result = BitSet::new();
        for &v in values {
            if let Some(bs) = self.bitmaps.get(&v) {
                result = result.or(bs);
            }
        }
        result.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitset_basic() {
        let mut bs = BitSet::new();
        bs.set(0);
        bs.set(5);
        bs.set(64);
        bs.set(100);

        assert!(bs.contains(0));
        assert!(bs.contains(5));
        assert!(bs.contains(64));
        assert!(bs.contains(100));
        assert!(!bs.contains(1));
        assert!(!bs.contains(65));
        assert_eq!(bs.count(), 4);
    }

    #[test]
    fn bitset_clear() {
        let mut bs = BitSet::new();
        bs.set(10);
        bs.set(20);
        bs.clear(10);
        assert!(!bs.contains(10));
        assert!(bs.contains(20));
    }

    #[test]
    fn bitset_iter() {
        let mut bs = BitSet::new();
        bs.set(3);
        bs.set(7);
        bs.set(64);
        bs.set(130);

        let collected: Vec<u64> = bs.iter().collect();
        assert_eq!(collected, vec![3, 7, 64, 130]);
    }

    #[test]
    fn bitset_and() {
        let mut a = BitSet::new();
        a.set(1);
        a.set(2);
        a.set(3);

        let mut b = BitSet::new();
        b.set(2);
        b.set(3);
        b.set(4);

        let result = a.and(&b);
        assert_eq!(result.to_vec(), vec![2, 3]);
    }

    #[test]
    fn bitset_or() {
        let mut a = BitSet::new();
        a.set(1);
        a.set(2);

        let mut b = BitSet::new();
        b.set(2);
        b.set(3);

        let result = a.or(&b);
        assert_eq!(result.to_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn bitmap_index_basic() {
        let mut idx = BitmapIndex::new();
        // node_type 0 = server, 1 = database
        idx.insert(0, 0); // slot 0 is a server
        idx.insert(0, 2); // slot 2 is a server
        idx.insert(1, 1); // slot 1 is a database
        idx.insert(1, 3); // slot 3 is a database

        assert_eq!(idx.slots_for(0), vec![0, 2]);
        assert_eq!(idx.slots_for(1), vec![1, 3]);
        assert_eq!(idx.slots_for(99), Vec::<u64>::new());
    }

    #[test]
    fn bitmap_index_or() {
        let mut idx = BitmapIndex::new();
        idx.insert(0, 0);
        idx.insert(1, 1);
        idx.insert(2, 2);

        let result = idx.slots_for_any(&[0, 2]);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn bitmap_index_remove() {
        let mut idx = BitmapIndex::new();
        idx.insert(0, 0);
        idx.insert(0, 1);
        idx.remove(0, 0);
        assert_eq!(idx.slots_for(0), vec![1]);
    }
}
