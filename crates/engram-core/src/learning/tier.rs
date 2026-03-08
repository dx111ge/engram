/// Memory tier management — core/active/archival promotion/demotion.
///
/// Tiers determine how knowledge is prioritized:
/// - **Core (0)**: Always included in LLM context. High-confidence, frequently used facts.
/// - **Active (1)**: Recently relevant. Default tier for new knowledge.
/// - **Archival (2)**: Search-only. Old, low-confidence, or rarely accessed facts.
///
/// Promotion/demotion rules:
/// - High confidence + high access → promote to core
/// - Low confidence or long unaccessed → demote to archival
/// - Explicit user override always wins

#[allow(unused_imports)]
use crate::storage::node::{TIER_ACTIVE, TIER_ARCHIVAL, TIER_CORE};

/// Thresholds for automatic tier transitions.
pub const PROMOTE_TO_CORE_CONFIDENCE: f32 = 0.90;
pub const PROMOTE_TO_CORE_ACCESS_COUNT: u32 = 10;
pub const DEMOTE_TO_ARCHIVAL_CONFIDENCE: f32 = 0.20;

/// Number of days without access before demotion to archival.
pub const DEMOTE_INACTIVE_DAYS: f64 = 90.0;
const NANOS_PER_DAY: i64 = 86_400_000_000_000;

/// Determine the recommended tier for a node based on its stats.
pub fn recommended_tier(
    confidence: f32,
    access_count: u32,
    last_accessed: i64,
    now: i64,
    current_tier: u8,
) -> u8 {
    // Core promotion: high confidence AND frequently accessed
    if confidence >= PROMOTE_TO_CORE_CONFIDENCE
        && access_count >= PROMOTE_TO_CORE_ACCESS_COUNT
    {
        return TIER_CORE;
    }

    // Archival demotion: low confidence OR very long unaccessed
    if confidence < DEMOTE_TO_ARCHIVAL_CONFIDENCE {
        return TIER_ARCHIVAL;
    }

    let days_inactive = (now.saturating_sub(last_accessed).max(0) as f64)
        / (NANOS_PER_DAY as f64);
    if days_inactive > DEMOTE_INACTIVE_DAYS && current_tier != TIER_CORE {
        return TIER_ARCHIVAL;
    }

    // Otherwise maintain current tier (default is active)
    current_tier
}

/// Result of a tier sweep operation.
#[derive(Debug, Default)]
pub struct TierSweepResult {
    /// Nodes promoted to core.
    pub promoted_to_core: u32,
    /// Nodes demoted to archival.
    pub demoted_to_archival: u32,
    /// Total nodes evaluated.
    pub evaluated: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000_000_000;

    #[test]
    fn high_confidence_high_access_promotes() {
        let tier = recommended_tier(0.95, 15, DAY, 2 * DAY, TIER_ACTIVE);
        assert_eq!(tier, TIER_CORE);
    }

    #[test]
    fn high_confidence_low_access_stays() {
        let tier = recommended_tier(0.95, 3, DAY, 2 * DAY, TIER_ACTIVE);
        assert_eq!(tier, TIER_ACTIVE); // not enough accesses
    }

    #[test]
    fn low_confidence_demotes() {
        let tier = recommended_tier(0.15, 5, DAY, 2 * DAY, TIER_ACTIVE);
        assert_eq!(tier, TIER_ARCHIVAL);
    }

    #[test]
    fn long_inactive_demotes() {
        let tier = recommended_tier(0.50, 5, 0, 100 * DAY, TIER_ACTIVE);
        assert_eq!(tier, TIER_ARCHIVAL);
    }

    #[test]
    fn core_nodes_resist_inactive_demotion() {
        // Core nodes don't get demoted just for inactivity
        let tier = recommended_tier(0.50, 5, 0, 100 * DAY, TIER_CORE);
        assert_eq!(tier, TIER_CORE);
    }

    #[test]
    fn core_nodes_demote_on_low_confidence() {
        // But even core nodes demote if confidence drops
        let tier = recommended_tier(0.10, 5, 0, 100 * DAY, TIER_CORE);
        assert_eq!(tier, TIER_ARCHIVAL);
    }

    #[test]
    fn default_active_stays_active() {
        let tier = recommended_tier(0.80, 3, DAY, 2 * DAY, TIER_ACTIVE);
        assert_eq!(tier, TIER_ACTIVE);
    }
}
