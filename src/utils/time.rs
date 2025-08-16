//! Time utilities for OpenAce Rust
//!
//! Provides time measurement, formatting, and performance tracking utilities.

use crate::error::{OpenAceError, Result};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, info};
use serde::{Serialize, Deserialize};

/// High-precision timer for performance measurements
#[derive(Debug, Clone)]
pub struct PrecisionTimer {
    start: Instant,
    name: String,
    checkpoints: Vec<(String, Instant)>,
}

impl PrecisionTimer {
    /// Create a new precision timer
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        debug!("Starting precision timer: {}", name);
        
        Self {
            start: Instant::now(),
            name,
            checkpoints: Vec::new(),
        }
    }
    
    /// Add a checkpoint
    pub fn checkpoint(&mut self, name: impl Into<String>) {
        let checkpoint_name = name.into();
        let now = Instant::now();
        self.checkpoints.push((checkpoint_name.clone(), now));
        
        let elapsed = now.duration_since(self.start);
        debug!("Timer '{}' checkpoint '{}': {}ms", self.name, checkpoint_name, elapsed.as_millis());
    }
    
    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    
    /// Get elapsed time since last checkpoint
    pub fn elapsed_since_last_checkpoint(&self) -> Duration {
        if let Some((_, last_checkpoint)) = self.checkpoints.last() {
            last_checkpoint.elapsed()
        } else {
            self.elapsed()
        }
    }
    
    /// Get time between two checkpoints
    pub fn time_between_checkpoints(&self, from: &str, to: &str) -> Option<Duration> {
        let from_time = self.checkpoints.iter()
            .find(|(name, _)| name == from)
            .map(|(_, time)| *time);
        
        let to_time = self.checkpoints.iter()
            .find(|(name, _)| name == to)
            .map(|(_, time)| *time);
        
        match (from_time, to_time) {
            (Some(from), Some(to)) if to >= from => Some(to.duration_since(from)),
            _ => None,
        }
    }
    
    /// Finish the timer and return total elapsed time
    pub fn finish(self) -> Duration {
        let elapsed = self.elapsed();
        info!("Timer '{}' finished: {}ms", self.name, elapsed.as_millis());
        elapsed
    }
    
    /// Get a summary of all checkpoints
    pub fn summary(&self) -> TimerSummary {
        let mut intervals = Vec::new();
        let mut last_time = self.start;
        
        for (name, time) in &self.checkpoints {
            let interval = time.duration_since(last_time);
            intervals.push(TimerInterval {
                name: name.clone(),
                duration: interval,
                cumulative: time.duration_since(self.start),
            });
            last_time = *time;
        }
        
        TimerSummary {
            name: self.name.clone(),
            total_duration: self.elapsed(),
            intervals,
        }
    }
}

/// Timer interval information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerInterval {
    pub name: String,
    pub duration: Duration,
    pub cumulative: Duration,
}

/// Timer summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerSummary {
    pub name: String,
    pub total_duration: Duration,
    pub intervals: Vec<TimerInterval>,
}

/// Performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub operation_name: String,
    pub total_calls: u64,
    pub total_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub avg_duration: Duration,
    pub last_duration: Duration,
    pub last_call_time: SystemTime,
}

impl PerformanceStats {
    fn new(operation_name: String) -> Self {
        Self {
            operation_name,
            total_calls: 0,
            total_duration: Duration::ZERO,
            min_duration: Duration::MAX,
            max_duration: Duration::ZERO,
            avg_duration: Duration::ZERO,
            last_duration: Duration::ZERO,
            last_call_time: SystemTime::now(),
        }
    }
    
    fn update(&mut self, duration: Duration) {
        self.total_calls += 1;
        self.total_duration += duration;
        self.last_duration = duration;
        self.last_call_time = SystemTime::now();
        
        if duration < self.min_duration {
            self.min_duration = duration;
        }
        if duration > self.max_duration {
            self.max_duration = duration;
        }
        
        self.avg_duration = self.total_duration / self.total_calls as u32;
    }
}

