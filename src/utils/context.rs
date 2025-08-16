//! Context management for OpenAce Rust
//!
//! Provides global context management, thread-safe state tracking,
//! and lifecycle management for the entire application.

use crate::error::{OpenAceError, Result};
use crate::config::OpenAceConfig;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, error};
use serde::{Serialize, Deserialize};

/// Application lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    /// Application is not initialized
    Uninitialized,
    /// Application is initializing
    Initializing,
    /// Application is running normally
    Running,
    /// Application is shutting down
    ShuttingDown,
    /// Application has shut down
    Shutdown,
    /// Application encountered an error
    Error,
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleState::Uninitialized => write!(f, "Uninitialized"),
            LifecycleState::Initializing => write!(f, "Initializing"),
            LifecycleState::Running => write!(f, "Running"),
            LifecycleState::ShuttingDown => write!(f, "Shutting Down"),
            LifecycleState::Shutdown => write!(f, "Shutdown"),
            LifecycleState::Error => write!(f, "Error"),
        }
    }
}

/// Context statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    /// When the context was created
    pub created_at: SystemTime,
    /// When the context was last updated
    pub updated_at: SystemTime,
    /// Number of state transitions
    pub state_transitions: u64,
    /// Number of configuration updates
    pub config_updates: u64,
    /// Number of errors encountered
    pub error_count: u64,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Memory usage statistics
    pub memory_stats: crate::utils::memory::MemoryStats,
}

impl Default for ContextStats {
    fn default() -> Self {
        let now = SystemTime::now();
        Self {
            created_at: now,
            updated_at: now,
            state_transitions: 0,
            config_updates: 0,
            error_count: 0,
            uptime_seconds: 0,
            memory_stats: crate::utils::memory::MemoryStats::default(),
        }
    }
}

/// Global application context
#[derive(Debug)]
pub struct GlobalContext {
    /// Current lifecycle state
    state: LifecycleState,
    /// Application configuration
    config: OpenAceConfig,
    /// Context statistics
    stats: ContextStats,
    /// Custom properties
    properties: HashMap<String, String>,
    /// Error history
    error_history: Vec<(SystemTime, String)>,
    /// State transition history
    state_history: Vec<(SystemTime, LifecycleState)>,
    /// Initialization start time
    init_start: Option<Instant>,
    /// Shutdown start time
    shutdown_start: Option<Instant>,
}

impl GlobalContext {
    /// Create a new global context
    pub fn new(config: OpenAceConfig) -> Self {
        let now = SystemTime::now();
        let mut context = Self {
            state: LifecycleState::Uninitialized,
            config,
            stats: ContextStats::default(),
            properties: HashMap::new(),
            error_history: Vec::new(),
            state_history: Vec::new(),
            init_start: None,
            shutdown_start: None,
        };
        
        // Record initial state
        context.state_history.push((now, LifecycleState::Uninitialized));
        
        info!("Global context created");
        context
    }
    
    /// Get current lifecycle state
    pub fn state(&self) -> LifecycleState {
        self.state
    }
    
    /// Set lifecycle state
    pub fn set_state(&mut self, new_state: LifecycleState) -> Result<()> {
        let old_state = self.state;
        
        // Validate state transition
        if !self.is_valid_transition(old_state, new_state) {
            return Err(OpenAceError::invalid_state_transition(
                format!("{}", old_state),
                format!("{}", new_state),
            ));
        }
        
        self.state = new_state;
        self.stats.state_transitions += 1;
        self.stats.updated_at = SystemTime::now();
        
        // Record state transition
        self.state_history.push((self.stats.updated_at, new_state));
        
        // Handle special state transitions
        match new_state {
            LifecycleState::Initializing => {
                self.init_start = Some(Instant::now());
            }
            LifecycleState::Running => {
                if let Some(start) = self.init_start {
                    let duration = start.elapsed();
                    info!("Initialization completed in {}ms", duration.as_millis());
                    self.init_start = None;
                }
            }
            LifecycleState::ShuttingDown => {
                self.shutdown_start = Some(Instant::now());
            }
            LifecycleState::Shutdown => {
                if let Some(start) = self.shutdown_start {
                    let duration = start.elapsed();
                    info!("Shutdown completed in {}ms", duration.as_millis());
                    self.shutdown_start = None;
                }
            }
            _ => {}
        }
        
        info!("State transition: {} -> {}", old_state, new_state);
        Ok(())
    }
    
