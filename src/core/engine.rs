//! Core engine implementation for OpenAce Rust
//!
//! This module implements the core engine that manages the fundamental
//! operations and state of the OpenAce system.

use crate::error::{OpenAceError, Result};
use crate::config::{CoreConfig, OpenAceConfig};
use crate::utils::{
    memory::{MemoryStats, MemoryPool},
    threading::{SafeRwLock, AtomicFlag, AtomicCounter},
    LifecycleState, ContextStats,
};
use crate::core::traits::Lifecycle;
use std::sync::Arc;
use tracing::{info, debug, instrument};
use tokio::sync::Notify;
use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

/// Engine state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineState {
    /// Engine is not initialized
    Uninitialized,
    /// Engine is initializing
    Initializing,
    /// Engine is running normally
    Running,
    /// Engine is pausing
    Pausing,
    /// Engine is paused
    Paused,
    /// Engine is stopping
    Stopping,
    /// Engine is stopped
    Stopped,
    /// Engine encountered an error
    Error,
}

impl Default for EngineState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

/// Engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub state: EngineState,
    pub uptime: Duration,
    pub operations_count: u64,
    pub errors_count: u64,
    pub memory_usage: MemoryStats,
    #[serde(skip)]
    pub last_operation_time: Option<Instant>,
    pub performance_metrics: HashMap<String, f64>,
}

impl Default for EngineStats {
    fn default() -> Self {
        Self {
            state: EngineState::default(),
            uptime: Duration::ZERO,
            operations_count: 0,
            errors_count: 0,
            memory_usage: MemoryStats::default(),
            last_operation_time: None,
            performance_metrics: HashMap::new(),
        }
    }
}

/// Core engine implementation
#[derive(Debug)]
pub struct CoreEngine {
    /// Engine configuration
    config: CoreConfig,
    /// Current engine state
    state: Arc<SafeRwLock<EngineState>>,
    /// Engine statistics
    stats: Arc<SafeRwLock<EngineStats>>,
    /// Memory pool for efficient allocations
    memory_pool: Arc<MemoryPool>,
    /// Shutdown notification
    shutdown_notify: Arc<Notify>,
    /// Engine start time
    start_time: Instant,
    /// Operation counter
    operation_counter: Arc<AtomicCounter>,
    /// Error counter
    error_counter: Arc<AtomicCounter>,
    /// Running flag
    running_flag: Arc<AtomicFlag>,
    /// Current lifecycle state
    lifecycle_state: SafeRwLock<LifecycleState>,
    /// Application configuration
    #[allow(dead_code)]
    app_config: SafeRwLock<OpenAceConfig>,
    /// Context statistics
    #[allow(dead_code)]
    context_stats: SafeRwLock<ContextStats>,
    /// Custom properties
    #[allow(dead_code)]
    properties: SafeRwLock<HashMap<String, String>>,
    /// Error history
    #[allow(dead_code)]
    error_history: SafeRwLock<Vec<(SystemTime, String)>>,
    /// State transition history
    state_history: SafeRwLock<Vec<(SystemTime, LifecycleState)>>,
    /// Initialization start time
    #[allow(dead_code)]
    init_start: SafeRwLock<Option<Instant>>,
    /// Shutdown start time
    #[allow(dead_code)]
    shutdown_start: SafeRwLock<Option<Instant>>
}

