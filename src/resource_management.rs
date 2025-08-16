//! Resource Management System for OpenAce Rust
//!
//! Provides comprehensive resource lifecycle management, optimization,
//! and monitoring with support for automatic cleanup and resource pooling.

use crate::error::{OpenAceError, ErrorContext, RecoveryStrategy, ErrorSeverity, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use serde::{Serialize, Deserialize};
use tokio::sync::{broadcast, Semaphore};
use tracing::{info, warn, error, debug};
use std::fmt::Debug;
use std::hash::Hash;
use std::any::{Any, TypeId};
use async_trait::async_trait;

/// Resource lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceState {
    /// Resource is being initialized
    Initializing,
    /// Resource is active and available
    Active,
    /// Resource is idle but still allocated
    Idle,
    /// Resource is being cleaned up
    Cleanup,
    /// Resource has been disposed
    Disposed,
    /// Resource is in error state
    Error,
}

/// Resource priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourcePriority {
    /// Critical resources that should never be disposed
    Critical = 0,
    /// High priority resources
    High = 1,
    /// Normal priority resources
    Normal = 2,
    /// Low priority resources (first to be disposed)
    Low = 3,
}

/// Resource usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageStats {
    /// Number of times resource was accessed
    pub access_count: u64,
    /// Last access timestamp
    pub last_accessed: u64,
    /// Total time resource was active (in milliseconds)
    pub total_active_time: u64,
    /// Average access duration (in milliseconds)
    pub avg_access_duration: f64,
    /// Peak memory usage (in bytes)
    pub peak_memory_usage: u64,
    /// Current memory usage (in bytes)
    pub current_memory_usage: u64,
}

impl Default for ResourceUsageStats {
    fn default() -> Self {
        Self {
            access_count: 0,
            last_accessed: 0,
            total_active_time: 0,
            avg_access_duration: 0.0,
            peak_memory_usage: 0,
            current_memory_usage: 0,
        }
    }
}

/// Resource metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetadata {
    /// Unique resource identifier
    pub id: String,
    /// Resource type name
    pub resource_type: String,
    /// Resource priority
    pub priority: ResourcePriority,
    /// Current state
    pub state: ResourceState,
    /// Creation timestamp
    pub created_at: u64,
    /// Last state change timestamp
    pub last_state_change: u64,
    /// Resource tags for categorization
    pub tags: Vec<String>,
    /// Usage statistics
    pub usage_stats: ResourceUsageStats,
    /// Resource-specific configuration
    pub config: HashMap<String, String>,
}

/// Resource lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceEvent {
    /// Resource was created
    Created {
        resource_id: String,
        resource_type: String,
        timestamp: u64,
    },
    /// Resource state changed
    StateChanged {
        resource_id: String,
        old_state: ResourceState,
        new_state: ResourceState,
        timestamp: u64,
    },
    /// Resource was accessed
    Accessed {
        resource_id: String,
        access_duration: u64,
        timestamp: u64,
    },
    /// Resource was disposed
    Disposed {
        resource_id: String,
        reason: String,
        timestamp: u64,
    },
    /// Resource error occurred
    Error {
        resource_id: String,
        error_message: String,
        timestamp: u64,
    },
    /// Memory pressure detected
    MemoryPressure {
        current_usage: u64,
        threshold: u64,
        timestamp: u64,
    },
}

/// Resource trait that all managed resources must implement
#[async_trait]
pub trait ManagedResource: Send + Sync + Debug {
    /// Get resource type name
    fn resource_type(&self) -> &str;
    
    /// Get current memory usage in bytes
    fn memory_usage(&self) -> u64;
    
    /// Check if resource is healthy
    async fn health_check(&self) -> Result<bool>;
    
    /// Cleanup resource (called before disposal)
    async fn cleanup(&mut self) -> Result<()>;
    
    /// Get resource-specific metrics
    fn get_metrics(&self) -> HashMap<String, f64> {
        HashMap::new()
    }
}

