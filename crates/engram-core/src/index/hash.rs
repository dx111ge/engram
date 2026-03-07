/// In-memory hash index for fast node lookup by label.
///
/// Rebuilt on open from the node region. Maps label_hash -> Vec<slot>
/// (multiple slots for hash collisions or duplicate labels).

use std::collections::HashMap;

pub struct HashIndex {
    /// label_hash -> list of node slots with that hash
    map: HashMap<u64, Vec<u64>>,
}

impl HashIndex {
    pub fn new() -> Self {
        HashIndex {
            map: HashMap::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        HashIndex {
            map: HashMap::with_capacity(cap),
        }
    }

    /// Insert a node slot for a given label hash.
    pub fn insert(&mut self, label_hash: u64, slot: u64) {
        self.map.entry(label_hash).or_default().push(slot);
    }

    /// Remove a node slot for a given label hash.
    pub fn remove(&mut self, label_hash: u64, slot: u64) {
        if let Some(slots) = self.map.get_mut(&label_hash) {
            slots.retain(|&s| s != slot);
            if slots.is_empty() {
                self.map.remove(&label_hash);
            }
        }
    }

    /// Get all slots matching a label hash.
    pub fn get(&self, label_hash: u64) -> &[u64] {
        self.map.get(&label_hash).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn len(&self) -> usize {
        self.map.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut idx = HashIndex::new();
        idx.insert(100, 0);
        idx.insert(200, 1);
        idx.insert(100, 2); // same hash, different slot

        assert_eq!(idx.get(100), &[0, 2]);
        assert_eq!(idx.get(200), &[1]);
        assert_eq!(idx.get(999), &[] as &[u64]);
    }

    #[test]
    fn remove_works() {
        let mut idx = HashIndex::new();
        idx.insert(100, 0);
        idx.insert(100, 1);
        idx.remove(100, 0);
        assert_eq!(idx.get(100), &[1]);
    }

    #[test]
    fn remove_last_cleans_up() {
        let mut idx = HashIndex::new();
        idx.insert(100, 0);
        idx.remove(100, 0);
        assert_eq!(idx.get(100), &[] as &[u64]);
        assert_eq!(idx.len(), 0);
    }
}
