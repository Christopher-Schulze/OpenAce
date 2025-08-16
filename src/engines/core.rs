//! Core Engine
//!
//! The Core Engine provides fundamental services and utilities that all other engines depend on.
//! It manages shared resources, provides common functionality, and ensures system-wide coordination.

use crate::config::CoreConfig;
use crate::error::{OpenAceError, Result};
use async_trait::async_trait;
use crate::core::traits::{Configurable, Lifecycle, Pausable, StatisticsProvider, Monitorable};
use crate::engines::EngineHealth;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, sleep};

/// Core engine state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreState {
    /// Engine is not initialized
    Uninitialized,
    /// Engine is initializing
    Initializing,
    /// Engine is ready to operate
    Ready,
    /// Engine is running
    Running,
    /// Engine is paused
    Paused,
    /// Engine is shutting down
    Shutdown,
    /// Engine has encountered an error
    Error(String),
}

/// Core engine events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEvent {
    /// Engine state changed
    StateChanged {
        old_state: CoreState,
        new_state: CoreState,
        timestamp: u64,
    },
    /// Resource allocated
    ResourceAllocated {
        resource_id: String,
        resource_type: String,
        size: u64,
        timestamp: u64,
    },
    /// Resource deallocated
    ResourceDeallocated {
        resource_id: String,
        resource_type: String,
        timestamp: u64,
    },
    /// System health check completed
    HealthCheckCompleted {
        status: HealthStatus,
        timestamp: u64,
    },
    /// Performance metrics updated
    MetricsUpdated {
        cpu_usage: f64,
        memory_usage: u64,
        timestamp: u64,
    },
}

/// Health status of the core engine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// System is healthy
    Healthy,
    /// System has warnings
    Warning(String),
    /// System is critical
    Critical(String),
    /// System is unhealthy
    Unhealthy(String),
}

/// Core engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreStats {
    /// Engine uptime in seconds
    pub uptime_seconds: u64,
    /// Total operations performed
    pub total_operations: u64,
    /// Failed operations count
    pub failed_operations: u64,
    /// Current memory usage in bytes
    pub memory_usage_bytes: u64,
    /// Peak memory usage in bytes
    pub peak_memory_usage_bytes: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Active resources count
    pub active_resources: u64,
    /// Total allocated resources
    pub total_allocated_resources: u64,
    /// Average operation duration in milliseconds
    pub avg_operation_duration_ms: f64,
    /// Last health check status
    pub last_health_status: HealthStatus,
    /// Last health check timestamp
    pub last_health_check: u64,
}

impl Default for CoreStats {
    fn default() -> Self {
        Self {
            uptime_seconds: 0,
            total_operations: 0,
            failed_operations: 0,
            memory_usage_bytes: 0,
            peak_memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            active_resources: 0,
            total_allocated_resources: 0,
            avg_operation_duration_ms: 0.0,
            last_health_status: HealthStatus::Healthy,
            last_health_check: 0,
        }
    }
}

/// Resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub id: String,
    pub resource_type: String,
    pub size: u64,
    pub allocated_at: u64,
    pub last_accessed: u64,
    pub access_count: u64,
}

/// Core Engine implementation
#[derive(Debug)]
pub struct CoreEngine {
    /// Engine configuration
    config: Arc<RwLock<CoreConfig>>,
    /// Current engine state
    state: Arc<RwLock<CoreState>>,
    /// Engine statistics
    stats: Arc<RwLock<CoreStats>>,
    /// Event broadcaster
    event_sender: broadcast::Sender<CoreEvent>,
    /// Shutdown signal sender
    shutdown_sender: Option<mpsc::Sender<()>>,
    /// Engine start time
    start_time: Arc<RwLock<Option<Instant>>>,
    /// Active resources
    resources: Arc<RwLock<HashMap<String, ResourceInfo>>>,
    /// Operation durations for averaging
    operation_durations: Arc<RwLock<Vec<Duration>>>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Operation counter
    operation_counter: Arc<AtomicU64>,
    /// Error counter
    error_counter: Arc<AtomicU64>,
}

