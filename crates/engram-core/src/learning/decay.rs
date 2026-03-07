/// Knowledge decay — time-based confidence reduction.
///
/// Unaccessed, unconfirmed knowledge fades over time.
/// Mimics human forgetting: use it or lose it.
///
/// Formula: confidence *= DAILY_DECAY_FACTOR ^ days_since_last_access
/// At 0.999/day, a fact loses ~3% per month, ~30% per year.
///
/// Below DECAY_THRESHOLD (0.10), nodes are candidates for garbage collection.

/// Daily decay multiplier (applied per day since last access).
pub const DAILY_DECAY_FACTOR: f64 = 0.999;

/// Below this confidence, a node is a candidate for archival/GC.
pub const DECAY_THRESHOLD: f32 = 0.10;

/// Nanoseconds per day.
const NANOS_PER_DAY: f64 = 86_400_000_000_000.0;

/// Calculate decayed confidence based on time elapsed since last access.
///
/// `current_confidence` — the node's current confidence
/// `last_accessed_nanos` — unix nanos of last access
/// `now_nanos` — current unix nanos
///
/// Returns the new confidence after decay.
pub fn apply_decay(current_confidence: f32, last_accessed_nanos: i64, now_nanos: i64) -> f32 {
    if current_confidence <= 0.0 {
        return 0.0;
    }

    let elapsed_nanos = now_nanos.saturating_sub(last_accessed_nanos).max(0) as f64;
    let days = elapsed_nanos / NANOS_PER_DAY;

    if days < 1.0 {
        return current_confidence; // no decay within a day
    }

    let factor = DAILY_DECAY_FACTOR.powf(days) as f32;
    let decayed = current_confidence * factor;

    if decayed < DECAY_THRESHOLD {
        decayed // caller decides whether to GC
    } else {
        decayed
    }
}

/// Check if a node should be considered for archival based on decayed confidence.
pub fn should_archive(confidence: f32) -> bool {
    confidence > 0.0 && confidence < DECAY_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000_000_000; // nanos

    #[test]
    fn no_decay_within_a_day() {
        let c = apply_decay(0.80, 0, DAY / 2);
        assert_eq!(c, 0.80);
    }

    #[test]
    fn slight_decay_after_one_day() {
        let c = apply_decay(0.80, 0, DAY);
        assert!(c < 0.80);
        assert!(c > 0.79); // 0.80 * 0.999 = 0.7992
    }

    #[test]
    fn significant_decay_after_one_year() {
        let c = apply_decay(0.80, 0, 365 * DAY);
        // 0.80 * 0.999^365 ≈ 0.80 * 0.694 ≈ 0.555
        assert!(c < 0.60);
        assert!(c > 0.50);
    }

    #[test]
    fn zero_stays_zero() {
        let c = apply_decay(0.0, 0, 1000 * DAY);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn should_archive_low_confidence() {
        assert!(should_archive(0.05));
        assert!(!should_archive(0.20));
        assert!(!should_archive(0.0)); // already dead, not archival candidate
    }

    #[test]
    fn decay_converges_to_zero() {
        // After ~7 years, 0.80 * 0.999^2555 ≈ 0.06
        let c = apply_decay(0.80, 0, 2555 * DAY);
        assert!(c < DECAY_THRESHOLD);
    }
}
