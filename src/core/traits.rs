//! Core traits for OpenAce Rust
//!
//! This module defines common traits and interfaces used throughout
//! the OpenAce system for consistent behavior and extensibility.

use crate::error::Result;
use crate::core::types::*;
use async_trait::async_trait;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::fmt::Debug;
use serde::{Serialize, Deserialize};

/// Initialization status of a component
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitializationStatus {
    Uninitialized,
    Initializing,
    Initialized,
    Failed(String),
}

/// Health status of a component
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning(String),
    Critical(String),
    Unknown,
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub network_bytes_in: u64,
    pub network_bytes_out: u64,
    pub disk_bytes_read: u64,
    pub disk_bytes_written: u64,
}

/// Statistic value with type information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StatisticValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Duration(Duration),
    Counter(u64),
    Gauge(f64),
}

/// Alert information
#[derive(Debug, Clone)]
pub struct Alert {
    pub id: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: Instant,
    pub source: String,
    pub metadata: HashMap<String, String>,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Resource information
#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub id: String,
    pub resource_type: String,
    pub amount: u64,
    pub allocated_at: Instant,
    pub last_accessed: Option<Instant>,
    pub metadata: HashMap<String, String>,
}

/// Resource allocation record
#[derive(Debug, Clone)]
pub struct ResourceAllocation {
    pub id: String,
    pub resource_type: String,
    pub amount: u64,
    pub allocated_at: Instant,
    pub deallocated_at: Option<Instant>,
    pub requester: String,
}

/// Trait for components that can be initialized and shutdown
#[async_trait]
pub trait Lifecycle: Send + Sync {
    /// Initialize the component
    async fn initialize(&mut self) -> Result<()>;
    
    /// Shutdown the component gracefully
    async fn shutdown(&mut self) -> Result<()>;
    
    /// Force shutdown the component (for emergency situations)
    async fn force_shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    
    /// Check if the component is initialized
    fn is_initialized(&self) -> bool;
    
    /// Get initialization status with details
    fn get_initialization_status(&self) -> InitializationStatus {
        if self.is_initialized() {
            InitializationStatus::Initialized
        } else {
            InitializationStatus::Uninitialized
        }
    }
}

/// Trait for components that can be paused and resumed
#[async_trait]
pub trait Pausable: Send + Sync {
    /// Pause the component
    async fn pause(&mut self) -> Result<()>;
    
    /// Resume the component
    async fn resume(&mut self) -> Result<()>;
    
    /// Check if the component is paused
    async fn is_paused(&self) -> bool;
    
    /// Toggle pause state
    async fn toggle_pause(&mut self) -> Result<()> {
        if self.is_paused().await {
            self.resume().await
        } else {
            self.pause().await
        }
    }
    
    /// Get pause duration if paused
    async fn get_pause_duration(&self) -> Option<Duration> {
        None // Default implementation
    }
}

/// Trait for components that provide statistics
#[async_trait]
pub trait StatisticsProvider: Send + Sync {
    /// Statistics type
    type Stats: Clone + Debug + Send + Sync;
    
    /// Get current statistics
    async fn get_statistics(&self) -> Self::Stats;
    
    /// Get statistics as a map
    async fn get_statistics_map(&self) -> HashMap<String, String> {
        HashMap::new() // Default implementation
    }
    
    /// Get detailed statistics with metadata
    async fn get_detailed_statistics(&self) -> HashMap<String, StatisticValue> {
        let basic_stats = self.get_statistics_map().await;
        basic_stats.into_iter()
            .map(|(k, v)| (k, StatisticValue::String(v)))
            .collect()
    }
    
    /// Get resource usage statistics
    async fn get_resource_usage(&self) -> Option<ResourceUsage> {
        None // Default implementation
    }
    
    /// Get health status
    async fn get_health_status(&self) -> HealthStatus {
        HealthStatus::Unknown // Default implementation
    }
    
    /// Reset statistics
    async fn reset_statistics(&mut self) -> Result<()>;
    
    /// Get statistics for a specific time range
    async fn get_statistics_range(&self, _start: Instant, _end: Instant) -> Self::Stats {
        self.get_statistics().await // Default implementation
    }
}

