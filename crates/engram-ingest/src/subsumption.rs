/// Query subsumption: detects when a new query is subsumed by a recent query.
///
/// A query is subsumed if:
/// 1. It's a substring of a recent query (or vice versa), OR
/// 2. It was executed within a configurable time window
///
/// This prevents redundant fetches when queries overlap.

use crate::ledger::SearchLedger;

/// Subsumption checker configuration.
#[derive(Debug, Clone)]
pub struct SubsumptionConfig {
    /// Time window in seconds: queries within this window are checked for overlap.
    pub window_secs: i64,
    /// Minimum query length to consider for substring check.
    pub min_query_len: usize,
}

impl Default for SubsumptionConfig {
    fn default() -> Self {
        Self {
            window_secs: 3600, // 1 hour
            min_query_len: 3,
        }
    }
}

/// Check if a query is subsumed by existing ledger entries.
///
/// Returns `Some(subsuming_query)` if the new query is covered by an existing
/// entry, or `None` if it should be executed.
pub fn check_subsumption(
    ledger: &SearchLedger,
    source: &str,
    query: &str,
    config: &SubsumptionConfig,
) -> Option<String> {
    if query.len() < config.min_query_len {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let cutoff = now - config.window_secs;
    let query_lower = query.to_lowercase();

    for entry in ledger.entries_for_source(source) {
        if entry.timestamp < cutoff {
            continue;
        }

        let entry_lower = entry.query.to_lowercase();

        // New query is a substring of an existing query (broader query already ran)
        if entry_lower.contains(&query_lower) {
            return Some(entry.query.clone());
        }

        // Existing query is a substring of new query (new query is broader,
        // but we still have recent partial results)
        if query_lower.contains(&entry_lower) && entry.item_count > 0 {
            return Some(entry.query.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ledger() -> (tempfile::TempDir, SearchLedger) {
        let dir = tempfile::TempDir::new().unwrap();
        let brain = dir.path().join("test.brain");
        let ledger = SearchLedger::open(&brain);
        (dir, ledger)
    }

    #[test]
    fn no_subsumption_for_empty_ledger() {
        let (_dir, ledger) = make_ledger();
        let config = SubsumptionConfig::default();
        assert!(check_subsumption(&ledger, "src", "Apple Inc", &config).is_none());
    }

    #[test]
    fn substring_subsumption_exact() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig::default();

        ledger.record("src", "Apple Inc CEO", 1, None, 5);

        // "Apple" is substring of "Apple Inc CEO"
        let result = check_subsumption(&ledger, "src", "Apple", &config);
        assert_eq!(result, Some("Apple Inc CEO".to_string()));
    }

    #[test]
    fn broader_query_subsumes_narrower() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig::default();

        ledger.record("src", "Apple", 1, None, 3);

        // "Apple Inc CEO" contains "Apple" (existing narrower query had results)
        let result = check_subsumption(&ledger, "src", "Apple Inc CEO", &config);
        assert_eq!(result, Some("Apple".to_string()));
    }

    #[test]
    fn case_insensitive_subsumption() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig::default();

        ledger.record("src", "apple inc", 1, None, 5);
        let result = check_subsumption(&ledger, "src", "APPLE", &config);
        assert_eq!(result, Some("apple inc".to_string()));
    }

    #[test]
    fn different_source_no_subsumption() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig::default();

        ledger.record("reuters", "Apple", 1, None, 5);
        assert!(check_subsumption(&ledger, "bbc", "Apple", &config).is_none());
    }

    #[test]
    fn short_query_skipped() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig { min_query_len: 5, ..Default::default() };

        ledger.record("src", "Apple Inc", 1, None, 5);
        // "App" is too short
        assert!(check_subsumption(&ledger, "src", "App", &config).is_none());
    }

    #[test]
    fn no_subsumption_for_zero_results() {
        let (_dir, mut ledger) = make_ledger();
        let config = SubsumptionConfig::default();

        // Existing query had zero results
        ledger.record("src", "Apple", 1, None, 0);

        // Broader query should NOT be subsumed by zero-result narrower query
        let result = check_subsumption(&ledger, "src", "Apple Inc", &config);
        assert!(result.is_none());
    }
}