/// Resource handle for safe access to managed resources
#[derive(Debug)]
pub struct ResourceHandle<T>
where
    T: ManagedResource + 'static,
{
    resource: Arc<RwLock<T>>,
    metadata: Arc<RwLock<ResourceMetadata>>,
    manager: Weak<ResourceManager>,
    access_start: Instant,
}

impl<T> ResourceHandle<T>
where
    T: ManagedResource + 'static,
{
    /// Create a new resource handle
    fn new(
        resource: Arc<RwLock<T>>,
        metadata: Arc<RwLock<ResourceMetadata>>,
        manager: Weak<ResourceManager>,
    ) -> Self {
        Self {
            resource,
            metadata,
            manager,
            access_start: Instant::now(),
        }
    }

    /// Get read access to the resource
    pub async fn read(&self) -> Result<std::sync::RwLockReadGuard<T>> {
        self.resource.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire read lock: {}", e),
                ErrorContext::new("resource_management", "handle_read")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })
    }

    /// Get write access to the resource
    pub async fn write(&self) -> Result<std::sync::RwLockWriteGuard<T>> {
        self.resource.write().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire write lock: {}", e),
                ErrorContext::new("resource_management", "handle_write")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })
    }

    /// Get resource metadata
    pub fn metadata(&self) -> Result<std::sync::RwLockReadGuard<ResourceMetadata>> {
        self.metadata.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire metadata lock: {}", e),
                ErrorContext::new("resource_management", "handle_metadata")
                    .with_severity(ErrorSeverity::Low),
                RecoveryStrategy::None,
            )
        })
    }

    /// Update resource usage statistics
    fn update_usage_stats(&self) {
        if let Ok(mut metadata) = self.metadata.write() {
            let access_duration = self.access_start.elapsed().as_millis() as u64;
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            metadata.usage_stats.access_count += 1;
            metadata.usage_stats.last_accessed = timestamp;
            
            // Update average access duration
            let total_duration = metadata.usage_stats.avg_access_duration * (metadata.usage_stats.access_count - 1) as f64;
            metadata.usage_stats.avg_access_duration = (total_duration + access_duration as f64) / metadata.usage_stats.access_count as f64;

            // Update memory usage if available
            if let Ok(resource) = self.resource.read() {
                let current_usage = resource.memory_usage();
                metadata.usage_stats.current_memory_usage = current_usage;
                if current_usage > metadata.usage_stats.peak_memory_usage {
                    metadata.usage_stats.peak_memory_usage = current_usage;
                }
            }
        }

        // Notify manager of access
        if let Some(manager) = self.manager.upgrade() {
            if let Ok(metadata) = self.metadata.read() {
                let access_duration = self.access_start.elapsed().as_millis() as u64;
                let event = ResourceEvent::Accessed {
                    resource_id: metadata.id.clone(),
                    access_duration,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };
                let _ = manager.event_sender.send(event);
            }
        }
    }
}

impl<T> Drop for ResourceHandle<T>
where
    T: ManagedResource + 'static,
{
    fn drop(&mut self) {
        self.update_usage_stats();
    }
}

/// Resource pool for managing multiple instances of the same resource type
pub struct ResourcePool<T> {
    /// Available resources
    available: Arc<Mutex<Vec<Arc<RwLock<T>>>>>,
    /// Resource factory function
    factory: Arc<dyn Fn() -> Result<T> + Send + Sync>,
    /// Maximum pool size
    max_size: usize,
    /// Current pool size
    current_size: Arc<Mutex<usize>>,
    /// Semaphore for controlling access
    semaphore: Arc<Semaphore>,
    /// Pool configuration
    config: PoolConfig,
}

impl<T> std::fmt::Debug for ResourcePool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourcePool")
            .field("max_size", &self.max_size)
            .field("current_size", &self.current_size)
            .field("config", &self.config)
            .field("factory", &"<closure>")
            .finish()
    }
}