/// Performance tracker for monitoring operation performance
#[derive(Debug)]
pub struct PerformanceTracker {
    stats: Arc<RwLock<HashMap<String, PerformanceStats>>>,
}

impl Default for PerformanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceTracker {
    /// Create a new performance tracker
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Record a performance measurement
    pub fn record(&self, operation: &str, duration: Duration) {
        let mut stats = self.stats.write();
        let entry = stats.entry(operation.to_string())
            .or_insert_with(|| PerformanceStats::new(operation.to_string()));
        
        entry.update(duration);
        
        debug!(
            "Performance: {} took {}ms (avg: {}ms, calls: {})",
            operation,
            duration.as_millis(),
            entry.avg_duration.as_millis(),
            entry.total_calls
        );
    }
    
    /// Get statistics for an operation
    pub fn get_stats(&self, operation: &str) -> Option<PerformanceStats> {
        self.stats.read().get(operation).cloned()
    }
    
    /// Get all statistics
    pub fn get_all_stats(&self) -> HashMap<String, PerformanceStats> {
        self.stats.read().clone()
    }
    
    /// Clear statistics for an operation
    pub fn clear_stats(&self, operation: &str) {
        self.stats.write().remove(operation);
    }
    
    /// Clear all statistics
    pub fn clear_all_stats(&self) {
        self.stats.write().clear();
    }
    
    /// Get operations that are slower than threshold
    pub fn get_slow_operations(&self, threshold: Duration) -> Vec<PerformanceStats> {
        self.stats.read()
            .values()
            .filter(|stats| stats.avg_duration > threshold)
            .cloned()
            .collect()
    }
}

/// Scoped performance measurement
pub struct ScopedTimer {
    operation: String,
    start: Instant,
    tracker: Option<Arc<PerformanceTracker>>,
}

impl ScopedTimer {
    /// Create a new scoped timer
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            start: Instant::now(),
            tracker: None,
        }
    }
    
    /// Create a new scoped timer with performance tracking
    pub fn with_tracker(operation: impl Into<String>, tracker: Arc<PerformanceTracker>) -> Self {
        Self {
            operation: operation.into(),
            start: Instant::now(),
            tracker: Some(tracker),
        }
    }
    
    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        
        if let Some(tracker) = &self.tracker {
            tracker.record(&self.operation, duration);
        } else {
            debug!("Operation '{}' took {}ms", self.operation, duration.as_millis());
        }
    }
}

/// Time formatting utilities
pub mod formatting {
    use super::*;
    
    /// Format a duration as human-readable string
    pub fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let millis = duration.subsec_millis();
        let micros = duration.subsec_micros() % 1000;
        let nanos = duration.subsec_nanos() % 1000;
        
        if total_secs >= 3600 {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            format!("{}h {}m {}s", hours, mins, secs)
        } else if total_secs >= 60 {
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            format!("{}m {}s", mins, secs)
        } else if total_secs > 0 {
            format!("{}.{:03}s", total_secs, millis)
        } else if millis > 0 {
            format!("{}.{:03}ms", millis, micros)
        } else if micros > 0 {
            format!("{}.{:03}μs", micros, nanos)
        } else {
            format!("{}ns", nanos)
        }
    }
    
    /// Format a SystemTime as ISO 8601 string
    pub fn format_system_time(time: SystemTime) -> String {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let secs = duration.as_secs();
                let nanos = duration.subsec_nanos();
                
                // Convert to chrono for proper formatting
                if let Some(datetime) = chrono::DateTime::from_timestamp(secs as i64, nanos) {
                    datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
                } else {
                    "Invalid time".to_string()
                }
            }
            Err(_) => "Invalid time".to_string(),
        }
    }
    
    /// Format elapsed time since a point in time
    pub fn format_elapsed_since(since: SystemTime) -> String {
        match since.elapsed() {
            Ok(duration) => format_duration(duration),
            Err(_) => "Invalid time".to_string(),
        }
    }
    
    /// Format a timestamp as relative time (e.g., "2 minutes ago")
    pub fn format_relative_time(time: SystemTime) -> String {
        match time.elapsed() {
            Ok(duration) => {
                let secs = duration.as_secs();
                
                if secs < 60 {
                    "just now".to_string()
                } else if secs < 3600 {
                    let mins = secs / 60;
                    if mins == 1 {
                        "1 minute ago".to_string()
                    } else {
                        format!("{} minutes ago", mins)
                    }
                } else if secs < 86400 {
                    let hours = secs / 3600;
                    if hours == 1 {
                        "1 hour ago".to_string()
                    } else {
                        format!("{} hours ago", hours)
                    }
                } else {
                    let days = secs / 86400;
                    if days == 1 {
                        "1 day ago".to_string()
                    } else {
                        format!("{} days ago", days)
                    }
                }
            }
            Err(_) => "in the future".to_string(),
        }
    }
}

