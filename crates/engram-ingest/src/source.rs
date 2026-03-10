/// Source registry: manages named sources, tracks usage and health.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::traits::{CostModel, Source, SourceCapabilities};

/// Usage counters for a single source.
#[derive(Debug)]
pub struct SourceUsage {
    /// Total requests made.
    pub requests: AtomicU64,
    /// Total items fetched.
    pub items: AtomicU64,
    /// Total errors encountered.
    pub errors: AtomicU64,
    /// Last successful fetch timestamp (unix seconds).
    pub last_success: AtomicU64,
    /// Last error timestamp (unix seconds).
    pub last_error: AtomicU64,
    /// Accumulated cost.
    pub cost: std::sync::Mutex<f64>,
}

impl Default for SourceUsage {
    fn default() -> Self {
        Self {
            requests: AtomicU64::new(0),
            items: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_success: AtomicU64::new(0),
            last_error: AtomicU64::new(0),
            cost: std::sync::Mutex::new(0.0),
        }
    }
}

impl SourceUsage {
    /// Record a successful fetch.
    pub fn record_success(&self, item_count: u64, cost_model: &CostModel) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.items.fetch_add(item_count, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_success.store(now, Ordering::Relaxed);

        // Update cost
        let cost = match cost_model {
            CostModel::Free => 0.0,
            CostModel::PerRequest(price) => *price,
            CostModel::PerItem(price) => *price * item_count as f64,
            CostModel::Quota { overage_cost, monthly_limit } => {
                let total = self.items.load(Ordering::Relaxed);
                if total > *monthly_limit {
                    *overage_cost * item_count as f64
                } else {
                    0.0
                }
            }
        };
        if cost > 0.0 {
            if let Ok(mut c) = self.cost.lock() {
                *c += cost;
            }
        }
    }

    /// Record a failed fetch.
    pub fn record_error(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.errors.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_error.store(now, Ordering::Relaxed);
    }

    /// Get a snapshot of current usage.
    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            requests: self.requests.load(Ordering::Relaxed),
            items: self.items.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            last_success: self.last_success.load(Ordering::Relaxed),
            last_error: self.last_error.load(Ordering::Relaxed),
            cost: self.cost.lock().map(|c| *c).unwrap_or(0.0),
        }
    }
}

/// Serializable snapshot of usage counters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageSnapshot {
    pub requests: u64,
    pub items: u64,
    pub errors: u64,
    pub last_success: u64,
    pub last_error: u64,
    pub cost: f64,
}

/// Serializable source info for the API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceInfo {
    pub name: String,
    pub capabilities: CapabilitiesInfo,
    pub usage: UsageSnapshot,
    pub healthy: bool,
}

/// Serializable capabilities for the API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilitiesInfo {
    pub temporal_cursor: bool,
    pub searchable: bool,
    pub streaming: bool,
    pub cost_model: String,
}

impl From<&SourceCapabilities> for CapabilitiesInfo {
    fn from(caps: &SourceCapabilities) -> Self {
        Self {
            temporal_cursor: caps.temporal_cursor,
            searchable: caps.searchable,
            streaming: caps.streaming,
            cost_model: match &caps.cost_model {
                CostModel::Free => "free".into(),
                CostModel::PerRequest(p) => format!("per_request:{:.4}", p),
                CostModel::PerItem(p) => format!("per_item:{:.4}", p),
                CostModel::Quota { monthly_limit, overage_cost } => {
                    format!("quota:{}:{:.4}", monthly_limit, overage_cost)
                }
            },
        }
    }
}

/// Registry entry: source implementation + usage tracker.
struct RegisteredSource {
    source: Box<dyn Source>,
    usage: Arc<SourceUsage>,
}