/// Trait for configurable components
#[async_trait]
pub trait Configurable: Send + Sync {
    /// Configuration type
    type Config: Clone + Debug + Send + Sync;
    
    /// Get current configuration
    async fn get_config(&self) -> Self::Config;
    
    /// Update configuration
    async fn update_config(&mut self, config: &Self::Config) -> Result<()>;
    
    /// Validate configuration
    async fn validate_config(config: &Self::Config) -> Result<()>;
    
    /// Apply configuration changes partially
    async fn apply_config_changes(&mut self, _changes: HashMap<String, String>) -> Result<()> {
        // Default implementation - subclasses should override for better performance
        let config = self.get_config().await;
        // This is a simplified approach - real implementation would need reflection or custom logic
        self.update_config(&config).await
    }
    
    /// Get configuration schema/metadata
    async fn get_config_schema(&self) -> HashMap<String, String> {
        HashMap::new() // Default empty schema
    }
    
    /// Check if configuration has changed since last update
    async fn has_config_changed(&self) -> bool {
        false // Default implementation
    }
    
    /// Reload configuration from external source
    async fn reload_config(&mut self) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
}

/// Trait for components that can be monitored
#[async_trait]
pub trait Monitorable: Send + Sync {
    /// Health status
    type Health: Clone + Debug + Send + Sync;
    
    /// Get health status
    async fn get_health(&self) -> Self::Health;
    
    /// Perform health check
    async fn health_check(&self) -> Result<Self::Health>;
    
    /// Get monitoring data
    async fn get_monitoring_data(&self) -> HashMap<String, String> {
        HashMap::new() // Default empty monitoring data
    }
    
    /// Get detailed health status with diagnostics
    async fn get_detailed_health(&self) -> HealthStatus {
        match self.health_check().await {
            Ok(_) => HealthStatus::Healthy,
            Err(e) => HealthStatus::Critical(e.to_string()),
        }
    }
    
    /// Get performance metrics
    async fn get_performance_metrics(&self) -> HashMap<String, f64> {
        HashMap::new() // Default empty metrics
    }
    
    /// Get alerts and warnings
    async fn get_alerts(&self) -> Vec<String> {
        Vec::new() // Default no alerts
    }
    
    /// Set monitoring thresholds
    async fn set_monitoring_thresholds(&mut self, _thresholds: HashMap<String, f64>) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
    
    /// Get uptime information
    async fn get_uptime(&self) -> Duration {
        Duration::from_secs(0) // Default implementation
    }
    
    /// Get last error information
    async fn get_last_error(&self) -> Option<String> {
        None // Default implementation
    }
}

/// Trait for event handling
#[async_trait]
pub trait EventHandler<T>: Send + Sync
where
    T: Send + Sync + 'static,
{
    /// Handle an event
    async fn handle_event(&self, event: T) -> Result<()>;
    
    /// Check if the handler can process this event type
    fn can_handle(&self, _event: &T) -> bool {
        true // Default implementation accepts all events
    }
    
    /// Handle multiple events in batch
    async fn handle_events(&self, events: Vec<T>) -> Result<Vec<Result<()>>> {
        let mut results = Vec::new();
        for event in events {
            results.push(self.handle_event(event).await);
        }
        Ok(results)
    }
    
    /// Get event handler priority (higher number = higher priority)
    fn get_priority(&self) -> i32 {
        0 // Default priority
    }
    
    /// Get supported event types
    fn get_supported_event_types(&self) -> Vec<String> {
        Vec::new() // Default empty list
    }
    
    /// Filter events before processing
    fn filter_event(&self, event: &T) -> bool {
        self.can_handle(event)
    }
    
    /// Get event processing statistics
    async fn get_event_stats(&self) -> HashMap<String, u64> {
        HashMap::new() // Default empty stats
    }
}

/// Trait for data processing
#[async_trait]
pub trait DataProcessor<Input, Output>: Send + Sync
where
    Input: Send + Sync + 'static,
    Output: Send + Sync + 'static,
{
    /// Process input data and return output
    async fn process(&self, input: Input) -> Result<Output>;
    
    /// Process data in batch
    async fn process_batch(&self, inputs: Vec<Input>) -> Result<Vec<Output>> {
        let mut outputs = Vec::with_capacity(inputs.len());
        for input in inputs {
            outputs.push(self.process(input).await?);
        }
        Ok(outputs)
    }
}

