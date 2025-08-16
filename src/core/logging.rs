//! Central Logging Infrastructure for OpenAce-Rust
//!
//! This module provides a centralized logging system that efficiently collects,
//! processes, and analyzes logs from all engine components.

use crate::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock, Mutex};
use tracing::{info, Level};
use serde::{Serialize, Deserialize};

/// Log levels with priority ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::TRACE => LogLevel::Trace,
            Level::DEBUG => LogLevel::Debug,
            Level::INFO => LogLevel::Info,
            Level::WARN => LogLevel::Warn,
            Level::ERROR => LogLevel::Error,
        }
    }
}

/// Structured log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    pub metadata: HashMap<String, String>,
    pub correlation_id: Option<String>,
}

impl LogEntry {
    pub fn new(
        level: LogLevel,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            level,
            component: component.into(),
            message: message.into(),
            metadata: HashMap::new(),
            correlation_id: None,
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

/// Log statistics for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStatistics {
    pub total_entries: u64,
    pub entries_by_level: HashMap<LogLevel, u64>,
    pub entries_by_component: HashMap<String, u64>,
    pub error_rate: f64,
    pub warning_rate: f64,
    pub peak_log_rate: u64, // logs per second
    pub average_log_rate: f64,
    pub uptime: Duration,
    pub last_error: Option<LogEntry>,
    pub last_warning: Option<LogEntry>,
}

impl Default for LogStatistics {
    fn default() -> Self {
        Self {
            total_entries: 0,
            entries_by_level: HashMap::new(),
            entries_by_component: HashMap::new(),
            error_rate: 0.0,
            warning_rate: 0.0,
            peak_log_rate: 0,
            average_log_rate: 0.0,
            uptime: Duration::default(),
            last_error: None,
            last_warning: None,
        }
    }
}

/// Central log aggregator and analyzer
#[derive(Debug)]
pub struct LogAggregator {
    entries: Arc<RwLock<Vec<LogEntry>>>,
    statistics: Arc<RwLock<LogStatistics>>,
    sender: mpsc::UnboundedSender<LogEntry>,
    receiver: Arc<Mutex<mpsc::UnboundedReceiver<LogEntry>>>,
    start_time: Instant,
    max_entries: usize,
    min_level: LogLevel,
}

impl LogAggregator {
    /// Create a new log aggregator
    pub fn new(max_entries: usize, min_level: LogLevel) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        Self {
            entries: Arc::new(RwLock::new(Vec::with_capacity(max_entries))),
            statistics: Arc::new(RwLock::new(LogStatistics::default())),
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            start_time: Instant::now(),
            max_entries,
            min_level,
        }
    }

    /// Get a sender for logging entries
    pub fn get_sender(&self) -> mpsc::UnboundedSender<LogEntry> {
        self.sender.clone()
    }

    /// Start the log processing loop
    pub async fn start_processing(&self) {
        let entries = self.entries.clone();
        let statistics = self.statistics.clone();
        let receiver = self.receiver.clone();
        let max_entries = self.max_entries;
        let min_level = self.min_level;
        let start_time = self.start_time;

        tokio::spawn(async move {
            let mut receiver = receiver.lock().await;
            let mut log_count_per_second = HashMap::new();
            let mut last_second = 0u64;

            while let Some(entry) = receiver.recv().await {
                // Filter by minimum level
                if entry.level < min_level {
                    continue;
                }

                let current_second = entry.timestamp / 1000;
                
                // Track logs per second
                *log_count_per_second.entry(current_second).or_insert(0) += 1;
                
                // Clean old per-second data (keep last 60 seconds)
                if current_second > last_second {
                    log_count_per_second.retain(|&k, _| current_second - k <= 60);
                    last_second = current_second;
                }

                // Add to entries with rotation
                {
                    let mut entries_guard = entries.write().await;
                    entries_guard.push(entry.clone());
                    
                    // Rotate if exceeding max entries
                    if entries_guard.len() > max_entries {
                        entries_guard.remove(0);
                    }
                }

                // Update statistics
                {
                    let mut stats = statistics.write().await;
                    stats.total_entries += 1;
                    *stats.entries_by_level.entry(entry.level).or_insert(0) += 1;
                    *stats.entries_by_component.entry(entry.component.clone()).or_insert(0) += 1;
                    
                    // Update rates
                    let uptime = start_time.elapsed();
                    stats.uptime = uptime;
                    stats.average_log_rate = stats.total_entries as f64 / uptime.as_secs_f64();
                    
                    // Calculate peak log rate
                    if let Some(&max_rate) = log_count_per_second.values().max() {
                        stats.peak_log_rate = stats.peak_log_rate.max(max_rate);
                    }
                    
                    // Calculate error and warning rates
                    let error_count = *stats.entries_by_level.get(&LogLevel::Error).unwrap_or(&0);
                    let warning_count = *stats.entries_by_level.get(&LogLevel::Warn).unwrap_or(&0);
                    stats.error_rate = error_count as f64 / stats.total_entries as f64;
                    stats.warning_rate = warning_count as f64 / stats.total_entries as f64;
                    
                    // Update last error/warning
                    match entry.level {
                        LogLevel::Error => stats.last_error = Some(entry),
                        LogLevel::Warn => stats.last_warning = Some(entry),
                        _ => {}
                    }
                }
            }
        });
    }

