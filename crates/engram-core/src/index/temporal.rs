/// Temporal index — sorted index for bi-temporal range queries.
///
/// Maintains sorted vectors of (timestamp, slot) pairs for both
/// created_at (ingestion time) and event_time (real-world time).
/// Uses binary search for efficient range queries.

/// A single entry in the temporal index
#[derive(Debug, Clone, Copy)]
struct TemporalEntry {
    timestamp: i64,
    slot: u64,
}

/// Temporal index supporting range queries on bi-temporal timestamps.
pub struct TemporalIndex {
    /// Sorted by created_at (ingestion time)
    by_created: Vec<TemporalEntry>,
    /// Sorted by event_time (real-world time)
    by_event: Vec<TemporalEntry>,
}

/// Which temporal dimension to query
#[derive(Debug, Clone, Copy)]
pub enum TimeAxis {
    /// When the fact was ingested into engram
    Created,
    /// When the real-world event occurred
    Event,
}

impl TemporalIndex {
    pub fn new() -> Self {
        TemporalIndex {
            by_created: Vec::new(),
            by_event: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        TemporalIndex {
            by_created: Vec::with_capacity(cap),
            by_event: Vec::with_capacity(cap),
        }
    }

    /// Insert a node into both temporal indexes.
    pub fn insert(&mut self, slot: u64, created_at: i64, event_time: i64) {
        insert_sorted(&mut self.by_created, TemporalEntry { timestamp: created_at, slot });
        insert_sorted(&mut self.by_event, TemporalEntry { timestamp: event_time, slot });
    }

    /// Remove a node from both temporal indexes.
    pub fn remove(&mut self, slot: u64) {
        self.by_created.retain(|e| e.slot != slot);
        self.by_event.retain(|e| e.slot != slot);
    }

    /// Query nodes within a time range [from, to] on the given axis.
    /// Returns node slots.
    pub fn range(&self, axis: TimeAxis, from: i64, to: i64) -> Vec<u64> {
        let entries = match axis {
            TimeAxis::Created => &self.by_created,
            TimeAxis::Event => &self.by_event,
        };

        let start = entries.partition_point(|e| e.timestamp < from);
        let end = entries.partition_point(|e| e.timestamp <= to);

        entries[start..end].iter().map(|e| e.slot).collect()
    }

    /// Query nodes before a timestamp.
    pub fn before(&self, axis: TimeAxis, before: i64) -> Vec<u64> {
        let entries = match axis {
            TimeAxis::Created => &self.by_created,
            TimeAxis::Event => &self.by_event,
        };

        let end = entries.partition_point(|e| e.timestamp < before);
        entries[..end].iter().map(|e| e.slot).collect()
    }

    /// Query nodes after a timestamp.
    pub fn after(&self, axis: TimeAxis, after: i64) -> Vec<u64> {
        let entries = match axis {
            TimeAxis::Created => &self.by_created,
            TimeAxis::Event => &self.by_event,
        };

        let start = entries.partition_point(|e| e.timestamp <= after);
        entries[start..].iter().map(|e| e.slot).collect()
    }

    /// Get the most recent N nodes by the given axis.
    pub fn most_recent(&self, axis: TimeAxis, n: usize) -> Vec<u64> {
        let entries = match axis {
            TimeAxis::Created => &self.by_created,
            TimeAxis::Event => &self.by_event,
        };

        let start = entries.len().saturating_sub(n);
        entries[start..].iter().rev().map(|e| e.slot).collect()
    }

    pub fn len(&self) -> usize {
        self.by_created.len()
    }
}

fn insert_sorted(vec: &mut Vec<TemporalEntry>, entry: TemporalEntry) {
    let pos = vec.partition_point(|e| {
        e.timestamp < entry.timestamp
            || (e.timestamp == entry.timestamp && e.slot < entry.slot)
    });
    vec.insert(pos, entry);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_range() {
        let mut idx = TemporalIndex::new();
        idx.insert(0, 100, 50);
        idx.insert(1, 200, 150);
        idx.insert(2, 300, 250);
        idx.insert(3, 400, 350);

        // Range on created_at: 150..350 includes 200, 300 but not 400
        let result = idx.range(TimeAxis::Created, 150, 350);
        assert_eq!(result, vec![1, 2]);

        // Range on created_at: 150..400 includes all three
        let result = idx.range(TimeAxis::Created, 150, 400);
        assert_eq!(result, vec![1, 2, 3]);

        // Range on event_time
        let result = idx.range(TimeAxis::Event, 100, 200);
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn before_and_after() {
        let mut idx = TemporalIndex::new();
        idx.insert(0, 100, 100);
        idx.insert(1, 200, 200);
        idx.insert(2, 300, 300);

        assert_eq!(idx.before(TimeAxis::Created, 250), vec![0, 1]);
        assert_eq!(idx.after(TimeAxis::Created, 200), vec![2]);
    }

    #[test]
    fn most_recent() {
        let mut idx = TemporalIndex::new();
        idx.insert(0, 100, 100);
        idx.insert(1, 200, 200);
        idx.insert(2, 300, 300);
        idx.insert(3, 400, 400);

        let recent = idx.most_recent(TimeAxis::Created, 2);
        assert_eq!(recent, vec![3, 2]); // most recent first
    }

    #[test]
    fn remove_works() {
        let mut idx = TemporalIndex::new();
        idx.insert(0, 100, 100);
        idx.insert(1, 200, 200);
        idx.remove(0);

        let result = idx.range(TimeAxis::Created, 0, 1000);
        assert_eq!(result, vec![1]);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn empty_range() {
        let idx = TemporalIndex::new();
        let result = idx.range(TimeAxis::Created, 0, 1000);
        assert!(result.is_empty());
    }

    #[test]
    fn sorted_insertion() {
        let mut idx = TemporalIndex::new();
        // Insert out of order
        idx.insert(2, 300, 300);
        idx.insert(0, 100, 100);
        idx.insert(1, 200, 200);

        let result = idx.range(TimeAxis::Created, 0, 1000);
        assert_eq!(result, vec![0, 1, 2]); // should be sorted
    }
}