/// Trait for data streaming
#[async_trait]
pub trait DataStreamer<T>: Send + Sync
where
    T: Send + Sync + 'static,
{
    /// Start streaming data
    async fn start_stream(&self) -> Result<()>;
    
    /// Stop streaming data
    async fn stop_stream(&self) -> Result<()>;
    
    /// Send data to stream
    async fn send(&self, data: T) -> Result<()>;
    
    /// Receive data from stream
    async fn receive(&self) -> Result<Option<T>>;
    
    /// Check if stream is active
    fn is_streaming(&self) -> bool;
}

/// Trait for resource management
#[async_trait]
pub trait ResourceManager<T>: Send + Sync
where
    T: Send + Sync + 'static,
{
    /// Acquire a resource
    async fn acquire(&self) -> Result<T>;
    
    /// Release a resource
    async fn release(&self, resource: T) -> Result<()>;
    
    /// Get available resource count
    fn available_count(&self) -> usize;
    
    /// Get total resource count
    fn total_count(&self) -> usize;
    
    /// Get detailed resource information
    async fn get_resource_info(&self, _resource_id: &str) -> Option<ResourceInfo> {
        None // Default implementation
    }
    
    /// Set resource limits
    async fn set_resource_limits(&mut self, _limits: HashMap<String, u64>) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
    
    /// Get resource limits
    async fn get_resource_limits(&self) -> HashMap<String, u64> {
        HashMap::new() // Default empty limits
    }
    
    /// Cleanup unused resources
    async fn cleanup_resources(&mut self) -> Result<u64> {
        Ok(0) // Default implementation returns 0 cleaned resources
    }
    
    /// Get resource allocation history
    async fn get_allocation_history(&self) -> Vec<ResourceAllocation> {
        Vec::new() // Default empty history
    }
    
    /// Reserve resources for future use
    async fn reserve_resource(&mut self, _resource_type: &str, _amount: u64, _duration: Duration) -> Result<String> {
        // Default implementation just acquires immediately
        let resource = self.acquire().await?;
        Ok(format!("reserved-{}", std::ptr::addr_of!(resource) as usize))
    }
    
    /// Release reserved resources
    async fn release_reservation(&mut self, _reservation_id: &str) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size: usize,
    pub capacity: usize,
}

/// Cache eviction strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionStrategy {
    LRU,  // Least Recently Used
    LFU,  // Least Frequently Used
    FIFO, // First In First Out
    TTL,  // Time To Live
    Random,
}

/// Trait for caching
#[async_trait]
pub trait Cache<K, V>: Send + Sync
where
    K: Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    /// Get value by key
    async fn get(&self, key: &K) -> Result<Option<V>>;
    
    /// Set value for key
    async fn set(&self, key: K, value: V) -> Result<()>;
    
    /// Remove value by key
    async fn remove(&self, key: &K) -> Result<Option<V>>;
    
    /// Clear all cached values
    async fn clear(&self) -> Result<()>;
    
    /// Get cache size
    fn size(&self) -> usize;
    
    /// Check if key exists
    async fn contains(&self, key: &K) -> bool {
        self.get(key).await.map(|v| v.is_some()).unwrap_or(false)
    }
    
    /// Set value with expiration time
    async fn set_with_ttl(&self, key: K, value: V, _ttl: Duration) -> Result<()> {
        // Default implementation ignores TTL
        self.set(key, value).await
    }
    
    /// Get cache hit rate
    async fn get_hit_rate(&self) -> f64 {
        0.0 // Default implementation
    }
    
    /// Get cache statistics
    async fn get_cache_stats(&self) -> CacheStats {
        CacheStats {
            hits: 0,
            misses: 0,
            evictions: 0,
            size: self.size(),
            capacity: 0,
        }
    }
    
    /// Set cache capacity
    async fn set_capacity(&mut self, _capacity: usize) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
    
    /// Get all keys in cache
    async fn keys(&self) -> Vec<K> {
        Vec::new() // Default empty implementation
    }
    
    /// Refresh/update a cache entry
    async fn refresh(&mut self, _key: &K) -> Result<bool> {
        Ok(false) // Default implementation - not refreshed
    }
    
    /// Set eviction strategy
    async fn set_eviction_strategy(&mut self, _strategy: EvictionStrategy) -> Result<()> {
        Ok(()) // Default no-op implementation
    }
    
    /// Get current eviction strategy
    async fn get_eviction_strategy(&self) -> EvictionStrategy {
        EvictionStrategy::LRU // Default strategy
    }
    
    /// Manually trigger eviction
    async fn evict(&mut self, _count: usize) -> Result<usize> {
        Ok(0) // Default implementation - nothing evicted
    }
    
    /// Get cache capacity
    async fn get_capacity(&self) -> usize {
        0 // Default unlimited capacity
    }
    
    /// Check if cache is full
    async fn is_full(&self) -> bool {
        let capacity = self.get_capacity().await;
        capacity > 0 && self.size() >= capacity
    }
    
    /// Get remaining capacity
    async fn remaining_capacity(&self) -> usize {
        let capacity = self.get_capacity().await;
        if capacity == 0 {
            usize::MAX // Unlimited
        } else {
            capacity.saturating_sub(self.size())
        }
    }
}