    /// Get current statistics
    pub async fn get_statistics(&self) -> LogStatistics {
        self.statistics.read().await.clone()
    }

    /// Get recent log entries
    pub async fn get_recent_entries(&self, count: usize) -> Vec<LogEntry> {
        let entries = self.entries.read().await;
        let start = entries.len().saturating_sub(count);
        entries[start..].to_vec()
    }

    /// Get entries by level
    pub async fn get_entries_by_level(&self, level: LogLevel) -> Vec<LogEntry> {
        let entries = self.entries.read().await;
        entries.iter()
            .filter(|entry| entry.level == level)
            .cloned()
            .collect()
    }

    /// Get entries by component
    pub async fn get_entries_by_component(&self, component: &str) -> Vec<LogEntry> {
        let entries = self.entries.read().await;
        entries.iter()
            .filter(|entry| entry.component == component)
            .cloned()
            .collect()
    }

    /// Search entries by message content
    pub async fn search_entries(&self, query: &str) -> Vec<LogEntry> {
        let entries = self.entries.read().await;
        entries.iter()
            .filter(|entry| entry.message.contains(query))
            .cloned()
            .collect()
    }

    /// Get health status based on log analysis
    pub async fn get_health_status(&self) -> LogHealthStatus {
        let stats = self.statistics.read().await;
        
        let status = if stats.error_rate > 0.1 {
            LogHealthLevel::Critical
        } else if stats.error_rate > 0.05 || stats.warning_rate > 0.2 {
            LogHealthLevel::Warning
        } else {
            LogHealthLevel::Healthy
        };

        LogHealthStatus {
            level: status,
            error_rate: stats.error_rate,
            warning_rate: stats.warning_rate,
            total_entries: stats.total_entries,
            peak_log_rate: stats.peak_log_rate,
            last_error: stats.last_error.clone(),
            last_warning: stats.last_warning.clone(),
        }
    }

    /// Clear all entries and reset statistics
    pub async fn clear(&self) {
        self.entries.write().await.clear();
        *self.statistics.write().await = LogStatistics::default();
    }
}

/// Log health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogHealthStatus {
    pub level: LogHealthLevel,
    pub error_rate: f64,
    pub warning_rate: f64,
    pub total_entries: u64,
    pub peak_log_rate: u64,
    pub last_error: Option<LogEntry>,
    pub last_warning: Option<LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogHealthLevel {
    Healthy,
    Warning,
    Critical,
}

/// Global log manager singleton
static LOG_MANAGER: once_cell::sync::Lazy<Arc<RwLock<Option<Arc<LogAggregator>>>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

/// Initialize the global log manager
pub async fn initialize_logging(max_entries: usize, min_level: LogLevel) -> Result<()> {
    let aggregator = Arc::new(LogAggregator::new(max_entries, min_level));
    aggregator.start_processing().await;
    
    *LOG_MANAGER.write().await = Some(aggregator);
    
    info!("Central logging system initialized with max_entries={}, min_level={:?}", 
          max_entries, min_level);
    
    Ok(())
}