impl CoreEngine {
    /// Create a new Core Engine
    pub fn new(config: CoreConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            config: Arc::new(RwLock::new(config)),
            state: Arc::new(RwLock::new(CoreState::Uninitialized)),
            stats: Arc::new(RwLock::new(CoreStats::default())),
            event_sender,
            shutdown_sender: None,
            start_time: Arc::new(RwLock::new(None)),
            resources: Arc::new(RwLock::new(HashMap::new())),
            operation_durations: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            operation_counter: Arc::new(AtomicU64::new(0)),
            error_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get current engine state
    pub async fn get_state(&self) -> CoreState {
        self.state.read().await.clone()
    }

    /// Set engine state and emit event
    async fn set_state(&self, new_state: CoreState) -> Result<()> {
        let old_state = {
            let mut state = self.state.write().await;
            let old = state.clone();
            *state = new_state.clone();
            old
        };

        if old_state != new_state {
            let event = CoreEvent::StateChanged {
                old_state,
                new_state,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            
            if let Err(e) = self.event_sender.send(event) {
                warn!("Failed to send state change event: {}", e);
            }
        }

        Ok(())
    }

    /// Allocate a resource
    pub async fn allocate_resource(
        &self,
        resource_id: String,
        resource_type: String,
        size: u64,
    ) -> Result<()> {
        let resource = ResourceInfo {
            id: resource_id.clone(),
            resource_type: resource_type.clone(),
            size,
            allocated_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            last_accessed: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            access_count: 0,
        };

        {
            let mut resources = self.resources.write().await;
            resources.insert(resource_id.clone(), resource);
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.active_resources += 1;
            stats.total_allocated_resources += 1;
            stats.memory_usage_bytes += size;
            if stats.memory_usage_bytes > stats.peak_memory_usage_bytes {
                stats.peak_memory_usage_bytes = stats.memory_usage_bytes;
            }
        }

        // Send event
        let event = CoreEvent::ResourceAllocated {
            resource_id,
            resource_type,
            size,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        
        if let Err(e) = self.event_sender.send(event) {
            warn!("Failed to send resource allocation event: {}", e);
        }

        Ok(())
    }

    /// Deallocate a resource
    pub async fn deallocate_resource(&self, resource_id: &str) -> Result<()> {
        let resource = {
            let mut resources = self.resources.write().await;
            resources.remove(resource_id)
        };

        if let Some(resource) = resource {
            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.active_resources = stats.active_resources.saturating_sub(1);
                stats.memory_usage_bytes = stats.memory_usage_bytes.saturating_sub(resource.size);
            }

            // Send event
            let event = CoreEvent::ResourceDeallocated {
                resource_id: resource_id.to_string(),
                resource_type: resource.resource_type,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            
            if let Err(e) = self.event_sender.send(event) {
                warn!("Failed to send resource deallocation event: {}", e);
            }
        }

        Ok(())
    }

    /// Access a resource (updates access statistics)
    pub async fn access_resource(&self, resource_id: &str) -> Result<()> {
        let mut resources = self.resources.write().await;
        if let Some(resource) = resources.get_mut(resource_id) {
            resource.last_accessed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            resource.access_count += 1;
        }
        Ok(())
    }

    /// Get resource information
    pub async fn get_resource(&self, resource_id: &str) -> Option<ResourceInfo> {
        let resources = self.resources.read().await;
        resources.get(resource_id).cloned()
    }

    /// Get all resources
    pub async fn get_all_resources(&self) -> HashMap<String, ResourceInfo> {
        self.resources.read().await.clone()
    }

    /// Record operation duration
    pub async fn record_operation(&self, duration: Duration) {
        self.operation_counter.fetch_add(1, Ordering::Relaxed);
        
        let mut durations = self.operation_durations.write().await;
        durations.push(duration);
        
        // Keep only last 1000 durations for averaging
        if durations.len() > 1000 {
            durations.remove(0);
        }
        
        // Update average
        let avg_ms = durations.iter().map(|d| d.as_millis() as f64).sum::<f64>() / durations.len() as f64;
        
        let mut stats = self.stats.write().await;
        stats.total_operations = self.operation_counter.load(Ordering::Relaxed);
        stats.avg_operation_duration_ms = avg_ms;
    }

    /// Record operation error
    pub async fn record_error(&self) {
        self.error_counter.fetch_add(1, Ordering::Relaxed);
        
        let mut stats = self.stats.write().await;
        stats.failed_operations = self.error_counter.load(Ordering::Relaxed);
    }

    /// Perform health check
    pub async fn health_check(&self) -> HealthStatus {
        let stats = self.stats.read().await;
        // Check memory usage
        let memory_threshold = 1024 * 1024 * 1024; // 1GB threshold
        if stats.memory_usage_bytes > memory_threshold {
            return HealthStatus::Warning("High memory usage detected".to_string());
        }
        
        // Check error rate
        let error_rate = if stats.total_operations > 0 {
            stats.failed_operations as f64 / stats.total_operations as f64
        } else {
            0.0
        };
        
        if error_rate > 0.1 {
            return HealthStatus::Critical(format!("High error rate: {:.2}%", error_rate * 100.0));
        } else if error_rate > 0.05 {
            return HealthStatus::Warning(format!("Elevated error rate: {:.2}%", error_rate * 100.0));
        }
        
        // Check if engine is running
        if !self.running.load(Ordering::Relaxed) {
            return HealthStatus::Critical("Engine is not running".to_string());
        }
        
        HealthStatus::Healthy
    }

    /// Subscribe to core events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<CoreEvent> {
        self.event_sender.subscribe()
    }

    /// Send a core event
    pub async fn send_event(&self, event: CoreEvent) -> Result<()> {
        self.event_sender.send(event)
            .map_err(|e| OpenAceError::internal(format!("Failed to send event: {}", e)))?;
        Ok(())
    }

    /// Get engine uptime
    pub async fn get_uptime(&self) -> Duration {
        if let Some(start_time) = *self.start_time.read().await {
            start_time.elapsed()
        } else {
            Duration::from_secs(0)
        }
    }

    /// Update statistics
    async fn update_statistics(&self) {
        let uptime = self.get_uptime().await;
        let health_status = self.health_check().await;
        
        {
            let mut stats = self.stats.write().await;
            stats.uptime_seconds = uptime.as_secs();
            stats.last_health_status = health_status.clone();
            stats.last_health_check = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
        }
        
        // Send metrics event
        let event = CoreEvent::HealthCheckCompleted {
            status: health_status,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        
        if let Err(e) = self.event_sender.send(event) {
            warn!("Failed to send health check event: {}", e);
        }
    }

    /// Monitoring loop
    async fn monitoring_loop(self: Arc<Self>) {
        let mut interval = interval(Duration::from_secs(30)); // Update every 30 seconds
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.update_statistics().await;
                }
                _ = async {
                    // Check if we should stop
                    if !self.running.load(Ordering::Relaxed) {
                        return;
                    }
                    // Sleep for a short time to prevent busy waiting
                    sleep(Duration::from_millis(100)).await;
                } => {
                    break;
                }
            }
        }
        
        debug!("Core engine monitoring loop stopped");
    }