/// Trait for serialization
pub trait Serializable: Send + Sync {
    /// Serialize to bytes
    fn serialize(&self) -> Result<Vec<u8>>;
    
    /// Deserialize from bytes
    fn deserialize(data: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

/// Trait for validation
pub trait Validatable: Send + Sync {
    /// Validation error type
    type ValidationError: std::error::Error + Send + Sync;
    
    /// Validate the object
    fn validate(&self) -> std::result::Result<(), Self::ValidationError>;
    
    /// Check if object is valid
    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Trait for cloneable components
pub trait CloneableComponent: Send + Sync {
    /// Clone the component
    fn clone_component(&self) -> Box<dyn CloneableComponent>;
}

/// Trait for named components
pub trait Named: Send + Sync {
    /// Get component name
    fn name(&self) -> &str;
    
    /// Get component description
    fn description(&self) -> Option<&str> {
        None
    }
    
    /// Get component version
    fn version(&self) -> Option<Version> {
        None
    }
}

/// Trait for components with metadata
pub trait MetadataProvider: Send + Sync {
    /// Get metadata
    fn get_metadata(&self) -> HashMap<String, String>;
    
    /// Set metadata value
    fn set_metadata(&self, key: String, value: String) -> Result<()>;
    
    /// Remove metadata value
    fn remove_metadata(&self, key: &str) -> Result<Option<String>>;
    
    /// Clear all metadata
    fn clear_metadata(&self) -> Result<()>;
}

/// Trait for progress tracking
pub trait ProgressTracker: Send + Sync {
    /// Get current progress (0.0 to 1.0)
    fn get_progress(&self) -> f32;
    
    /// Set progress
    fn set_progress(&self, progress: f32) -> Result<()>;
    
    /// Get progress message
    fn get_progress_message(&self) -> Option<String>;
    
    /// Set progress message
    fn set_progress_message(&self, message: Option<String>) -> Result<()>;
    
    /// Check if operation is complete
    fn is_complete(&self) -> bool {
        self.get_progress() >= 1.0
    }
}

/// Trait for rate limiting
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if operation is allowed
    async fn is_allowed(&self) -> bool;
    
    /// Wait until operation is allowed
    async fn wait_for_permission(&self) -> Result<()>;
    
    /// Get time until next operation is allowed
    async fn time_until_allowed(&self) -> Option<Duration>;
    
    /// Reset rate limiter
    async fn reset(&self) -> Result<()>;
}

/// Retry strategy types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryStrategy {
    FixedDelay,
    ExponentialBackoff,
    LinearBackoff,
    CustomBackoff,
}

/// Retry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStats {
    pub total_attempts: u64,
    pub successful_retries: u64,
    pub failed_retries: u64,
    pub average_delay: Duration,
}