impl CoreEngine {
    /// Create a new core engine instance
    pub fn new(config: CoreConfig) -> Result<Self> {
        info!("Creating new CoreEngine with config: {:?}", config);
        
        let memory_pool = Arc::new(MemoryPool::new(
            "core_engine_memory_pool".to_string(),
            1024 * 1024, // 1MB block size
            config.memory_pool_size_mb,
        )?);
        
        let engine = Self {
            config,
            state: Arc::new(SafeRwLock::new(EngineState::Uninitialized, "core_engine_state")),
            stats: Arc::new(SafeRwLock::new(EngineStats::default(), "core_engine_stats")),
            memory_pool,
            shutdown_notify: Arc::new(Notify::new()),
            start_time: Instant::now(),
            operation_counter: Arc::new(AtomicCounter::new(0)),
            error_counter: Arc::new(AtomicCounter::new(0)),
            running_flag: Arc::new(AtomicFlag::new(false)),
            lifecycle_state: SafeRwLock::new(LifecycleState::Uninitialized, "lifecycle_state"),
            app_config: SafeRwLock::new(OpenAceConfig::default(), "app_config"),
            context_stats: SafeRwLock::new(ContextStats::default(), "context_stats"),
            properties: SafeRwLock::new(HashMap::new(), "properties"),
            error_history: SafeRwLock::new(Vec::new(), "error_history"),
            state_history: SafeRwLock::new(Vec::new(), "state_history"),
            init_start: SafeRwLock::new(None, "init_start"),
            shutdown_start: SafeRwLock::new(None, "shutdown_start"),
        };
        
        debug!("CoreEngine created successfully");
        Ok(engine)
    }
    
    // Remove get_global_context method
    pub async fn set_state(&self, new_state: LifecycleState) -> Result<()> {
        let mut state = self.lifecycle_state.write().await;
        let old_state = *state;
        *state = new_state;
        // Add to state_history
        let mut history = self.state_history.write().await;
        history.push((SystemTime::now(), new_state));
        // Log or notify as needed
        info!("Lifecycle state changed from {:?} to {:?}", old_state, new_state);
        Ok(())
    }
    
    /// Initialize the core engine
    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing CoreEngine");
        