    /// Clone for async tasks
    pub fn clone_for_task(&self) -> Arc<Self> {
        Arc::new(Self {
            config: Arc::clone(&self.config),
            state: Arc::clone(&self.state),
            stats: Arc::clone(&self.stats),
            event_sender: self.event_sender.clone(),
            shutdown_sender: self.shutdown_sender.clone(),
            start_time: Arc::clone(&self.start_time),
            resources: Arc::clone(&self.resources),
            operation_durations: Arc::clone(&self.operation_durations),
            running: Arc::clone(&self.running),
            operation_counter: Arc::clone(&self.operation_counter),
            error_counter: Arc::clone(&self.error_counter),
        })
    }
}

#[async_trait::async_trait]
impl Lifecycle for CoreEngine {
    async fn initialize(&mut self) -> Result<()> {
        info!("Initializing Core Engine");
        
        self.set_state(CoreState::Initializing).await?;
        
        // Set start time
        {
            let mut start_time = self.start_time.write().await;
            *start_time = Some(Instant::now());
        }
        
        // Create shutdown channel
        let (shutdown_sender, mut shutdown_receiver) = mpsc::channel::<()>(1);
        self.shutdown_sender = Some(shutdown_sender);
        
        // Set running flag
        self.running.store(true, Ordering::Relaxed);
        
        // Start monitoring loop
        let engine_clone = self.clone_for_task();
        tokio::spawn(async move {
            engine_clone.monitoring_loop().await;
        });
        
        // Start shutdown listener
        let engine_clone = self.clone_for_task();
        tokio::spawn(async move {
            if shutdown_receiver.recv().await.is_some() {
                info!("Core engine received shutdown signal");
                engine_clone.running.store(false, Ordering::Relaxed);
            }
        });
        
        self.set_state(CoreState::Ready).await?;
        info!("Core Engine initialized successfully");
        
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down Core Engine");
        
        self.set_state(CoreState::Shutdown).await?;
        
        // Stop running
        self.running.store(false, Ordering::Relaxed);
        
        // Send shutdown signal
        if let Some(sender) = &self.shutdown_sender {
            if let Err(e) = sender.send(()).await {
                warn!("Failed to send shutdown signal: {}", e);
            }
        }
        
        // Clear all resources
        {
            let mut resources = self.resources.write().await;
            resources.clear();
        }
        
        // Reset statistics
        {
            let mut stats = self.stats.write().await;
            stats.active_resources = 0;
            stats.memory_usage_bytes = 0;
        }
        
        info!("Core Engine shut down successfully");
        Ok(())
    }