/// Trait for retry logic
#[async_trait]
pub trait RetryPolicy: Send + Sync {
    /// Check if operation should be retried
    fn should_retry(&self, attempt: u32, error: &dyn std::error::Error) -> bool;
    
    /// Get delay before next retry
    fn get_retry_delay(&self, attempt: u32) -> Duration;
    
    /// Get maximum number of retries
    fn max_retries(&self) -> u32;
    
    /// Get retry strategy
    fn get_strategy(&self) -> RetryStrategy {
        RetryStrategy::FixedDelay
    }
    
    /// Check if error is retryable
    fn is_retryable_error(&self, _error: &dyn std::error::Error) -> bool {
        true // Default: all errors are retryable
    }
    
    /// Get backoff multiplier for exponential backoff
    fn get_backoff_multiplier(&self) -> f64 {
        2.0 // Default exponential backoff multiplier
    }
    
    /// Get maximum delay between retries
    fn get_max_delay(&self) -> Duration {
        Duration::from_secs(300) // Default 5 minutes max delay
    }
    
    /// Get jitter factor for randomizing delays
    fn get_jitter_factor(&self) -> f64 {
        0.1 // Default 10% jitter
    }
    
    /// Reset retry state
    async fn reset(&mut self) {
        // Default no-op implementation
    }
    
    /// Get retry statistics
    async fn get_retry_stats(&self) -> RetryStats {
        RetryStats {
            total_attempts: 0,
            successful_retries: 0,
            failed_retries: 0,
            average_delay: Duration::from_secs(0),
        }
    }
}

/// Trait for connection management
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// Connection type
    type Connection: Send + Sync;
    
    /// Connect to remote endpoint
    async fn connect(&self, address: &str) -> Result<Self::Connection>;
    
    /// Disconnect from remote endpoint
    async fn disconnect(&self, connection: Self::Connection) -> Result<()>;
    
    /// Get active connections
    fn get_connections(&self) -> Vec<ConnectionInfo>;
    
    /// Get connection count
    fn connection_count(&self) -> usize;
}

/// Trait for task scheduling
#[async_trait]
pub trait TaskScheduler: Send + Sync {
    /// Task type
    type Task: Send + Sync;
    
    /// Schedule a task
    async fn schedule(&self, task: Self::Task, delay: Option<Duration>) -> Result<Id>;
    
    /// Cancel a scheduled task
    async fn cancel(&self, task_id: &Id) -> Result<bool>;
    
    /// Get scheduled tasks
    fn get_scheduled_tasks(&self) -> Vec<TaskInfo>;
    
    /// Get task count
    fn task_count(&self) -> usize;
}

/// Trait for plugin system
#[async_trait]
pub trait Plugin: Named + Send + Sync {
    /// Initialize the plugin
    async fn initialize(&self) -> Result<()>;
    
    /// Shutdown the plugin
    async fn shutdown(&self) -> Result<()>;
    
    /// Get plugin dependencies
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// Check if plugin is enabled
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Trait for factory pattern
pub trait Factory<T>: Send + Sync {
    /// Configuration type for creation
    type Config;
    
    /// Create new instance
    fn create(&self, config: Self::Config) -> Result<T>;
    
    /// Check if factory can create with given config
    fn can_create(&self, config: &Self::Config) -> bool;
}

/// Trait for observer pattern
#[async_trait]
pub trait Observer<T>: Send + Sync
where
    T: Send + Sync,
{
    /// Handle notification
    async fn notify(&self, event: T) -> Result<()>;
}

/// Trait for observable subjects
#[async_trait]
pub trait Observable<T>: Send + Sync
where
    T: Send + Sync,
{
    /// Observer type
    type Observer: Observer<T>;
    
    /// Add observer
    async fn add_observer(&self, observer: Self::Observer) -> Result<()>;
    
    /// Remove observer
    async fn remove_observer(&self, observer: &Self::Observer) -> Result<bool>;
    
    /// Notify all observers
    async fn notify_observers(&self, event: T) -> Result<()>;
    
    /// Get observer count
    fn observer_count(&self) -> usize;
}

/// Trait for command pattern
#[async_trait]
pub trait Command: Send + Sync {
    /// Execute the command
    async fn execute(&self) -> Result<()>;
    