/// Time conversion utilities
pub mod conversion {
    use super::*;
    
    /// Convert milliseconds to Duration
    pub fn millis_to_duration(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }
    
    /// Convert microseconds to Duration
    pub fn micros_to_duration(micros: u64) -> Duration {
        Duration::from_micros(micros)
    }
    
    /// Convert nanoseconds to Duration
    pub fn nanos_to_duration(nanos: u64) -> Duration {
        Duration::from_nanos(nanos)
    }
    
    /// Convert Duration to milliseconds
    pub fn duration_to_millis(duration: Duration) -> u64 {
        duration.as_millis() as u64
    }
    
    /// Convert Duration to microseconds
    pub fn duration_to_micros(duration: Duration) -> u64 {
        duration.as_micros() as u64
    }
    
    /// Convert Duration to nanoseconds
    pub fn duration_to_nanos(duration: Duration) -> u64 {
        duration.as_nanos() as u64
    }
    
    /// Convert SystemTime to Unix timestamp (seconds)
    pub fn system_time_to_unix_timestamp(time: SystemTime) -> Result<u64> {
        time.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .map_err(|e| OpenAceError::internal(format!("Time conversion error: {}", e)))
    }
    
    /// Convert Unix timestamp to SystemTime
    pub fn unix_timestamp_to_system_time(timestamp: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(timestamp)
    }
    
    /// Get current Unix timestamp
    pub fn current_unix_timestamp() -> Result<u64> {
        system_time_to_unix_timestamp(SystemTime::now())
    }
}

/// Rate limiting utilities
pub mod rate_limiting {
    use super::*;
    use std::collections::VecDeque;
    
    /// Simple rate limiter using sliding window
    #[derive(Debug)]
    pub struct RateLimiter {
        max_requests: usize,
        window_duration: Duration,
        requests: VecDeque<Instant>,
    }
    
    impl RateLimiter {
        /// Create a new rate limiter
        pub fn new(max_requests: usize, window_duration: Duration) -> Self {
            Self {
                max_requests,
                window_duration,
                requests: VecDeque::new(),
            }
        }
        
        /// Check if a request is allowed
        pub fn is_allowed(&mut self) -> bool {
            let now = Instant::now();
            
            // Remove old requests outside the window
            while let Some(&front) = self.requests.front() {
                if now.duration_since(front) > self.window_duration {
                    self.requests.pop_front();
                } else {
                    break;
                }
            }
            
            // Check if we can add a new request
            if self.requests.len() < self.max_requests {
                self.requests.push_back(now);
                true
            } else {
                false
            }
        }
        