    fn is_initialized(&self) -> bool {
        if let Ok(state) = self.state.try_read() {
            matches!(*state, CoreState::Ready | CoreState::Running | CoreState::Paused)
        } else {
            false
        }
    }
}

#[async_trait::async_trait]
impl Pausable for CoreEngine {
    async fn pause(&mut self) -> Result<()> {
        info!("Pausing Core Engine");
        
        let current_state = self.get_state().await;
        match current_state {
            CoreState::Ready | CoreState::Running => {
                self.set_state(CoreState::Paused).await?;
                info!("Core Engine paused");
                Ok(())
            }
            _ => Err(OpenAceError::InvalidStateTransition {
                from: format!("{:?}", current_state),
                to: "Paused".to_string(),
                context: Box::new(crate::error::ErrorContext::new("CoreEngine", "pause")
                    .with_severity(crate::error::ErrorSeverity::Medium)
                    .with_recovery_suggestion("Ensure engine is in Running state before attempting to pause")),
                recovery_strategy: Box::new(crate::error::RecoveryStrategy::Reset { 
                    component: "CoreEngine".to_string() 
                }),
            }),
        }
    }

    async fn resume(&mut self) -> Result<()> {
        info!("Resuming Core Engine");
        
        let current_state = self.get_state().await;
        match current_state {
            CoreState::Paused => {
                self.set_state(CoreState::Ready).await?;
                info!("Core Engine resumed");
                Ok(())
            }
            _ => Err(OpenAceError::InvalidStateTransition {
                from: format!("{:?}", current_state),
                to: "Ready".to_string(),
                context: Box::new(crate::error::ErrorContext::new("CoreEngine", "resume")
                    .with_severity(crate::error::ErrorSeverity::Medium)
                    .with_recovery_suggestion("Ensure engine is in Paused state before attempting to resume")),
                recovery_strategy: Box::new(crate::error::RecoveryStrategy::Reset { 
                    component: "CoreEngine".to_string() 
                }),
            }),
        }
    }

