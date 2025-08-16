//! Logging utilities for OpenAce Rust
//!
//! Provides structured logging with performance tracing, file rotation,
//! and thread-safe operation.

use crate::error::{OpenAceError, Result};
use crate::config::LoggingConfig;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_appender::{non_blocking, rolling};
use std::io;
use lazy_static::lazy_static;
use std::sync::Mutex;

/// Global logging state
static LOGGING_STATE: once_cell::sync::OnceCell<Arc<RwLock<LoggingState>>> = 
    once_cell::sync::OnceCell::new();

/// Internal logging state
#[derive(Debug)]
struct LoggingState {
    /// Current configuration
    config: LoggingConfig,
    /// File appender guard (keeps file writer alive)
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    /// Whether logging is initialized
    initialized: bool,
}

/// Performance span for measuring operation duration
#[derive(Debug)]
pub struct PerfSpan {
    span: tracing::Span,
    start_time: std::time::Instant,
}

impl PerfSpan {
    /// Create a new performance span
    pub fn new(name: &str, target: &str) -> Self {
        let span = tracing::info_span!(
            "perf",
            operation = name,
            target = target,
            duration_ms = tracing::field::Empty,
        );
        
        Self {
            span,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Enter the span
    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }
    
    /// Record additional fields
    pub fn record<T: std::fmt::Debug>(&self, field: &str, value: T) {
        self.span.record(field, tracing::field::debug(value));
    }
    
    /// Finish the span and record duration
    pub fn finish(self) {
        let duration = self.start_time.elapsed();
        self.span.record("duration_ms", duration.as_millis() as u64);
        
        if duration.as_millis() > 1000 {
            let opname = self
                .span
                .metadata()
                .map(|m| m.name())
                .unwrap_or("unknown");
            tracing::warn!(
                "Slow operation detected: {} took {}ms",
                opname,
                duration.as_millis()
            );
        }
    }
}

/// Macro for creating performance spans
#[macro_export]
macro_rules! perf_span {
    ($name:expr) => {
        $crate::utils::logging::PerfSpan::new($name, module_path!())
    };
    ($name:expr, $target:expr) => {
        $crate::utils::logging::PerfSpan::new($name, $target)
    };
}

/// Macro for timing code blocks
#[macro_export]
macro_rules! timed {
    ($name:expr, $block:block) => {
        {
            let _span = $crate::perf_span!($name);
            let _guard = _span.enter();
            let result = $block;
            _span.finish();
            result
        }
    };
}

/// Initialize logging subsystem
pub async fn initialize_logging() -> Result<()> {
    let config = LoggingConfig::default();
    initialize_logging_with_config(config).await
}

/// Initialize logging with custom configuration
pub async fn initialize_logging_with_config(config: LoggingConfig) -> Result<()> {
    // Validate configuration
    config.validate()?;
    
    let mut file_guard = None;
    
    // Parse log level
    let level = parse_log_level(&config.level)?;
    let make_filter = || EnvFilter::from_default_env()
        .add_directive(level.into());
    
    // Console output layer (always add)
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_filter(make_filter());
    
    // File output layer
    // File output layer (sink if not configured)
    let file_writer: BoxMakeWriter = if let Some(ref path) = config.output_file {
        // Create file appender
        let file_appender = rolling::daily(
            path.parent().unwrap_or(std::path::Path::new(".")),
            path.file_name().unwrap_or_default(),
        );
        let (non_blocking, guard) = non_blocking(file_appender);
        file_guard = Some(guard);
        BoxMakeWriter::new(non_blocking)
    } else {
        BoxMakeWriter::new(io::sink)
    };

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_filter(make_filter());
    
    // Performance tracing layer (always present, off if disabled)
    let perf_layer = fmt::layer()
        .with_target(false)
        .with_level(false)
        .with_span_events(FmtSpan::NONE)
        .with_filter(if config.enable_tracing { EnvFilter::new("perf=info") } else { EnvFilter::new("off") });
    
    // Initialize subscriber with statically chained layers
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .with(perf_layer)
        .init();
    
    // Store logging state
    let state = LoggingState {
        config,
        _file_guard: file_guard,
        initialized: true,
    };
    
    LOGGING_STATE.set(Arc::new(RwLock::new(state)))
        .map_err(|_| OpenAceError::internal("Logging already initialized"))?;
    
    tracing::info!("Logging subsystem initialized");
    Ok(())
}

/// Shutdown logging subsystem
pub async fn shutdown_logging() -> Result<()> {
    if let Some(state_arc) = LOGGING_STATE.get() {
        {
            let mut state = state_arc.write();
            if state.initialized {
                tracing::info!("Shutting down logging subsystem");
                
                // Drop file guard to flush remaining logs
                state._file_guard.take();
                state.initialized = false;
            }
        }
        
        // Small delay to allow final log messages to be written
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    Ok(())
}

/// Update logging configuration at runtime
pub async fn update_logging_config(new_config: LoggingConfig) -> Result<()> {
    // Shutdown current logging
    shutdown_logging().await?;
    
    // Reinitialize with new config
    initialize_logging_with_config(new_config).await
}

/// Get current logging configuration
pub fn get_logging_config() -> Option<LoggingConfig> {
    LOGGING_STATE.get()
        .map(|state| state.read().config.clone())
}

/// Check if logging is initialized
pub fn is_logging_initialized() -> bool {
    LOGGING_STATE.get()
        .map(|state| state.read().initialized)
        .unwrap_or(false)
}

/// Parse log level from string
fn parse_log_level(level: &str) -> Result<Level> {
    match level.to_lowercase().as_str() {
        "trace" => Ok(Level::TRACE),
        "debug" => Ok(Level::DEBUG),
        "info" => Ok(Level::INFO),
        "warn" | "warning" => Ok(Level::WARN),
        "error" => Ok(Level::ERROR),
        _ => Err(OpenAceError::configuration(
            format!("Invalid log level: {}", level)
        )),
    }
}

impl LoggingConfig {
    /// Validate logging configuration
    pub fn validate(&self) -> Result<()> {
        // Validate log level
        parse_log_level(&self.level)?;
        
        // Validate file size limits
        if self.max_file_size_mb == 0 {
            return Err(OpenAceError::configuration(
                "max_file_size_mb must be greater than 0"
            ));
        }
        
        if self.max_files == 0 {
            return Err(OpenAceError::configuration(
                "max_files must be greater than 0"
            ));
        }
        
        // Validate output file path
        if let Some(ref path) = self.output_file {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| OpenAceError::configuration(
                            format!("Cannot create log directory: {}", e)
                        ))?;
                }
            }
        }
        