    /// Check if state transition is valid
    fn is_valid_transition(&self, from: LifecycleState, to: LifecycleState) -> bool {
        use LifecycleState::*;
        
        match (from, to) {
            // From Uninitialized
            (Uninitialized, Initializing) => true,
            (Uninitialized, Error) => true,
            
            // From Initializing
            (Initializing, Running) => true,
            (Initializing, Error) => true,
            (Initializing, ShuttingDown) => true,
            
            // From Running
            (Running, ShuttingDown) => true,
            (Running, Error) => true,
            
            // From ShuttingDown
            (ShuttingDown, Shutdown) => true,
            (ShuttingDown, Error) => true,
            
            // From Error
            (Error, Initializing) => true,
            (Error, ShuttingDown) => true,
            (Error, Shutdown) => true,
            
            // From Shutdown
            (Shutdown, Initializing) => true,
            
            // Invalid transitions
            _ => false,
        }
    }
    
    /// Get application configuration
    pub fn config(&self) -> &OpenAceConfig {
        &self.config
    }
    
    /// Update application configuration
    pub fn update_config(&mut self, config: OpenAceConfig) {
        self.config = config;
        self.stats.config_updates += 1;
        self.stats.updated_at = SystemTime::now();
        info!("Configuration updated");
    }
    
    /// Get context statistics
    pub fn stats(&self) -> &ContextStats {
        &self.stats
    }
    
    /// Update statistics
    pub fn update_stats(&mut self) {
        self.stats.updated_at = SystemTime::now();
        
        // Calculate uptime
        if let Ok(duration) = self.stats.updated_at.duration_since(self.stats.created_at) {
            self.stats.uptime_seconds = duration.as_secs();
        }
        
        // Update memory stats
        self.stats.memory_stats = crate::utils::memory::get_memory_stats();
    }
    
    /// Set a custom property
    pub fn set_property(&mut self, key: String, value: String) {
        self.properties.insert(key.clone(), value.clone());
        debug!("Property set: {} = {}", key, value);
    }
    
    /// Get a custom property
    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(|s| s.as_str())
    }
    
    /// Remove a custom property
    pub fn remove_property(&mut self, key: &str) -> Option<String> {
        let result = self.properties.remove(key);
        if result.is_some() {
            debug!("Property removed: {}", key);
        }
        result
    }
    
    /// Get all properties
    pub fn properties(&self) -> &HashMap<String, String> {
        &self.properties
    }
    
    /// Record an error
    pub fn record_error(&mut self, error: String) {
        let now = SystemTime::now();
        self.error_history.push((now, error.clone()));
        self.stats.error_count += 1;
        self.stats.updated_at = now;
        
        // Keep only last 100 errors
        if self.error_history.len() > 100 {
            self.error_history.remove(0);
        }
        
        error!("Error recorded: {}", error);
    }
    
    /// Get error history
    pub fn error_history(&self) -> &[(SystemTime, String)] {
        &self.error_history
    }
    
    /// Get state history
    pub fn state_history(&self) -> &[(SystemTime, LifecycleState)] {
        &self.state_history
    }
    
    /// Check if context is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self.state, LifecycleState::Running | LifecycleState::Initializing)
    }
    
    /// Get uptime duration
    pub fn uptime(&self) -> Duration {
        self.stats.created_at.elapsed().unwrap_or(Duration::ZERO)
    }
    
    /// Get initialization duration (if completed)
    pub fn initialization_duration(&self) -> Option<Duration> {
        if self.state == LifecycleState::Running {
            // Find the time when we transitioned to Running
            for (time, state) in self.state_history.iter().rev() {
                if *state == LifecycleState::Running {
                    return time.duration_since(self.stats.created_at).ok();
                }
            }
        }
        None
    }
    
    /// Export context as JSON
    pub fn to_json(&self) -> Result<String> {
        #[derive(Serialize)]
        struct ContextExport {
            state: LifecycleState,
            stats: ContextStats,
            properties: HashMap<String, String>,
            error_count: usize,
            state_transition_count: usize,
        }
        
        let export = ContextExport {
            state: self.state,
            stats: self.stats.clone(),
            properties: self.properties.clone(),
            error_count: self.error_history.len(),
            state_transition_count: self.state_history.len(),
        };
        
        serde_json::to_string_pretty(&export)
            .map_err(|e| OpenAceError::serialization(format!("Context export failed: {}", e)))
    }
}

/// Thread-safe global context wrapper
#[derive(Debug, Clone)]
pub struct SafeGlobalContext {
    inner: Arc<RwLock<GlobalContext>>,
}