    async fn is_paused(&self) -> bool {
        matches!(self.get_state().await, CoreState::Paused)
    }
}

#[async_trait]
impl StatisticsProvider for CoreEngine {
    type Stats = CoreStats;

    async fn get_statistics(&self) -> <Self as StatisticsProvider>::Stats {
        self.stats.read().await.clone()
    }

    async fn reset_statistics(&mut self) -> Result<()> {
        info!("Resetting Core Engine statistics");
        
        self.operation_counter.store(0, Ordering::Relaxed);
        self.error_counter.store(0, Ordering::Relaxed);
        
        {
            let mut stats = self.stats.write().await;
            *stats = CoreStats::default();
        }
        
        {
            let mut durations = self.operation_durations.write().await;
            durations.clear();
        }
        
        info!("Core Engine statistics reset");
        Ok(())
    }
}

#[async_trait::async_trait]
impl Configurable for CoreEngine {
    type Config = CoreConfig;

    async fn get_config(&self) -> <Self as Configurable>::Config {
        self.config.read().await.clone()
    }

    async fn update_config(&mut self, config: &<Self as Configurable>::Config) -> Result<()> {
        info!("Updating Core Engine configuration");
        
        // Validate configuration
        if config.max_concurrent_operations == 0 {
            return Err(OpenAceError::configuration(
                "max_concurrent_operations must be greater than 0".to_string(),
            ));
        }
        
        if config.default_timeout_ms == 0 {
            return Err(OpenAceError::configuration(
            "default_timeout_ms must be greater than 0".to_string(),
        ));
        }
        
        {
            let mut current_config = self.config.write().await;
            *current_config = config.clone();
        }
        
        info!("Core Engine configuration updated");
        Ok(())
    }