        /// Get time until next request is allowed
        pub fn time_until_allowed(&self) -> Option<Duration> {
            if self.requests.len() < self.max_requests {
                return None;
            }
            
            if let Some(&oldest) = self.requests.front() {
                let elapsed = oldest.elapsed();
                if elapsed < self.window_duration {
                    Some(self.window_duration - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        }
        
        /// Get current request count in window
        pub fn current_count(&self) -> usize {
            self.requests.len()
        }
        
        /// Reset the rate limiter
        pub fn reset(&mut self) {
            self.requests.clear();
        }
    }
}
/// Global performance tracker
static GLOBAL_PERFORMANCE_TRACKER: once_cell::sync::OnceCell<Arc<PerformanceTracker>> = 
    once_cell::sync::OnceCell::new();

/// Initialize time utilities
pub fn initialize_time() -> Result<()> {
    info!("Initializing time utilities");
    
    let tracker = Arc::new(PerformanceTracker::new());
    GLOBAL_PERFORMANCE_TRACKER.set(tracker)
        .map_err(|_| OpenAceError::internal("Time utilities already initialized"))?;
    
    info!("Time utilities initialized");
    Ok(())
}

/// Shutdown time utilities
pub fn shutdown_time() -> Result<()> {
    if let Some(tracker) = GLOBAL_PERFORMANCE_TRACKER.get() {
        info!("Shutting down time utilities");
        
        // Log final performance statistics
        let stats = tracker.get_all_stats();
        for (operation, stat) in stats {
            info!(
                "Final stats for '{}': {} calls, avg: {}ms, total: {}ms",
                operation,
                stat.total_calls,
                stat.avg_duration.as_millis(),
                stat.total_duration.as_millis()
            );
        }
    }
    
    Ok(())
}

/// Get global performance tracker
pub fn get_global_performance_tracker() -> Option<&'static Arc<PerformanceTracker>> {
    GLOBAL_PERFORMANCE_TRACKER.get()
}

/// Record performance measurement globally
pub fn record_performance(operation: &str, duration: Duration) {
    if let Some(tracker) = GLOBAL_PERFORMANCE_TRACKER.get() {
        tracker.record(operation, duration);
    }
}

/// Convenience macros for timing
/// Time a block of code
#[macro_export]
macro_rules! time_block {
    ($name:expr, $block:block) => {
        {
            let _timer = $crate::utils::time::ScopedTimer::new($name);
            $block
        }
    };
}

/// Time a block of code with global tracking
#[macro_export]
macro_rules! time_block_global {
    ($name:expr, $block:block) => {
        {
            let tracker = $crate::utils::time::get_global_performance_tracker()
                .map(|t| t.clone());
            let _timer = if let Some(t) = tracker {
                $crate::utils::time::ScopedTimer::with_tracker($name, t)
            } else {
                $crate::utils::time::ScopedTimer::new($name)
            };
            $block
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    
    #[test]
    fn test_precision_timer() {
        let mut timer = PrecisionTimer::new("test");
        
        thread::sleep(Duration::from_millis(10));
        timer.checkpoint("checkpoint1");
        
        thread::sleep(Duration::from_millis(10));
        timer.checkpoint("checkpoint2");
        
        let elapsed = timer.finish();
        assert!(elapsed >= Duration::from_millis(20));
    }
    
    #[test]
    fn test_performance_tracker() {
        let tracker = PerformanceTracker::new();
        
        tracker.record("test_op", Duration::from_millis(100));
        tracker.record("test_op", Duration::from_millis(200));
        
        let stats = tracker.get_stats("test_op").unwrap();
        assert_eq!(stats.total_calls, 2);
        assert_eq!(stats.avg_duration, Duration::from_millis(150));
    }
    
    #[test]
    fn test_rate_limiter() {
        let mut limiter = rate_limiting::RateLimiter::new(2, Duration::from_secs(1));
        
        assert!(limiter.is_allowed());
        assert!(limiter.is_allowed());
        assert!(!limiter.is_allowed()); // Should be rate limited
        
        assert_eq!(limiter.current_count(), 2);
    }
    
    #[test]
    fn test_duration_formatting() {
        assert_eq!(formatting::format_duration(Duration::from_millis(1500)), "1.500s");
        assert_eq!(formatting::format_duration(Duration::from_secs(65)), "1m 5s");
        assert_eq!(formatting::format_duration(Duration::from_secs(3661)), "1h 1m 1s");
    }
}