/// Resource pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of resources to keep in pool
    pub min_size: usize,
    /// Maximum number of resources in pool
    pub max_size: usize,
    /// Maximum idle time before resource is disposed (in seconds)
    pub max_idle_time: u64,
    /// Health check interval (in seconds)
    pub health_check_interval: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 1,
            max_size: 10,
            max_idle_time: 300, // 5 minutes
            health_check_interval: 60, // 1 minute
        }
    }
}

impl<T> ResourcePool<T>
where
    T: ManagedResource + 'static,
{
    /// Create a new resource pool
    pub fn new<F>(factory: F, config: PoolConfig) -> Self
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        Self {
            available: Arc::new(Mutex::new(Vec::new())),
            factory: Arc::new(factory),
            max_size: config.max_size,
            current_size: Arc::new(Mutex::new(0)),
            semaphore: Arc::new(Semaphore::new(config.max_size)),
            config,
        }
    }

    /// Get a resource from the pool
    pub async fn acquire(&self) -> Result<Arc<RwLock<T>>> {
        // Wait for available slot
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire semaphore: {}", e),
                ErrorContext::new("resource_management", "pool_acquire")
                    .with_severity(ErrorSeverity::High),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;

        // Try to get existing resource
        {
            let mut available = self.available.lock().unwrap();
            if let Some(resource) = available.pop() {
                return Ok(resource);
            }
        }

        // Create new resource if under limit
        {
            let mut current_size = self.current_size.lock().unwrap();
            if *current_size < self.max_size {
                let resource = (self.factory)()?;
                let resource_arc = Arc::new(RwLock::new(resource));
                *current_size += 1;
                return Ok(resource_arc);
            }
        }

        // Should not reach here due to semaphore, but handle gracefully
        Err(OpenAceError::new_with_context(
            "resource",
            "Pool exhausted",
            ErrorContext::new("resource_management", "pool_acquire")
                .with_severity(ErrorSeverity::High),
            RecoveryStrategy::Retry {
                max_attempts: 3,
                base_delay_ms: 1000,
            },
        ))
    }

    /// Return a resource to the pool
    #[allow(clippy::await_holding_lock)]
    pub async fn release(&self, resource: Arc<RwLock<T>>) -> Result<()> {
        // Check resource health before returning to pool
        let is_healthy = {
            let resource_guard = resource.read().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire resource lock: {}", e),
                    ErrorContext::new("resource_management", "pool_release")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::None,
                )
            })?;

            resource_guard.health_check().await
        }?;

        if !is_healthy {
            // Resource is unhealthy, dispose it
            self.dispose_resource(resource).await?;
            return Ok(());
        }

        // Return healthy resource to pool
        {
            let mut available = self.available.lock().unwrap();
            available.push(resource);
        }

        Ok(())
    }

    /// Dispose a resource and update pool size
    #[allow(clippy::await_holding_lock)]
    async fn dispose_resource(&self, resource: Arc<RwLock<T>>) -> Result<()> {
        // Perform cleanup
        {
            let mut resource_guard = resource.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire resource lock for cleanup: {}", e),
                    ErrorContext::new("resource_management", "pool_dispose")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::None,
                )
            })?;

            resource_guard.cleanup().await?;
        }

        {
            let mut current_size = self.current_size.lock().unwrap();
            *current_size = current_size.saturating_sub(1);
        }

        Ok(())
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let available_count = self.available.lock().unwrap().len();
        let current_size = *self.current_size.lock().unwrap();
        
        PoolStats {
            total_resources: current_size,
            available_resources: available_count,
            active_resources: current_size - available_count,
            max_size: self.max_size,
        }
    }
}

/// Resource pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_resources: usize,
    pub available_resources: usize,
    pub active_resources: usize,
    pub max_size: usize,
}