    async fn validate_config(config: &<Self as Configurable>::Config) -> Result<()> {
        if config.max_concurrent_operations == 0 {
            return Err(OpenAceError::configuration(
                "max_concurrent_operations must be greater than 0".to_string(),
            ));
        }
        
        if config.default_timeout_ms == 0 {
            return Err(OpenAceError::configuration(
            "default_timeout_ms must be greater than 0".to_string(),
        ));
        }
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl Monitorable for CoreEngine {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        // Synchronous version that checks basic state
        if let Ok(state) = self.state.try_read() {
            match *state {
                CoreState::Running => {
                    if let Ok(stats) = self.stats.try_read() {
                        if stats.failed_operations > 100 {
                            EngineHealth::Error(format!("High failure rate: {} failed operations", stats.failed_operations))
                        } else if stats.failed_operations > 10 {
                            EngineHealth::Warning(format!("Elevated failure rate: {} failed operations", stats.failed_operations))
                        } else {
                            EngineHealth::Healthy
                        }
                    } else {
                        EngineHealth::Warning("Cannot read statistics".to_string())
                    }
                },
                CoreState::Ready => EngineHealth::Healthy,
                CoreState::Paused => EngineHealth::Warning("Engine is paused".to_string()),
                CoreState::Error(ref msg) => EngineHealth::Error(msg.clone()),
                CoreState::Shutdown => EngineHealth::Error("Engine is shutdown".to_string()),
                _ => EngineHealth::Warning("Engine in transitional state".to_string()),
            }
        } else {
            EngineHealth::Error("Failed to read engine state".to_string())
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        Ok(self.get_health().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    fn create_test_config() -> CoreConfig {
        CoreConfig {
            max_concurrent_operations: 100,
            default_timeout_ms: 5000,
            debug_mode: false,
            memory_pool_size_mb: 256,
        }
    }

    #[tokio::test]
    async fn test_core_engine_creation() {
        let config = create_test_config();
        let engine = CoreEngine::new(config);
        
        assert_eq!(engine.get_state().await, CoreState::Uninitialized);
        assert!(!engine.running.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_core_engine_lifecycle() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        
        // Test initialization
        assert!(engine.initialize().await.is_ok());
        assert_eq!(engine.get_state().await, CoreState::Ready);
        assert!(engine.running.load(Ordering::Relaxed));
        
        // Test shutdown
        assert!(engine.shutdown().await.is_ok());
        assert_eq!(engine.get_state().await, CoreState::Shutdown);
        assert!(!engine.running.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_core_engine_pause_resume() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        
        assert!(engine.initialize().await.is_ok());
        
        // Test pause
        assert!(engine.pause().await.is_ok());
        assert_eq!(engine.get_state().await, CoreState::Paused);
        assert!(engine.is_paused().await);
        
        // Test resume
        assert!(engine.resume().await.is_ok());
        assert_eq!(engine.get_state().await, CoreState::Ready);
        assert!(!engine.is_paused().await);
    }

    #[tokio::test]
    async fn test_resource_management() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        assert!(engine.initialize().await.is_ok());
        
        // Test resource allocation
        assert!(engine.allocate_resource(
            "test_resource".to_string(),
            "memory".to_string(),
            1024
        ).await.is_ok());
        
        // Test resource access
        assert!(engine.get_resource("test_resource").await.is_some());
        assert!(engine.access_resource("test_resource").await.is_ok());
        
        // Test resource deallocation
        assert!(engine.deallocate_resource("test_resource").await.is_ok());
        assert!(engine.get_resource("test_resource").await.is_none());
    }

    #[tokio::test]
    async fn test_operation_recording() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        assert!(engine.initialize().await.is_ok());
        
        // Record some operations
        engine.record_operation(Duration::from_millis(100)).await;
        engine.record_operation(Duration::from_millis(200)).await;
        engine.record_error().await;
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_operations, 2);
        assert_eq!(stats.failed_operations, 1);
        assert!(stats.avg_operation_duration_ms > 0.0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        assert!(engine.initialize().await.is_ok());
        
        let health = engine.health_check().await;
        assert_eq!(health, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        
        let mut receiver = engine.subscribe_to_events();
        
        // Initialize engine to trigger state change event
        assert!(engine.initialize().await.is_ok());
        
        // Should receive first state change event (Initializing)
        let event1 = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(event1.is_ok());
        
        if let Ok(Ok(CoreEvent::StateChanged { new_state, .. })) = event1 {
            assert_eq!(new_state, CoreState::Initializing);
        } else {
            panic!("Expected first StateChanged event to Initializing");
        }
        
        // Should receive second state change event (Ready)
        let event2 = timeout(Duration::from_secs(1), receiver.recv()).await;
        assert!(event2.is_ok());
        
        if let Ok(Ok(CoreEvent::StateChanged { new_state, .. })) = event2 {
            assert_eq!(new_state, CoreState::Ready);
        } else {
            panic!("Expected second StateChanged event to Ready");
        }
    }

    #[tokio::test]
    async fn test_configuration_update() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        
        let new_config = CoreConfig {
            max_concurrent_operations: 200,
            default_timeout_ms: 10000,
            debug_mode: true,
            memory_pool_size_mb: 512,
        };
        
        assert!(engine.update_config(&new_config).await.is_ok());
        
        let current_config = engine.get_config().await;
        assert_eq!(current_config.max_concurrent_operations, 200);
        assert_eq!(current_config.default_timeout_ms, 10000);
    }

    #[tokio::test]
    async fn test_statistics_reset() {
        let config = create_test_config();
        let mut engine = CoreEngine::new(config);
        assert!(engine.initialize().await.is_ok());
        
        // Record some operations
        engine.record_operation(Duration::from_millis(100)).await;
        engine.record_error().await;
        
        // Reset statistics
        assert!(engine.reset_statistics().await.is_ok());
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.failed_operations, 0);
    }
}