impl SafeGlobalContext {
    /// Create a new safe global context
    pub fn new(config: OpenAceConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(GlobalContext::new(config))),
        }
    }
    
    /// Get current state
    pub fn state(&self) -> LifecycleState {
        self.inner.read().state()
    }
    
    /// Set state
    pub fn set_state(&self, state: LifecycleState) -> Result<()> {
        self.inner.write().set_state(state)
    }
    
    /// Get configuration
    pub fn config(&self) -> OpenAceConfig {
        self.inner.read().config().clone()
    }
    
    /// Update configuration
    pub fn update_config(&self, config: OpenAceConfig) {
        self.inner.write().update_config(config);
    }
    
    /// Get statistics
    pub fn stats(&self) -> ContextStats {
        self.inner.read().stats().clone()
    }
    
    /// Update statistics
    pub fn update_stats(&self) {
        self.inner.write().update_stats();
    }
    
    /// Set property
    pub fn set_property(&self, key: String, value: String) {
        self.inner.write().set_property(key, value);
    }
    
    /// Get property
    pub fn get_property(&self, key: &str) -> Option<String> {
        self.inner.read().get_property(key).map(|s| s.to_string())
    }
    
    /// Remove property
    pub fn remove_property(&self, key: &str) -> Option<String> {
        self.inner.write().remove_property(key)
    }
    
    /// Record error
    pub fn record_error(&self, error: String) {
        self.inner.write().record_error(error);
    }
    
    /// Check if healthy
    pub fn is_healthy(&self) -> bool {
        self.inner.read().is_healthy()
    }
    
    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.inner.read().uptime()
    }
    
    /// Export to JSON
    pub fn to_json(&self) -> Result<String> {
        self.inner.read().to_json()
    }
    
    /// Execute a function with read access to the context
    pub fn with_read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&GlobalContext) -> R,
    {
        f(&self.inner.read())
    }
    
    /// Execute a function with write access to the context
    pub fn with_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut GlobalContext) -> R,
    {
        f(&mut self.inner.write())
    }
}

/// Global context singleton
static GLOBAL_CONTEXT: once_cell::sync::OnceCell<SafeGlobalContext> = 
    once_cell::sync::OnceCell::new();

/// Initialize global context
pub fn initialize_context(config: OpenAceConfig) -> Result<()> {
    info!("Initializing global context");
    
    let context = SafeGlobalContext::new(config);
    
    GLOBAL_CONTEXT.set(context)
        .map_err(|_| OpenAceError::context_initialization("Global context already initialized".to_string()))?;
    
    // Set initial state to initializing
    if let Some(ctx) = GLOBAL_CONTEXT.get() {
        ctx.set_state(LifecycleState::Initializing)?;
    }
    
    info!("Global context initialized");
    Ok(())
}

/// Get global context
pub fn get_global_context() -> Option<&'static SafeGlobalContext> {
    GLOBAL_CONTEXT.get()
}

/// Get global context or panic
pub fn global_context() -> &'static SafeGlobalContext {
    GLOBAL_CONTEXT.get().expect("Global context not initialized")
}

/// Shutdown global context
pub fn shutdown_context() -> Result<()> {
    if let Some(context) = GLOBAL_CONTEXT.get() {
        info!("Shutting down global context");
        context.set_state(LifecycleState::ShuttingDown)?;
        
        // Update final stats
        context.update_stats();
        
        // Log final statistics
        let stats = context.stats();
        info!(
            "Final context stats - Uptime: {}s, State transitions: {}, Errors: {}",
            stats.uptime_seconds,
            stats.state_transitions,
            stats.error_count
        );
        
        context.set_state(LifecycleState::Shutdown)?;
    }
    
    Ok(())
}

/// Convenience functions for common operations
/// Set global state
pub fn set_global_state(state: LifecycleState) -> Result<()> {
    global_context().set_state(state)
}

/// Get global state
pub fn get_global_state() -> LifecycleState {
    global_context().state()
}

/// Record global error
pub fn record_global_error(error: impl Into<String>) {
    global_context().record_error(error.into());
}

/// Check if application is healthy
pub fn is_application_healthy() -> bool {
    global_context().is_healthy()
}

/// Get application uptime
pub fn get_application_uptime() -> Duration {
    global_context().uptime()
}

/// Set global property
pub fn set_global_property(key: impl Into<String>, value: impl Into<String>) {
    global_context().set_property(key.into(), value.into());
}

/// Get global property
pub fn get_global_property(key: &str) -> Option<String> {
    global_context().get_property(key)
}

/// Update global statistics
pub fn update_global_stats() {
    global_context().update_stats();
}