        Ok(())
    }
}

/// Thread-safe logger for C-style logging functions
#[derive(Debug, Clone)]
pub struct ThreadSafeLogger {
    target: String,
}

impl ThreadSafeLogger {
    /// Create a new thread-safe logger
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
        }
    }
    
    /// Log a trace message
    pub fn trace(&self, message: &str) {
        tracing::event!(Level::TRACE, log_target = %self.target, "{}", message);
    }
    
    /// Log a debug message
    pub fn debug(&self, message: &str) {
        tracing::event!(Level::DEBUG, log_target = %self.target, "{}", message);
    }
    
    /// Log an info message
    pub fn info(&self, message: &str) {
        tracing::event!(Level::INFO, log_target = %self.target, "{}", message);
    }
    
    /// Log a warning message
    pub fn warn(&self, message: &str) {
        tracing::event!(Level::WARN, log_target = %self.target, "{}", message);
    }
    
    /// Log an error message
    pub fn error(&self, message: &str) {
        tracing::event!(Level::ERROR, log_target = %self.target, "{}", message);
    }
    
    /// Log with custom level
    pub fn log(&self, level: Level, message: &str) {
        match level {
            Level::TRACE => self.trace(message),
            Level::DEBUG => self.debug(message),
            Level::INFO => self.info(message),
            Level::WARN => self.warn(message),
            Level::ERROR => self.error(message),
        }
    }
}

/// Get a thread-safe logger for a specific target
pub fn get_logger(target: &str) -> ThreadSafeLogger {
    ThreadSafeLogger::new(target)
}

/// Memory-safe alternative to C's printf-style logging
pub fn safe_log(level: Level, target: &str, message: &str) {
    let logger = get_logger(target);
    logger.log(level, message);
}

/// Log memory statistics
pub fn log_memory_stats() {
    let stats = crate::utils::memory::get_memory_stats();
    tracing::info!(
        "Memory stats: {:.2}MB current, {:.2}MB peak, {} allocs, {} deallocs",
        stats.current_usage_mb(),
        stats.peak_usage_mb(),
        stats.allocation_count,
        stats.deallocation_count
    );
    
    if stats.has_potential_leaks() {
        tracing::warn!(
            "Potential memory leaks: {} more allocations than deallocations",
            stats.allocation_count - stats.deallocation_count
        );
    }
}

/// Log performance metrics
pub fn log_performance_metrics() {
    let pool_stats = crate::utils::memory::get_pool_stats();
    
    for (pool_name, (total, in_use, available)) in pool_stats {
        tracing::info!(
            "Memory pool '{}': {} total, {} in use, {} available",
            pool_name, total, in_use, available
        );
    }
}

/// Structured logging macros
#[macro_export]
macro_rules! log_with_context {
    ($level:expr, $($field:ident = $value:expr),* ; $($arg:tt)*) => {
        tracing::event!(
            $level,
            $($field = $value,)*
            $($arg)*
        );
    };
}

#[macro_export]
macro_rules! info_with_context {
    ($($field:ident = $value:expr),* ; $($arg:tt)*) => {
        $crate::log_with_context!(tracing::Level::INFO, $($field = $value),* ; $($arg)*);
    };
}

#[macro_export]
macro_rules! warn_with_context {
    ($($field:ident = $value:expr),* ; $($arg:tt)*) => {
        $crate::log_with_context!(tracing::Level::WARN, $($field = $value),* ; $($arg)*);
    };
}

#[macro_export]
macro_rules! error_with_context {
    ($($field:ident = $value:expr),* ; $($arg:tt)*) => {
        $crate::log_with_context!(tracing::Level::ERROR, $($field = $value),* ; $($arg)*);
    };
}


/// Central log collector
pub struct LogCollector {
    logs: Vec<String>,
}

impl Default for LogCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LogCollector {
    pub fn new() -> Self {
        LogCollector { logs: Vec::new() }
    }

    pub fn add_log(&mut self, log: String) {
        self.logs.push(log);
    }

    pub fn get_logs(&self) -> Vec<String> {
        self.logs.clone()
    }
}

// Global collector
lazy_static! {
    pub static ref GLOBAL_LOG_COLLECTOR: Mutex<LogCollector> = Mutex::new(LogCollector::new());
}

// Function to get all logs
pub fn get_all_logs() -> Vec<String> {
    GLOBAL_LOG_COLLECTOR.lock().unwrap().get_logs()
}

// Integrate with tracing
// Add subscriber to collect logs