        // Set state to initializing
        {
            let mut state = self.state.write().await;
            if *state != EngineState::Uninitialized {
                return Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Uninitialized"
                ));
            }
            *state = EngineState::Initializing;
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.state = EngineState::Initializing;
        }
        
        // Initialize memory pool
        // Note: MemoryPool is already thread-safe and doesn't need explicit initialization
        debug!("Memory pool is ready for use");
        
        // Perform core initialization tasks
        self.perform_initialization().await?;
        
        // Set state to running
        {
            let mut state = self.state.write().await;
            *state = EngineState::Running;
        }
        
        // Update stats and flags
        {
            let mut stats = self.stats.write().await;
            stats.state = EngineState::Running;
        }
        self.running_flag.set();
        
        info!("CoreEngine initialized successfully");
        Ok(())
    }
    
    /// Shutdown the core engine
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down CoreEngine");
        
        // Set state to stopping
        {
            let mut state = self.state.write().await;
            if *state == EngineState::Stopped {
                debug!("CoreEngine already stopped");
                return Ok(());
            }
            *state = EngineState::Stopping;
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.state = EngineState::Stopping;
        }
        
        // Clear running flag
        self.running_flag.clear();
        
        // Notify shutdown
        self.shutdown_notify.notify_waiters();
        
        // Perform cleanup
        self.perform_cleanup().await?;
        
        // Cleanup memory pool
        // Note: MemoryPool is already thread-safe and doesn't need explicit shutdown
        debug!("Memory pool cleanup completed");
        
        // Set final state
        {
            let mut state = self.state.write().await;
            *state = EngineState::Stopped;
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.state = EngineState::Stopped;
            stats.uptime = self.start_time.elapsed();
        }
        
        info!("CoreEngine shutdown complete");
        Ok(())
    }
    
    /// Pause the engine
    #[instrument(skip(self))]
    pub async fn pause(&self) -> Result<()> {
        info!("Pausing CoreEngine");
        
        let mut state = self.state.write().await;
        match *state {
            EngineState::Running => {
                *state = EngineState::Pausing;
                drop(state);
                
                // Perform pause operations
                self.perform_pause().await?;
                
                let mut state = self.state.write().await;
                *state = EngineState::Paused;
                
                let mut stats = self.stats.write().await;
                stats.state = EngineState::Paused;
                
                info!("CoreEngine paused");
                Ok(())
            }
            _ => Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Running"
            ))
        }
    }
    
    /// Resume the engine
    #[instrument(skip(self))]
    pub async fn resume(&self) -> Result<()> {
        info!("Resuming CoreEngine");
        
        let mut state = self.state.write().await;
        match *state {
            EngineState::Paused => {
                *state = EngineState::Running;
                drop(state);
                
                // Perform resume operations
                self.perform_resume().await?;
                
                let mut stats = self.stats.write().await;
                stats.state = EngineState::Running;
                
                info!("CoreEngine resumed");
                Ok(())
            }
            _ => Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Running"
            ))
        }
    }
    
    /// Get current engine state
    pub async fn get_state(&self) -> EngineState {
        *self.state.read().await
    }
    
    /// Get engine statistics
    pub async fn get_stats(&self) -> EngineStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime = self.start_time.elapsed();
        stats.operations_count = self.operation_counter.get().max(0) as u64;
        stats.errors_count = self.error_counter.get().max(0) as u64;
        stats.memory_usage = crate::utils::memory::get_memory_stats();
        stats
    }
    
    /// Check if engine is running
    pub fn is_running(&self) -> bool {
        self.running_flag.is_set()
    }
    
    /// Wait for shutdown notification
    pub async fn wait_for_shutdown(&self) {
        self.shutdown_notify.notified().await;
    }
    
    /// Record an operation
    pub fn record_operation(&self) {
        self.operation_counter.increment();
        
        // Update last operation time in stats
        if let Ok(mut stats) = self.stats.try_write() {
            stats.last_operation_time = Some(Instant::now());
        }
    }
    
    /// Record an error
    pub fn record_error(&self) {
        self.error_counter.increment();
    }
    
    /// Get memory pool reference
    pub fn get_memory_pool(&self) -> &Arc<MemoryPool> {
        &self.memory_pool
    }
    
    /// Get configuration
    pub fn get_config(&self) -> &CoreConfig {
        &self.config
    }
    
    /// Update performance metric
    pub async fn update_performance_metric(&self, name: String, value: f64) {
        if let Ok(mut stats) = self.stats.try_write() {
            stats.performance_metrics.insert(name, value);
        }
    }
    
    /// Perform core initialization tasks
    async fn perform_initialization(&self) -> Result<()> {
        debug!("Performing core initialization tasks");
        
        // Initialize core subsystems
        // This is where we would initialize any core-specific functionality
        
        // Simulate some initialization work
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        debug!("Core initialization tasks completed");
        Ok(())
    }
    
    /// Perform cleanup tasks
    async fn perform_cleanup(&self) -> Result<()> {
        debug!("Performing core cleanup tasks");
        
        // Cleanup core subsystems
        // This is where we would cleanup any core-specific functionality
        
        debug!("Core cleanup tasks completed");
        Ok(())
    }
    
    /// Perform pause operations
    async fn perform_pause(&self) -> Result<()> {
        debug!("Performing core pause operations");
        
        // Pause core operations
        // This is where we would pause any ongoing core operations
        
        debug!("Core pause operations completed");
        Ok(())
    }
    
    /// Perform resume operations
    async fn perform_resume(&self) -> Result<()> {
        debug!("Performing core resume operations");
        
        // Resume core operations
        // This is where we would resume any paused core operations
        
        debug!("Core resume operations completed");
        Ok(())
    }
}

/// Engine trait for common engine operations
#[async_trait]
pub trait Engine: Send + Sync {
    /// Initialize the engine
    async fn initialize(&self) -> Result<()>;
    
    /// Shutdown the engine
    async fn shutdown(&self) -> Result<()>;
    
    /// Pause the engine
    async fn pause(&self) -> Result<()>;
    
    /// Resume the engine
    async fn resume(&self) -> Result<()>;
    
    /// Get current engine state
    async fn get_state(&self) -> EngineState;
    