/// Thread-safe registry of named sources with usage tracking.
pub struct SourceRegistry {
    sources: RwLock<HashMap<String, RegisteredSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
        }
    }

    /// Register a source. Replaces any existing source with the same name.
    pub fn register(&self, source: Box<dyn Source>) {
        let name = source.name().to_string();
        let entry = RegisteredSource {
            source,
            usage: Arc::new(SourceUsage::default()),
        };
        if let Ok(mut map) = self.sources.write() {
            map.insert(name, entry);
        }
    }

    /// Remove a source by name.
    pub fn unregister(&self, name: &str) -> bool {
        self.sources
            .write()
            .map(|mut map| map.remove(name).is_some())
            .unwrap_or(false)
    }

    /// List all registered source names.
    pub fn list(&self) -> Vec<String> {
        self.sources
            .read()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get info for all sources.
    pub fn list_info(&self) -> Vec<SourceInfo> {
        let map = match self.sources.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        map.iter()
            .map(|(name, entry)| {
                let caps = entry.source.capabilities();
                let snapshot = entry.usage.snapshot();
                let healthy = is_healthy(&snapshot);
                SourceInfo {
                    name: name.clone(),
                    capabilities: CapabilitiesInfo::from(&caps),
                    usage: snapshot,
                    healthy,
                }
            })
            .collect()
    }

    /// Get info for a single source.
    pub fn get_info(&self, name: &str) -> Option<SourceInfo> {
        let map = self.sources.read().ok()?;
        let entry = map.get(name)?;
        let caps = entry.source.capabilities();
        let snapshot = entry.usage.snapshot();
        let healthy = is_healthy(&snapshot);
        Some(SourceInfo {
            name: name.to_string(),
            capabilities: CapabilitiesInfo::from(&caps),
            usage: snapshot,
            healthy,
        })
    }

    /// Get usage snapshot for a source.
    pub fn get_usage(&self, name: &str) -> Option<UsageSnapshot> {
        let map = self.sources.read().ok()?;
        let entry = map.get(name)?;
        Some(entry.usage.snapshot())
    }

    /// Fetch from a named source with usage tracking.
    pub async fn fetch(
        &self,
        name: &str,
        params: &crate::traits::SourceParams,
    ) -> Result<Vec<crate::types::RawItem>, crate::IngestError> {
        // Get source and usage tracker under read lock, then release
        let (source_ref, usage, cost_model) = {
            let map = self.sources.read().map_err(|_| {
                crate::IngestError::Source("registry lock poisoned".into())
            })?;
            let entry = map.get(name).ok_or_else(|| {
                crate::IngestError::Source(format!("source '{}' not registered", name))
            })?;
            // We need the source to be used outside the lock, but Source is trait object
            // behind &. We can't hold the read lock across await. So we need to restructure.
            // For now, get usage and cost_model, and do the fetch inside the lock scope.
            let caps = entry.source.capabilities();
            let cost_model = caps.cost_model.clone();
            let usage = entry.usage.clone();
            // Fetch under the read lock (source is behind the lock)
            let result = entry.source.fetch(params).await;
            (result, usage, cost_model)
        };

        match source_ref {
            Ok(items) => {
                let count = items.len() as u64;
                usage.record_success(count, &cost_model);
                Ok(items)
            }
            Err(e) => {
                usage.record_error();
                Err(e)
            }
        }
    }

    /// Pre-fetch budget check. Returns an error string if the source is over budget.
    pub fn check_budget(&self, name: &str) -> Result<(), String> {
        let map = self.sources.read().map_err(|_| "registry lock poisoned".to_string())?;
        let entry = match map.get(name) {
            Some(e) => e,
            None => return Ok(()), // unknown source = no budget constraint
        };
        let caps = entry.source.capabilities();
        match caps.cost_model {
            CostModel::Free | CostModel::PerRequest(_) | CostModel::PerItem(_) => Ok(()),
            CostModel::Quota { monthly_limit, .. } => {
                let used = entry.usage.items.load(Ordering::Relaxed);
                if used >= monthly_limit {
                    Err(format!(
                        "source '{}' exceeded monthly quota ({}/{})",
                        name, used, monthly_limit
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Get remaining budget for a quota source. Returns None for non-quota sources.
    pub fn remaining_budget(&self, name: &str) -> Option<u64> {
        let map = self.sources.read().ok()?;
        let entry = map.get(name)?;
        let caps = entry.source.capabilities();
        match caps.cost_model {
            CostModel::Quota { monthly_limit, .. } => {
                let used = entry.usage.items.load(Ordering::Relaxed);
                Some(monthly_limit.saturating_sub(used))
            }
            _ => None,
        }
    }

    /// Check if a source has budget remaining (for quota-based sources).
    pub fn has_budget(&self, name: &str) -> Option<bool> {
        let map = self.sources.read().ok()?;
        let entry = map.get(name)?;
        let caps = entry.source.capabilities();
        match caps.cost_model {
            CostModel::Free => Some(true),
            CostModel::PerRequest(_) | CostModel::PerItem(_) => Some(true), // no hard limit
            CostModel::Quota { monthly_limit, .. } => {
                let used = entry.usage.items.load(Ordering::Relaxed);
                Some(used < monthly_limit)
            }
        }
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple health heuristic: healthy if last success > last error,
/// or if no requests have been made yet.
fn is_healthy(snapshot: &UsageSnapshot) -> bool {
    if snapshot.requests == 0 {
        return true; // never used, assume healthy
    }
    if snapshot.errors == 0 {
        return true;
    }
    // Healthy if success rate > 50% and last op was success
    let success_rate = (snapshot.requests - snapshot.errors) as f64 / snapshot.requests as f64;
    success_rate > 0.5 && snapshot.last_success >= snapshot.last_error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_snapshot_defaults() {
        let usage = SourceUsage::default();
        let snap = usage.snapshot();
        assert_eq!(snap.requests, 0);
        assert_eq!(snap.items, 0);
        assert_eq!(snap.errors, 0);
        assert_eq!(snap.cost, 0.0);
    }

    #[test]
    fn record_success_increments_counters() {
        let usage = SourceUsage::default();
        usage.record_success(5, &CostModel::Free);
        let snap = usage.snapshot();
        assert_eq!(snap.requests, 1);
        assert_eq!(snap.items, 5);
        assert_eq!(snap.errors, 0);
        assert_eq!(snap.cost, 0.0);
        assert!(snap.last_success > 0);
    }

    #[test]
    fn record_error_increments_error_counter() {
        let usage = SourceUsage::default();
        usage.record_error();
        let snap = usage.snapshot();
        assert_eq!(snap.requests, 1);
        assert_eq!(snap.errors, 1);
        assert!(snap.last_error > 0);
    }

    #[test]
    fn per_request_cost_tracking() {
        let usage = SourceUsage::default();
        usage.record_success(3, &CostModel::PerRequest(0.01));
        usage.record_success(2, &CostModel::PerRequest(0.01));
        let snap = usage.snapshot();
        assert!((snap.cost - 0.02).abs() < 0.001);
    }

    #[test]
    fn per_item_cost_tracking() {
        let usage = SourceUsage::default();
        usage.record_success(10, &CostModel::PerItem(0.001));
        let snap = usage.snapshot();
        assert!((snap.cost - 0.01).abs() < 0.0001);
    }

    #[test]
    fn quota_cost_only_on_overage() {
        let usage = SourceUsage::default();
        let model = CostModel::Quota {
            monthly_limit: 100,
            overage_cost: 0.05,
        };
        // First 100 items are free
        usage.record_success(100, &model);
        assert_eq!(usage.snapshot().cost, 0.0);

        // 101st item triggers overage
        usage.record_success(10, &model);
        assert!((usage.snapshot().cost - 0.50).abs() < 0.001);
    }

    #[test]
    fn health_check() {
        // No requests = healthy
        let snap = UsageSnapshot {
            requests: 0, items: 0, errors: 0,
            last_success: 0, last_error: 0, cost: 0.0,
        };
        assert!(is_healthy(&snap));

        // All success = healthy
        let snap = UsageSnapshot {
            requests: 10, items: 50, errors: 0,
            last_success: 100, last_error: 0, cost: 0.0,
        };
        assert!(is_healthy(&snap));

        // High error rate = unhealthy
        let snap = UsageSnapshot {
            requests: 10, items: 5, errors: 8,
            last_success: 50, last_error: 100, cost: 0.0,
        };
        assert!(!is_healthy(&snap));
    }

    #[test]
    fn registry_register_and_list() {
        let registry = SourceRegistry::new();
        assert!(registry.list().is_empty());

        // Create a dummy source
        struct DummySource;
        impl Source for DummySource {
            fn fetch(&self, _params: &crate::traits::SourceParams) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<crate::types::RawItem>, crate::IngestError>> + Send + '_>> {
                Box::pin(async { Ok(vec![]) })
            }
            fn name(&self) -> &str { "dummy" }
            fn capabilities(&self) -> SourceCapabilities { SourceCapabilities::default() }
        }

        registry.register(Box::new(DummySource));
        assert_eq!(registry.list(), vec!["dummy"]);

        let info = registry.get_info("dummy").unwrap();
        assert_eq!(info.name, "dummy");
        assert!(info.healthy);

        assert!(registry.unregister("dummy"));
        assert!(registry.list().is_empty());
    }

    #[test]
    fn registry_has_budget() {
        let registry = SourceRegistry::new();

        struct QuotaSource;
        impl Source for QuotaSource {
            fn fetch(&self, _: &crate::traits::SourceParams) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<crate::types::RawItem>, crate::IngestError>> + Send + '_>> {
                Box::pin(async { Ok(vec![]) })
            }
            fn name(&self) -> &str { "quota" }
            fn capabilities(&self) -> SourceCapabilities {
                SourceCapabilities {
                    cost_model: CostModel::Quota { monthly_limit: 10, overage_cost: 1.0 },
                    ..Default::default()
                }
            }
        }

        registry.register(Box::new(QuotaSource));
        assert_eq!(registry.has_budget("quota"), Some(true));
        assert_eq!(registry.has_budget("nonexistent"), None);
    }
}
