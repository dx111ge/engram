/// Adaptive frequency scheduler: adjusts fetch intervals based on yield.
///
/// Each source gets an interval that adapts:
/// - High yield (many new items) -> decrease interval (fetch more often)
/// - Low yield (few/no new items) -> increase interval (fetch less often)
/// - Respects min/max bounds to prevent polling storms or stale data

use std::collections::HashMap;

/// Scheduler configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SchedulerConfig {
    /// Minimum fetch interval in seconds.
    pub min_interval_secs: u64,
    /// Maximum fetch interval in seconds.
    pub max_interval_secs: u64,
    /// Default starting interval in seconds.
    pub default_interval_secs: u64,
    /// Yield threshold: items below this count = "low yield".
    pub low_yield_threshold: u32,
    /// Yield threshold: items above this count = "high yield".
    pub high_yield_threshold: u32,
    /// Multiplier applied on high yield (shrink interval).
    pub speedup_factor: f64,
    /// Multiplier applied on low yield (grow interval).
    pub slowdown_factor: f64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            min_interval_secs: 30,
            max_interval_secs: 86400, // 24 hours
            default_interval_secs: 300, // 5 minutes
            low_yield_threshold: 1,
            high_yield_threshold: 10,
            speedup_factor: 0.5,  // halve interval on high yield
            slowdown_factor: 2.0, // double interval on low yield
        }
    }
}

/// Per-source schedule state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceSchedule {
    /// Current fetch interval in seconds.
    pub interval_secs: u64,
    /// Last fetch timestamp (unix seconds).
    pub last_fetch: i64,
    /// Consecutive low-yield fetches.
    pub consecutive_low: u32,
    /// Consecutive high-yield fetches.
    pub consecutive_high: u32,
    /// Whether the source is paused.
    pub paused: bool,
}

/// Adaptive frequency scheduler.
pub struct AdaptiveScheduler {
    config: SchedulerConfig,
    schedules: HashMap<String, SourceSchedule>,
}

impl AdaptiveScheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            schedules: HashMap::new(),
        }
    }

    /// Register a source with the default interval.
    pub fn register(&mut self, source: &str) {
        self.schedules.entry(source.to_string()).or_insert(SourceSchedule {
            interval_secs: self.config.default_interval_secs,
            last_fetch: 0,
            consecutive_low: 0,
            consecutive_high: 0,
            paused: false,
        });
    }

    /// Set a specific interval for a source (overrides adaptive adjustment).
    pub fn set_interval(&mut self, source: &str, interval_secs: u64) {
        if let Some(schedule) = self.schedules.get_mut(source) {
            schedule.interval_secs = interval_secs;
        }
    }

    /// Check if a source is due for a fetch.
    pub fn is_due(&self, source: &str) -> bool {
        let schedule = match self.schedules.get(source) {
            Some(s) => s,
            None => return true, // unknown source = always due
        };

        if schedule.paused {
            return false;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        now - schedule.last_fetch >= schedule.interval_secs as i64
    }

    /// Report fetch results, adjusting the interval.
    pub fn report_yield(&mut self, source: &str, item_count: u32) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let schedule = self.schedules.entry(source.to_string()).or_insert(SourceSchedule {
            interval_secs: self.config.default_interval_secs,
            last_fetch: 0,
            consecutive_low: 0,
            consecutive_high: 0,
            paused: false,
        });

        schedule.last_fetch = now;

        if item_count <= self.config.low_yield_threshold {
            schedule.consecutive_low += 1;
            schedule.consecutive_high = 0;

            // Slow down: increase interval
            let new_interval = (schedule.interval_secs as f64 * self.config.slowdown_factor) as u64;
            schedule.interval_secs = new_interval.min(self.config.max_interval_secs);
        } else if item_count >= self.config.high_yield_threshold {
            schedule.consecutive_high += 1;
            schedule.consecutive_low = 0;

            // Speed up: decrease interval
            let new_interval = (schedule.interval_secs as f64 * self.config.speedup_factor) as u64;
            schedule.interval_secs = new_interval.max(self.config.min_interval_secs);
        } else {
            // Normal yield: reset streaks, keep interval
            schedule.consecutive_low = 0;
            schedule.consecutive_high = 0;
        }
    }

    /// Report a fetch error (treat as low yield).
    pub fn report_error(&mut self, source: &str) {
        self.report_yield(source, 0);
    }

    /// Pause a source.
    pub fn pause(&mut self, source: &str) {
        if let Some(s) = self.schedules.get_mut(source) {
            s.paused = true;
        }
    }

    /// Resume a source.
    pub fn resume(&mut self, source: &str) {
        if let Some(s) = self.schedules.get_mut(source) {
            s.paused = false;
        }
    }

    /// Get schedule for a source.
    pub fn get_schedule(&self, source: &str) -> Option<&SourceSchedule> {
        self.schedules.get(source)
    }

    /// Get all source names with their current intervals.
    pub fn list_schedules(&self) -> Vec<(String, u64, bool)> {
        self.schedules
            .iter()
            .map(|(name, s)| (name.clone(), s.interval_secs, s.paused))
            .collect()
    }

    /// Get all schedules (for serialization).
    pub fn schedules(&self) -> &HashMap<String, SourceSchedule> {
        &self.schedules
    }

    /// Get the config.
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    /// Save scheduler state (config + schedules) to a JSON file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let data = serde_json::json!({
            "config": self.config,
            "schedules": self.schedules,
        });
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load scheduler state from a JSON file. Returns default if file doesn't exist.
    pub fn load(path: &std::path::Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                #[derive(serde::Deserialize)]
                struct SavedState {
                    config: SchedulerConfig,
                    schedules: HashMap<String, SourceSchedule>,
                }
                match serde_json::from_str::<SavedState>(&contents) {
                    Ok(state) => Self {
                        config: state.config,
                        schedules: state.schedules,
                    },
                    Err(_) => Self::default(),
                }
            }
            Err(_) => Self::default(),
        }
    }

    /// Reset a source to the default interval.
    pub fn reset(&mut self, source: &str) {
        if let Some(s) = self.schedules.get_mut(source) {
            s.interval_secs = self.config.default_interval_secs;
            s.consecutive_low = 0;
            s.consecutive_high = 0;
        }
    }
}