    /// Undo the command (if supported)
    async fn undo(&self) -> Result<()> {
        Err(crate::error::OpenAceError::internal("Undo not supported"))
    }
    
    /// Check if command can be undone
    fn can_undo(&self) -> bool {
        false
    }
    
    /// Get command description
    fn description(&self) -> String;
}

/// Trait for state machine
#[async_trait]
pub trait StateMachine: Send + Sync {
    /// State type
    type State: Clone + Debug + Send + Sync;
    
    /// Event type
    type Event: Send + Sync;
    
    /// Get current state
    fn current_state(&self) -> Self::State;
    
    /// Process event and transition state
    async fn process_event(&self, event: Self::Event) -> Result<Self::State>;
    
    /// Check if transition is valid
    fn is_valid_transition(&self, from: &Self::State, to: &Self::State) -> bool;
    
    /// Get possible transitions from current state
    fn possible_transitions(&self) -> Vec<Self::State>;
}

/// Trait for middleware pattern
#[async_trait]
pub trait Middleware<Request, Response>: Send + Sync
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    /// Process request before main handler
    async fn before(&self, _request: &mut Request) -> Result<()> {
        Ok(())
    }
    
    /// Process response after main handler
    async fn after(&self, _request: &Request, _response: &mut Response) -> Result<()> {
        Ok(())
    }
    
    /// Handle error during processing
    async fn on_error(&self, _request: &Request, _error: &dyn std::error::Error) -> Result<()> {
        Ok(())
    }
}

/// Trait for builder pattern
pub trait Builder<T>: Send + Sync {
    /// Build the final object
    fn build(self) -> Result<T>
    where
        Self: Sized;
    
    /// Validate builder state
    fn validate(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    // Test implementation of Lifecycle trait
    struct TestComponent {
        initialized: Arc<RwLock<bool>>,
    }
    
    impl TestComponent {
        fn new() -> Self {
            Self {
                initialized: Arc::new(RwLock::new(false)),
            }
        }
    }
    
    #[async_trait]
    impl Lifecycle for TestComponent {
        async fn initialize(&mut self) -> Result<()> {
            let mut initialized = self.initialized.write().await;
            *initialized = true;
            Ok(())
        }
        
        async fn shutdown(&mut self) -> Result<()> {
            let mut initialized = self.initialized.write().await;
            *initialized = false;
            Ok(())
        }
        
        fn is_initialized(&self) -> bool {
            // For testing, we'll use try_read
            self.initialized.try_read().map(|v| *v).unwrap_or(false)
        }
    }
    
    #[tokio::test]
    async fn test_lifecycle_trait() {
        let mut component = TestComponent::new();
        
        assert!(!component.is_initialized());
        
        component.initialize().await.unwrap();
        assert!(component.is_initialized());
        
        component.shutdown().await.unwrap();
        assert!(!component.is_initialized());
    }
    
    // Test implementation of StatisticsProvider trait
    #[derive(Clone, Debug)]
    struct TestStats {
        counter: u64,
    }
    
    struct TestStatsProvider {
        stats: Arc<RwLock<TestStats>>,
    }
    
    impl TestStatsProvider {
        fn new() -> Self {
            Self {
                stats: Arc::new(RwLock::new(TestStats { counter: 0 })),
            }
        }
        
        async fn increment(&self) {
            let mut stats = self.stats.write().await;
            stats.counter += 1;
        }
    }
    
    #[async_trait]
    impl StatisticsProvider for TestStatsProvider {
        type Stats = TestStats;
        
        async fn get_statistics(&self) -> Self::Stats {
            self.stats.read().await.clone()
        }
        
        async fn reset_statistics(&mut self) -> Result<()> {
            let mut stats = self.stats.write().await;
            stats.counter = 0;
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_statistics_provider_trait() {
        let mut provider = TestStatsProvider::new();
        
        let stats = provider.get_statistics().await;
        assert_eq!(stats.counter, 0);
        
        provider.increment().await;
        let stats = provider.get_statistics().await;
        assert_eq!(stats.counter, 1);
        
        provider.reset_statistics().await.unwrap();
        let stats = provider.get_statistics().await;
        assert_eq!(stats.counter, 0);
    }
}