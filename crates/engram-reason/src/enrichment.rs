/// 3-tier enrichment dispatcher: mesh > free external > paid external.
///
/// When a query's local results are insufficient, the enrichment system
/// fans out to progressively more expensive sources:
///   Tier 1: Mesh peers (free, fast, pre-verified)
///   Tier 2: Free external APIs (GDELT, RSS, SearXNG)
///   Tier 3: Paid external APIs (Brave Search, OpenAI)
///
/// Short-circuits when sufficient results are found. Cooldown prevents
/// duplicate queries within a time window.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Enrichment mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnrichmentMode {
    /// Return local results immediately, enrich in background.
    Eager,
    /// Block response until enrichment completes.
    Await,
    /// No enrichment, local only.
    None,
}

impl Default for EnrichmentMode {
    fn default() -> Self {
        Self::None
    }
}

/// Enrichment configuration.
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Master switch.
    pub enabled: bool,
    /// Default mode when not specified per-request.
    pub default_mode: EnrichmentMode,
    /// Cooldown in seconds (same query returns cached within window).
    pub cooldown_secs: u64,
    /// Maximum concurrent source fan-outs.
    pub max_concurrent: usize,
    /// Tier ordering.
    pub tier_order: Vec<EnrichmentTier>,
    /// Minimum result count to consider "sufficient".
    pub sufficiency_threshold: usize,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_mode: EnrichmentMode::None,
            cooldown_secs: 1800, // 30 minutes
            max_concurrent: 5,
            tier_order: vec![
                EnrichmentTier::Mesh,
                EnrichmentTier::FreeExternal,
                EnrichmentTier::PaidExternal,
            ],
            sufficiency_threshold: 5,
        }
    }
}

/// Enrichment tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentTier {
    /// Mesh peer federation (free).
    Mesh,
    /// Free external APIs.
    FreeExternal,
    /// Paid external APIs.
    PaidExternal,
}

/// Result from a single enrichment source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentResult {
    /// Source name (e.g., "mesh-peers", "gdelt", "brave-search").
    pub source: String,
    /// Tier this source belongs to.
    pub tier: EnrichmentTier,
    /// Number of new facts found.
    pub new_facts: u32,
    /// Elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Whether this source succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Progress event emitted during enrichment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum EnrichmentEvent {
    /// Local results returned.
    Local { result_count: u32, avg_confidence: f32 },
    /// Enrichment started, listing sources being queried.
    Enriching { sources: Vec<String> },
    /// A single source completed.
    Enriched(EnrichmentResult),
    /// All enrichment complete.
    Complete { total_results: u32, new_facts_stored: u32, conflicts: u32 },
}

/// Dispatcher that manages enrichment requests with cooldown.
pub struct EnrichmentDispatcher {
    config: EnrichmentConfig,
    /// Query -> last enrichment time (for cooldown).
    cooldown_cache: HashMap<String, Instant>,
}

impl EnrichmentDispatcher {
    pub fn new(config: EnrichmentConfig) -> Self {
        Self {
            config,
            cooldown_cache: HashMap::new(),
        }
    }

    /// Check if a query is within cooldown window.
    pub fn is_cooled_down(&self, query: &str) -> bool {
        if let Some(last) = self.cooldown_cache.get(query) {
            last.elapsed().as_secs() < self.config.cooldown_secs
        } else {
            false
        }
    }

    /// Mark a query as enriched (start cooldown).
    pub fn mark_enriched(&mut self, query: &str) {
        self.cooldown_cache.insert(query.to_string(), Instant::now());
    }

    /// Determine the effective enrichment mode for a request.
    pub fn effective_mode(&self, requested: Option<&EnrichmentMode>) -> EnrichmentMode {
        if !self.config.enabled {
            return EnrichmentMode::None;
        }
        requested.cloned().unwrap_or_else(|| self.config.default_mode.clone())
    }

    /// Get the tier order for enrichment.
    pub fn tier_order(&self) -> &[EnrichmentTier] {
        &self.config.tier_order
    }

    /// Check if current result count meets sufficiency threshold.
    pub fn is_sufficient(&self, result_count: usize) -> bool {
        result_count >= self.config.sufficiency_threshold
    }

    /// Clean up expired cooldown entries.
    pub fn cleanup_cooldown(&mut self) {
        let threshold = self.config.cooldown_secs;
        self.cooldown_cache.retain(|_, last| last.elapsed().as_secs() < threshold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_tracking() {
        let config = EnrichmentConfig {
            cooldown_secs: 3600,
            ..Default::default()
        };
        let mut dispatcher = EnrichmentDispatcher::new(config);

        assert!(!dispatcher.is_cooled_down("test query"));
        dispatcher.mark_enriched("test query");
        assert!(dispatcher.is_cooled_down("test query"));
    }

    #[test]
    fn sufficiency_check() {
        let config = EnrichmentConfig {
            sufficiency_threshold: 5,
            ..Default::default()
        };
        let dispatcher = EnrichmentDispatcher::new(config);

        assert!(!dispatcher.is_sufficient(3));
        assert!(dispatcher.is_sufficient(5));
        assert!(dispatcher.is_sufficient(10));
    }

    #[test]
    fn effective_mode_disabled() {
        let config = EnrichmentConfig {
            enabled: false,
            default_mode: EnrichmentMode::Eager,
            ..Default::default()
        };
        let dispatcher = EnrichmentDispatcher::new(config);
        assert_eq!(dispatcher.effective_mode(None), EnrichmentMode::None);
    }

    #[test]
    fn effective_mode_override() {
        let config = EnrichmentConfig {
            enabled: true,
            default_mode: EnrichmentMode::Eager,
            ..Default::default()
        };
        let dispatcher = EnrichmentDispatcher::new(config);
        assert_eq!(
            dispatcher.effective_mode(Some(&EnrichmentMode::Await)),
            EnrichmentMode::Await
        );
    }

    #[test]
    fn tier_order_default() {
        let config = EnrichmentConfig::default();
        let dispatcher = EnrichmentDispatcher::new(config);
        assert_eq!(dispatcher.tier_order(), &[
            EnrichmentTier::Mesh,
            EnrichmentTier::FreeExternal,
            EnrichmentTier::PaidExternal,
        ]);
    }
}