impl Default for AdaptiveScheduler {
    fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_source_is_due() {
        let scheduler = AdaptiveScheduler::default();
        assert!(scheduler.is_due("unknown"));
    }

    #[test]
    fn registered_source_starts_due() {
        let mut scheduler = AdaptiveScheduler::default();
        scheduler.register("src");
        // last_fetch is 0, so it's always due
        assert!(scheduler.is_due("src"));
    }

    #[test]
    fn paused_source_not_due() {
        let mut scheduler = AdaptiveScheduler::default();
        scheduler.register("src");
        scheduler.pause("src");
        assert!(!scheduler.is_due("src"));

        scheduler.resume("src");
        assert!(scheduler.is_due("src"));
    }

    #[test]
    fn high_yield_decreases_interval() {
        let mut scheduler = AdaptiveScheduler::new(SchedulerConfig {
            default_interval_secs: 300,
            min_interval_secs: 30,
            speedup_factor: 0.5,
            high_yield_threshold: 10,
            ..Default::default()
        });

        scheduler.register("src");
        scheduler.report_yield("src", 20); // high yield

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 150); // 300 * 0.5
        assert_eq!(schedule.consecutive_high, 1);
    }

    #[test]
    fn low_yield_increases_interval() {
        let mut scheduler = AdaptiveScheduler::new(SchedulerConfig {
            default_interval_secs: 300,
            max_interval_secs: 86400,
            slowdown_factor: 2.0,
            low_yield_threshold: 1,
            ..Default::default()
        });

        scheduler.register("src");
        scheduler.report_yield("src", 0); // low yield

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 600); // 300 * 2.0
        assert_eq!(schedule.consecutive_low, 1);
    }

    #[test]
    fn interval_respects_min_bound() {
        let mut scheduler = AdaptiveScheduler::new(SchedulerConfig {
            default_interval_secs: 60,
            min_interval_secs: 30,
            speedup_factor: 0.1,
            high_yield_threshold: 5,
            ..Default::default()
        });

        scheduler.register("src");
        scheduler.report_yield("src", 100);

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 30); // clamped to min
    }

    #[test]
    fn interval_respects_max_bound() {
        let mut scheduler = AdaptiveScheduler::new(SchedulerConfig {
            default_interval_secs: 50000,
            max_interval_secs: 86400,
            slowdown_factor: 10.0,
            low_yield_threshold: 1,
            ..Default::default()
        });

        scheduler.register("src");
        scheduler.report_yield("src", 0);

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 86400); // clamped to max
    }

    #[test]
    fn normal_yield_keeps_interval() {
        let mut scheduler = AdaptiveScheduler::new(SchedulerConfig {
            default_interval_secs: 300,
            low_yield_threshold: 1,
            high_yield_threshold: 10,
            ..Default::default()
        });

        scheduler.register("src");
        scheduler.report_yield("src", 5); // normal yield

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 300);
    }

    #[test]
    fn reset_restores_default() {
        let mut scheduler = AdaptiveScheduler::default();
        scheduler.register("src");
        scheduler.report_yield("src", 0); // slow down
        scheduler.reset("src");

        let schedule = scheduler.get_schedule("src").unwrap();
        assert_eq!(schedule.interval_secs, 300);
    }
}