/// Get the global log manager
pub async fn get_log_manager() -> Option<Arc<LogAggregator>> {
    LOG_MANAGER.read().await.clone()
}

/// Log a message to the central aggregator
pub async fn log_to_central(
    level: LogLevel,
    component: impl Into<String>,
    message: impl Into<String>,
) {
    if let Some(manager) = get_log_manager().await {
        let entry = LogEntry::new(level, component, message);
        let _ = manager.get_sender().send(entry);
    }
}

/// Macro for easy logging to central system
#[macro_export]
macro_rules! central_log {
    ($level:expr, $component:expr, $($arg:tt)*) => {
        {
            let message = format!($($arg)*);
            tokio::spawn(async move {
                $crate::core::logging::log_to_central($level, $component, message).await;
            });
        }
    };
}

/// Convenience macros for different log levels
#[macro_export]
macro_rules! central_error {
    ($component:expr, $($arg:tt)*) => {
        central_log!($crate::core::logging::LogLevel::Error, $component, $($arg)*)
    };
}

#[macro_export]
macro_rules! central_warn {
    ($component:expr, $($arg:tt)*) => {
        central_log!($crate::core::logging::LogLevel::Warn, $component, $($arg)*)
    };
}

#[macro_export]
macro_rules! central_info {
    ($component:expr, $($arg:tt)*) => {
        central_log!($crate::core::logging::LogLevel::Info, $component, $($arg)*)
    };
}

#[macro_export]
macro_rules! central_debug {
    ($component:expr, $($arg:tt)*) => {
        central_log!($crate::core::logging::LogLevel::Debug, $component, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_log_aggregator() {
        let aggregator = LogAggregator::new(100, LogLevel::Debug);
        aggregator.start_processing().await;
        
        let sender = aggregator.get_sender();
        
        // Send test entries
        let entry1 = LogEntry::new(LogLevel::Info, "test", "Test message 1");
        let entry2 = LogEntry::new(LogLevel::Error, "test", "Test error");
        
        sender.send(entry1).unwrap();
        sender.send(entry2).unwrap();
        
        // Wait for processing
        sleep(Duration::from_millis(10)).await;
        
        let stats = aggregator.get_statistics().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.entries_by_level.get(&LogLevel::Info), Some(&1));
        assert_eq!(stats.entries_by_level.get(&LogLevel::Error), Some(&1));
        
        let recent = aggregator.get_recent_entries(10).await;
        assert_eq!(recent.len(), 2);
    }

    #[tokio::test]
    async fn test_log_filtering() {
        let aggregator = LogAggregator::new(100, LogLevel::Warn);
        aggregator.start_processing().await;
        
        let sender = aggregator.get_sender();
        
        // Send entries of different levels
        sender.send(LogEntry::new(LogLevel::Debug, "test", "Debug message")).unwrap();
        sender.send(LogEntry::new(LogLevel::Info, "test", "Info message")).unwrap();
        sender.send(LogEntry::new(LogLevel::Warn, "test", "Warning message")).unwrap();
        sender.send(LogEntry::new(LogLevel::Error, "test", "Error message")).unwrap();
        
        sleep(Duration::from_millis(10)).await;
        
        let stats = aggregator.get_statistics().await;
        // Only warn and error should be counted
        assert_eq!(stats.total_entries, 2);
    }

    #[tokio::test]
    async fn test_health_status() {
        let aggregator = LogAggregator::new(100, LogLevel::Debug);
        aggregator.start_processing().await;
        
        let sender = aggregator.get_sender();
        
        // Send mostly errors to trigger critical status
        for i in 0..10 {
            if i < 8 {
                sender.send(LogEntry::new(LogLevel::Error, "test", "Error message")).unwrap();
            } else {
                sender.send(LogEntry::new(LogLevel::Info, "test", "Info message")).unwrap();
            }
        }
        
        sleep(Duration::from_millis(10)).await;
        
        let health = aggregator.get_health_status().await;
        assert_eq!(health.level, LogHealthLevel::Critical);
        assert!(health.error_rate > 0.1);
    }
}