/// Main resource manager
#[derive(Debug)]
pub struct ResourceManager {
    /// Managed resources
    resources: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
    /// Resource metadata
    metadata: Arc<RwLock<HashMap<String, Arc<RwLock<ResourceMetadata>>>>>,
    /// Resource pools by type
    pools: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
    /// Event broadcaster
    pub event_sender: broadcast::Sender<ResourceEvent>,
    /// Memory usage threshold (in bytes)
    memory_threshold: u64,
    /// Cleanup interval (in seconds)
    cleanup_interval: u64,
    /// Manager configuration
    config: ManagerConfig,
}

/// Resource manager configuration
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Maximum total memory usage (in bytes)
    pub max_memory_usage: u64,
    /// Memory pressure threshold (percentage)
    pub memory_pressure_threshold: f64,
    /// Automatic cleanup enabled
    pub auto_cleanup: bool,
    /// Cleanup interval (in seconds)
    pub cleanup_interval: u64,
    /// Health check interval (in seconds)
    pub health_check_interval: u64,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            max_memory_usage: 1024 * 1024 * 1024, // 1GB
            memory_pressure_threshold: 0.8, // 80%
            auto_cleanup: true,
            cleanup_interval: 300, // 5 minutes
            health_check_interval: 60, // 1 minute
        }
    }
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(config: ManagerConfig) -> Arc<Self> {
        let (event_sender, _) = broadcast::channel(1000);
        
        let manager = Arc::new(Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            pools: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            memory_threshold: (config.max_memory_usage as f64 * config.memory_pressure_threshold) as u64,
            cleanup_interval: config.cleanup_interval,
            config,
        });

        // Start background tasks
        if manager.config.auto_cleanup {
            Self::start_cleanup_task(manager.clone());
        }
        Self::start_health_check_task(manager.clone());

        manager
    }

    /// Register a resource
    pub async fn register_resource<T>(
        &self,
        id: impl Into<String>,
        resource: T,
        priority: ResourcePriority,
        tags: Vec<String>,
    ) -> Result<ResourceHandle<T>>
    where
        T: ManagedResource + 'static,
    {
        let id = id.into();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let metadata = ResourceMetadata {
            id: id.clone(),
            resource_type: resource.resource_type().to_string(),
            priority,
            state: ResourceState::Initializing,
            created_at: timestamp,
            last_state_change: timestamp,
            tags,
            usage_stats: ResourceUsageStats::default(),
            config: HashMap::new(),
        };

        let resource_arc = Arc::new(RwLock::new(resource));
        let metadata_arc = Arc::new(RwLock::new(metadata));

        // Store resource and metadata
        {
            let mut resources = self.resources.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire resources lock: {}", e),
                    ErrorContext::new("resource_management", "register_resource")
                    .with_severity(ErrorSeverity::High),
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay_ms: 500,
                },
                )
            })?;

            let mut metadata_map = self.metadata.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire metadata lock: {}", e),
                    ErrorContext::new("resource_management", "register_resource")
                        .with_severity(ErrorSeverity::High),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;

            resources.insert(id.clone(), Box::new(resource_arc.clone()));
            metadata_map.insert(id.clone(), metadata_arc.clone());
        }

        // Update state to active
        self.update_resource_state(&id, ResourceState::Active).await?;

        // Emit creation event
        let event = ResourceEvent::Created {
            resource_id: id.clone(),
            resource_type: metadata_arc.read().unwrap().resource_type.clone(),
            timestamp,
        };
        let _ = self.event_sender.send(event);

        info!(resource_id = %id, "Resource registered successfully");

        Ok(ResourceHandle::new(
            resource_arc,
            metadata_arc,
            Arc::downgrade(&Arc::new(self.clone())),
        ))
    }

    /// Get a resource handle
    pub async fn get_resource<T>(&self, id: &str) -> Result<Option<ResourceHandle<T>>>
    where
        T: ManagedResource + 'static,
    {
        let resources = self.resources.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire resources lock: {}", e),
                ErrorContext::new("resource_management", "register_resource")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;

        let metadata_map = self.metadata.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire metadata lock: {}", e),
                ErrorContext::new("resource_management", "get_resource")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;

        if let (Some(resource_any), Some(metadata_arc)) = (resources.get(id), metadata_map.get(id)) {
            if let Some(resource_arc) = resource_any.downcast_ref::<Arc<RwLock<T>>>() {
                return Ok(Some(ResourceHandle::new(
                    resource_arc.clone(),
                    metadata_arc.clone(),
                    Arc::downgrade(&Arc::new(self.clone())),
                )));
            }
        }

        Ok(None)
    }

    /// Dispose a resource
    pub async fn dispose_resource(&self, id: &str) -> Result<()> {
        // Get resource for cleanup
        let (resource_any, metadata_arc) = {
            let mut resources = self.resources.write().map_err(|e| {
                OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire resources lock: {}", e),
                ErrorContext::new("resource_management", "dispose_resource")
                    .with_severity(ErrorSeverity::High),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
            })?;

            let metadata_map = self.metadata.read().map_err(|e| {
                OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire metadata lock: {}", e),
                ErrorContext::new("resource_management", "dispose_resource")
                    .with_severity(ErrorSeverity::High),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
            })?;

            let resource_any = resources.remove(id);
            let metadata_arc = metadata_map.get(id).cloned();

            (resource_any, metadata_arc)
        };

        if let (Some(_resource), Some(_metadata_arc)) = (resource_any, metadata_arc) {
            // Update state to cleanup
            self.update_resource_state(id, ResourceState::Cleanup).await?;

            // Perform cleanup (resource-specific cleanup would be called here)
            // Note: In a real implementation, we'd need to downcast and call cleanup

            // Update state to disposed
            self.update_resource_state(id, ResourceState::Disposed).await?;

            // Remove metadata
            {
                let mut metadata_map = self.metadata.write().map_err(|e| {
                    OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire metadata lock: {}", e),
                    ErrorContext::new("resource_management", "dispose_resource")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::None,
                )
                })?;
                metadata_map.remove(id);
            }

            // Emit disposal event
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let event = ResourceEvent::Disposed {
                resource_id: id.to_string(),
                reason: "Manual disposal".to_string(),
                timestamp,
            };
            let _ = self.event_sender.send(event);

            info!(resource_id = %id, "Resource disposed successfully");
        }

        Ok(())
    }

    /// Update resource state
    async fn update_resource_state(&self, id: &str, new_state: ResourceState) -> Result<()> {
        let metadata_map = self.metadata.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire metadata lock: {}", e),
                ErrorContext::new("resource_management", "update_state")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;

        if let Some(metadata_arc) = metadata_map.get(id) {
            let mut metadata = metadata_arc.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire metadata write lock: {}", e),
                    ErrorContext::new("resource_management", "update_state")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;

            let old_state = metadata.state;
            metadata.state = new_state;
            metadata.last_state_change = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            // Emit state change event
            let event = ResourceEvent::StateChanged {
                resource_id: id.to_string(),
                old_state,
                new_state,
                timestamp: metadata.last_state_change,
            };
            let _ = self.event_sender.send(event);
        }

        Ok(())
    }

    /// Get resource statistics
    pub async fn get_stats(&self) -> Result<ResourceManagerStats> {
        let metadata_map = self.metadata.read().map_err(|e| {
            OpenAceError::new_with_context(
                "resource",
                format!("Failed to acquire metadata lock: {}", e),
                ErrorContext::new("resource_management", "get_stats")
                    .with_severity(ErrorSeverity::Low),
                RecoveryStrategy::None,
            )
        })?;

        let mut stats = ResourceManagerStats::default();
        let mut total_memory = 0u64;
        let mut state_counts = HashMap::new();
        let mut priority_counts = HashMap::new();

        for metadata_arc in metadata_map.values() {
            if let Ok(metadata) = metadata_arc.read() {
                stats.total_resources += 1;
                total_memory += metadata.usage_stats.current_memory_usage;

                *state_counts.entry(metadata.state).or_insert(0) += 1;
                *priority_counts.entry(metadata.priority).or_insert(0) += 1;

                if metadata.state == ResourceState::Active {
                    stats.active_resources += 1;
                }
                if metadata.state == ResourceState::Idle {
                    stats.idle_resources += 1;
                }
            }
        }

        stats.total_memory_usage = total_memory;
        stats.memory_pressure = if self.config.max_memory_usage > 0 {
            total_memory as f64 / self.config.max_memory_usage as f64
        } else {
            0.0
        };

        Ok(stats)
    }

    /// Subscribe to resource events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<ResourceEvent> {
        self.event_sender.subscribe()
    }

    /// Start cleanup background task
    fn start_cleanup_task(manager: Arc<Self>) {
        let cleanup_interval = Duration::from_secs(manager.cleanup_interval);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = manager.cleanup_idle_resources().await {
                    error!(error = %e, "Failed to cleanup idle resources");
                }
            }
        });
    }

    /// Start health check background task
    fn start_health_check_task(manager: Arc<Self>) {
        let health_check_interval = Duration::from_secs(manager.config.health_check_interval);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = manager.health_check_resources().await {
                    error!(error = %e, "Failed to perform health checks");
                }
            }
        });
    }

    /// Cleanup idle resources
    async fn cleanup_idle_resources(&self) -> Result<()> {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let resources_to_dispose = {
            let metadata_map = self.metadata.read().map_err(|e| {
                OpenAceError::new_with_context(
                    "resource",
                    format!("Failed to acquire metadata lock: {}", e),
                    ErrorContext::new("resource_management", "cleanup_idle")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::None,
                )
            })?;

            let mut resources_to_dispose = Vec::new();

            for (id, metadata_arc) in metadata_map.iter() {
                if let Ok(metadata) = metadata_arc.read() {
                    if metadata.state == ResourceState::Idle {
                        let idle_time = (current_time - metadata.usage_stats.last_accessed) / 1000; // Convert to seconds
                        
                        // Only dispose low priority resources that have been idle too long
                        if metadata.priority == ResourcePriority::Low && idle_time > 300 { // 5 minutes
                            resources_to_dispose.push(id.clone());
                        }
                    }
                }
            }

            resources_to_dispose
        };

        // Dispose idle resources
        for resource_id in resources_to_dispose {
            if let Err(e) = self.dispose_resource(&resource_id).await {
                warn!(
                    resource_id = %resource_id,
                    error = %e,
                    "Failed to dispose idle resource"
                );
            }
        }

        Ok(())
    }

    /// Perform health checks on all resources
    async fn health_check_resources(&self) -> Result<()> {
        // This would iterate through all resources and call their health_check method
        // For now, we'll just log that health checks are running
        debug!("Performing health checks on all resources");
        Ok(())
    }
}