    /// Check if engine is running
    fn is_running(&self) -> bool;
}

#[async_trait]
impl Engine for CoreEngine {
    async fn initialize(&self) -> Result<()> {
        CoreEngine::initialize(self).await
    }
    
    async fn shutdown(&self) -> Result<()> {
        CoreEngine::shutdown(self).await
    }
    
    async fn pause(&self) -> Result<()> {
        CoreEngine::pause(self).await
    }
    
    async fn resume(&self) -> Result<()> {
        CoreEngine::resume(self).await
    }
    
    async fn get_state(&self) -> EngineState {
        CoreEngine::get_state(self).await
    }
    
    fn is_running(&self) -> bool {
        CoreEngine::is_running(self)
    }
}

// Implement Lifecycle trait for Arc<CoreEngine> to support tests
#[async_trait]
impl Lifecycle for Arc<CoreEngine> {
    async fn initialize(&mut self) -> Result<()> {
        self.as_ref().initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        self.as_ref().shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        // Check if the engine is in an initialized state
        if let Ok(state) = self.lifecycle_state.try_read() {
            !matches!(*state, crate::utils::LifecycleState::Uninitialized | crate::utils::LifecycleState::Error)
        } else {
            false
        }
    }
}

/// Core engine factory
pub struct CoreEngineFactory;

impl CoreEngineFactory {
    /// Create a new core engine instance
    pub fn create(config: CoreConfig) -> Result<Arc<CoreEngine>> {
        let engine = CoreEngine::new(config)?;
        Ok(Arc::new(engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;
    
    async fn create_test_engine() -> Arc<CoreEngine> {
        let config = CoreConfig::default();
        CoreEngineFactory::create(config).unwrap()
    }
    
    #[tokio::test]
    async fn test_engine_lifecycle() {
        let mut engine = create_test_engine().await;
        
        // Test initial state
        assert_eq!(engine.get_state().await, EngineState::Uninitialized);
        assert!(!engine.is_running());
        
        // Test initialization
        engine.initialize().await.unwrap();
        assert_eq!(engine.get_state().await, EngineState::Running);
        assert!(engine.is_running());
        
        // Test pause/resume
        engine.pause().await.unwrap();
        assert_eq!(engine.get_state().await, EngineState::Paused);
        
        engine.resume().await.unwrap();
        assert_eq!(engine.get_state().await, EngineState::Running);
        
        // Test shutdown
        engine.shutdown().await.unwrap();
        assert_eq!(engine.get_state().await, EngineState::Stopped);
        assert!(!engine.is_running());
    }
    
    #[tokio::test]
    async fn test_engine_stats() {
        let mut engine = create_test_engine().await;
        engine.initialize().await.unwrap();
        
        // Record some operations
        engine.record_operation();
        engine.record_operation();
        engine.record_error();
        
        let stats = engine.get_stats().await;
        assert_eq!(stats.operations_count, 2);
        assert_eq!(stats.errors_count, 1);
        assert_eq!(stats.state, EngineState::Running);
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let mut engine = create_test_engine().await;
        
        // Try to pause uninitialized engine
        assert!(engine.pause().await.is_err());
        
        // Try to resume uninitialized engine
        assert!(engine.resume().await.is_err());
        
        // Initialize and test invalid transitions
        engine.initialize().await.unwrap();
        
        // Try to initialize again
        assert!(engine.initialize().await.is_err());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_shutdown_notification() {
        let mut engine = create_test_engine().await;
        engine.initialize().await.unwrap();
        
        let engine_clone = engine.clone();
        let shutdown_task = tokio::spawn(async move {
            engine_clone.wait_for_shutdown().await;
        });
        
        // Give the task time to start waiting
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Shutdown the engine
        engine.shutdown().await.unwrap();
        
        // The shutdown task should complete
        timeout(Duration::from_secs(1), shutdown_task).await.unwrap().unwrap();
    }
}