// Implement Clone for ResourceManager to support Arc<ResourceManager>
impl Clone for ResourceManager {
    fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            metadata: self.metadata.clone(),
            pools: self.pools.clone(),
            event_sender: self.event_sender.clone(),
            memory_threshold: self.memory_threshold,
            cleanup_interval: self.cleanup_interval,
            config: self.config.clone(),
        }
    }
}

/// Resource manager statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ResourceManagerStats {
    pub total_resources: usize,
    pub active_resources: usize,
    pub idle_resources: usize,
    pub total_memory_usage: u64,
    pub memory_pressure: f64,
}

/// Global resource manager instance
static GLOBAL_RESOURCE_MANAGER: std::sync::OnceLock<Arc<ResourceManager>> = std::sync::OnceLock::new();

/// Get the global resource manager
pub fn get_global_resource_manager() -> Arc<ResourceManager> {
    GLOBAL_RESOURCE_MANAGER
        .get_or_init(|| ResourceManager::new(ManagerConfig::default()))
        .clone()
}

/// Initialize the global resource manager with custom configuration
pub fn init_global_resource_manager(config: ManagerConfig) -> Arc<ResourceManager> {
    GLOBAL_RESOURCE_MANAGER
        .get_or_init(|| ResourceManager::new(config))
        .clone()
}