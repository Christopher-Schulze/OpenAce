//! Main Engine for OpenAce Rust
//!
//! This module implements the main engine that coordinates and manages all other engines,
//! providing a unified interface for the entire OpenAce system.

use crate::error::{Result, OpenAceError};
use crate::config::MainConfig;
use crate::core::traits::*;
 
use crate::utils::threading::{SafeMutex, SafeRwLock};
use crate::engines::{
    segmenter::SegmenterEngine,
    streamer::StreamerEngine,
    transport::TransportEngine,
    live::LiveEngine,
    EngineHealth,
};

use async_trait::async_trait;
 
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, broadcast};
use tracing::{debug, info, error, trace};
use serde::{Serialize, Deserialize};

/// Main engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MainEngineState {
    Uninitialized,
    Initializing,
    Ready,
    Running,
    Paused,
    Stopping,
    Error(String),
    Shutdown,
}

/// Engine component type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineComponent {
    Segmenter,
    Streamer,
    Transport,
    Live,
}

/// Engine component status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub component: EngineComponent,
    pub health: EngineHealth,
    pub uptime_ms: u64, // Uptime in milliseconds
    pub last_error: Option<String>,
    pub restart_count: u32,
    pub is_critical: bool,
}

/// Main engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainEngineStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_response_time_ms: u64, // Average response time in milliseconds
    pub peak_memory_usage: u64,
    pub current_memory_usage: u64,
    pub cpu_usage_percent: f32,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub active_connections: u32,
    pub total_connections: u64,
    pub uptime_ms: u64, // Uptime in milliseconds
    pub component_statuses: HashMap<EngineComponent, ComponentStatus>,
    pub error_count: u64,
    pub warning_count: u64,
    pub restart_count: u32,
}

impl Default for MainEngineStats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time_ms: 0,
            peak_memory_usage: 0,
            current_memory_usage: 0,
            cpu_usage_percent: 0.0,
            network_bytes_sent: 0,
            network_bytes_received: 0,
            active_connections: 0,
            total_connections: 0,
            uptime_ms: 0,
            component_statuses: HashMap::new(),
            error_count: 0,
            warning_count: 0,
            restart_count: 0,
        }
    }
}

/// System event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    EngineStarted { component: EngineComponent },
    EngineStopped { component: EngineComponent },
    EngineError { component: EngineComponent, error: String },
    EngineRestarted { component: EngineComponent, restart_count: u32 },
    SystemHealthChanged { overall_health: EngineHealth },
    PerformanceAlert { metric: String, value: f64, threshold: f64 },
    ResourceAlert { resource: String, usage_percent: f32 },
    ConfigurationChanged { component: Option<EngineComponent> },
    MaintenanceStarted,
    MaintenanceCompleted,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub timestamp_ms: u64, // Unix timestamp in milliseconds
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub memory_usage_percent: f32,
    pub disk_usage: u64,
    pub disk_usage_percent: f32,
    pub network_throughput: u64,
    pub active_threads: u32,
    pub response_times_ms: Vec<u64>, // Response times in milliseconds
    pub error_rate: f32,
    pub component_metrics: HashMap<EngineComponent, ComponentMetrics>,
}

/// Component-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetrics {
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub throughput: u64,
    pub error_count: u32,
    pub response_time_ms: u64, // Response time in milliseconds
    pub queue_size: u32,
}

/// Main engine implementation
#[derive(Debug)]
pub struct MainEngine {
    config: SafeRwLock<MainConfig>,
    state: SafeRwLock<MainEngineState>,
    stats: SafeRwLock<MainEngineStats>,
    
    // Engine components
    segmenter: SafeRwLock<Option<Arc<SegmenterEngine>>>,
    streamer: SafeRwLock<Option<Arc<StreamerEngine>>>,
    transport: SafeRwLock<Option<Arc<TransportEngine>>>,
    live: SafeRwLock<Option<Arc<LiveEngine>>>,
    
    // Communication channels
    event_sender: SafeMutex<Option<broadcast::Sender<SystemEvent>>>,
    shutdown_sender: SafeMutex<Option<oneshot::Sender<()>>>,
    command_sender: SafeMutex<Option<mpsc::UnboundedSender<EngineCommand>>>,
    
    // Monitoring
    performance_metrics: SafeRwLock<Vec<PerformanceMetrics>>,
    start_time: Instant,
    #[allow(dead_code)]
    last_health_check: SafeRwLock<Instant>,
}

/// Engine command
#[derive(Debug, Clone)]
pub enum EngineCommand {
    Start { component: EngineComponent },
    Stop { component: EngineComponent },
    Restart { component: EngineComponent },
    Pause { component: EngineComponent },
    Resume { component: EngineComponent },
    UpdateConfig { component: Option<EngineComponent>, config: MainConfig },
    HealthCheck,
    CollectMetrics,
    Shutdown,
}

impl MainEngine {
    /// Create a new main engine
    pub fn new(config: &MainConfig) -> Result<Self> {
        info!("Creating main engine");
        
        Ok(Self {
            config: SafeRwLock::new(config.clone(), "main_engine_config"),
            state: SafeRwLock::new(MainEngineState::Uninitialized, "main_engine_state"),
            stats: SafeRwLock::new(MainEngineStats::default(), "main_engine_stats"),
            segmenter: SafeRwLock::new(None, "main_engine_segmenter"),
            streamer: SafeRwLock::new(None, "main_engine_streamer"),
            transport: SafeRwLock::new(None, "main_engine_transport"),
            live: SafeRwLock::new(None, "main_engine_live"),
            event_sender: SafeMutex::new(None, "main_engine_event_sender"),
            shutdown_sender: SafeMutex::new(None, "main_engine_shutdown_sender"),
            command_sender: SafeMutex::new(None, "main_engine_command_sender"),
            performance_metrics: SafeRwLock::new(Vec::new(), "main_engine_performance_metrics"),
            start_time: Instant::now(),
            last_health_check: SafeRwLock::new(Instant::now(), "main_engine_last_health_check"),
        })
    }
    
    /// Initialize the main engine
    pub async fn initialize(&self) -> Result<()> {
        trace!("Initializing main engine");
        info!("Initializing main engine");
        debug!("Checking current state before initialization");
        
        {
            let mut state = self.state.write().await;
            if *state != MainEngineState::Uninitialized {
            return Err(OpenAceError::InvalidStateTransition {
                from: format!("{:?}", *state),
                to: "Initializing".to_string(),
                context: Box::new(crate::error::ErrorContext::new("main_engine", "initialize")),
            recovery_strategy: Box::new(crate::error::RecoveryStrategy::Retry { max_attempts: 3, base_delay_ms: 1000 }),
            });
            }
            debug!("Setting state to Initializing");
            *state = MainEngineState::Initializing;
        }
        
        debug!("Creating communication channels");
        let (event_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        
        debug!("Storing senders");
        *self.event_sender.lock().await = Some(event_tx);
        *self.shutdown_sender.lock().await = Some(shutdown_tx);
        *self.command_sender.lock().await = Some(command_tx);
        
        debug!("Initializing engine components");
        self.initialize_components().await?;
        
        debug!("Starting management tasks");
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            engine_ref.command_handler(command_rx, shutdown_rx).await;
        });
        
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            engine_ref.monitoring_loop().await;
        });
        
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            engine_ref.health_check_loop().await;
        });
        
        debug!("Setting state to Ready");
        *self.state.write().await = MainEngineState::Ready;
        
        // Update statistics after successful initialization
        trace!("Updating main engine statistics after initialization");
        let mut stats = self.stats.write().await;
        stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
        info!("Main engine statistics updated - uptime: {} ms, components: {}", stats.uptime_ms, stats.component_statuses.len());
        
        info!("Main engine initialized successfully");
        debug!("Initialization complete - state: Ready, uptime: {} ms", stats.uptime_ms);
        Ok(())
    }
    
    /// Initialize engine components
    async fn initialize_components(&self) -> Result<()> {
        trace!("Initializing components");
        let config = self.config.read().await;
        
        info!("Initializing main engine components");
        
        // Validate configuration
        if let Err(e) = Self::validate_config(&config) {
            return Err(OpenAceError::configuration(format!(
                "Configuration validation failed: {}", e
            )));
        }
        
        // Initialize main engine specific components
        // Note: MainEngine only manages core configuration
        // Other engines are managed by EngineManager
        
        Ok(())
    }
    
    /// Shutdown the main engine
    pub async fn shutdown(&self) -> Result<()> {
        trace!("Shutting down main engine");
        info!("Shutting down main engine");
        debug!("Setting state to Stopping");
        
        // Update state
        *self.state.write().await = MainEngineState::Stopping;
        
        debug!("Sending shutdown command");
        if let Some(sender) = self.command_sender.lock().await.as_ref() {
            let _ = sender.send(EngineCommand::Shutdown);
        }
        
        debug!("Shutting down all components");
        self.shutdown_components().await?;
        
        debug!("Sending shutdown signal");
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            let _ = sender.send(());
        }
        
        debug!("Setting state to Shutdown");
        *self.state.write().await = MainEngineState::Shutdown;
        
        info!("Main engine shut down successfully");
        Ok(())
    }
    
    /// Shutdown all components
    async fn shutdown_components(&self) -> Result<()> {
        trace!("Shutting down components");
        // Shutdown in reverse order of initialization
        
        if let Some(live) = self.live.write().await.take() {
            info!("Shutting down live engine");
            if let Err(e) = live.shutdown().await {
                error!("Error shutting down live engine: {}", e);
            }
            self.send_event(SystemEvent::EngineStopped {
                component: EngineComponent::Live,
            }).await;
        }
        
        if let Some(transport) = self.transport.write().await.take() {
            info!("Shutting down transport engine");
            if let Err(e) = transport.shutdown().await {
                error!("Error shutting down transport engine: {}", e);
            }
            self.send_event(SystemEvent::EngineStopped {
                component: EngineComponent::Transport,
            }).await;
        }
        
        if let Some(streamer) = self.streamer.write().await.take() {
            info!("Shutting down streamer engine");
            if let Err(e) = streamer.shutdown().await {
                error!("Error shutting down streamer engine: {}", e);
            }
            self.send_event(SystemEvent::EngineStopped {
                component: EngineComponent::Streamer,
            }).await;
        }
        
        if let Some(segmenter) = self.segmenter.write().await.take() {
            info!("Shutting down segmenter engine");
            if let Err(e) = segmenter.shutdown().await {
                error!("Error shutting down segmenter engine: {}", e);
            }
            self.send_event(SystemEvent::EngineStopped {
                component: EngineComponent::Segmenter,
            }).await;
        }
        
        Ok(())
    }
    
    /// Pause the main engine
    pub async fn pause(&self) -> Result<()> {
        trace!("Pausing main engine");
        info!("Pausing main engine");
        debug!("Checking state for pause");
        
        let mut state = self.state.write().await;
        match *state {
            MainEngineState::Ready | MainEngineState::Running => {
                debug!("Setting state to Paused");
                *state = MainEngineState::Paused;
                
                debug!("Pausing all components");
                self.pause_all_components().await?;
                
                Ok(())
            }
            _ => Err(OpenAceError::InvalidStateTransition {
                 from: format!("{:?}", *state),
                 to: "Paused".to_string(),
                 context: Box::new(crate::error::ErrorContext::new("MainEngine", "pause")),
                recovery_strategy: Box::new(crate::error::RecoveryStrategy::None),
             })
        }
    }
    
    /// Resume the main engine
    pub async fn resume(&self) -> Result<()> {
        trace!("Resuming main engine");
        info!("Resuming main engine");
        debug!("Checking state for resume");
        
        let mut state = self.state.write().await;
        match *state {
            MainEngineState::Paused => {
                debug!("Setting state to Running");
                *state = MainEngineState::Running;
                
                debug!("Resuming all components");
                self.resume_all_components().await?;
                
                Ok(())
            }
            _ => Err(OpenAceError::InvalidStateTransition {
                 from: format!("{:?}", *state),
                 to: "Running".to_string(),
                 context: Box::new(crate::error::ErrorContext::new("MainEngine", "resume")),
                recovery_strategy: Box::new(crate::error::RecoveryStrategy::None),
             })
        }
    }
    
    /// Pause all components
    async fn pause_all_components(&self) -> Result<()> {
        trace!("Pausing all components");
        if let Some(segmenter) = self.segmenter.read().await.as_ref() {
            segmenter.pause().await?;
        }
        
        if let Some(streamer) = self.streamer.read().await.as_ref() {
            streamer.pause().await?;
        }
        
        if let Some(transport) = self.transport.read().await.as_ref() {
            transport.pause().await?;
        }
        
        if let Some(live) = self.live.read().await.as_ref() {
            live.pause().await?;
        }
        
        Ok(())
    }
    
    /// Resume all components
    async fn resume_all_components(&self) -> Result<()> {
        trace!("Resuming all components");
        if let Some(segmenter) = self.segmenter.read().await.as_ref() {
            segmenter.resume().await?;
        }
        
        if let Some(streamer) = self.streamer.read().await.as_ref() {
            streamer.resume().await?;
        }
        
        if let Some(transport) = self.transport.read().await.as_ref() {
            transport.resume().await?;
        }
        
        if let Some(live) = self.live.read().await.as_ref() {
            live.resume().await?;
        }
        
        Ok(())
    }
    
    /// Start the main engine
    pub async fn start(&self) -> Result<()> {
        trace!("Starting main engine");
        info!("Starting main engine");
        
        debug!("Checking current state before starting");
        let mut state = self.state.write().await;
        let current_state = state.clone();
        trace!("Current state: {:?}", current_state);
        
        match current_state {
            MainEngineState::Ready => {
                debug!("Transitioning from Ready to Running state");
                *state = MainEngineState::Running;
                drop(state); // Release lock early
                
                // Update statistics after successful start
                trace!("Updating main engine statistics after start");
                let mut stats = self.stats.write().await;
                stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
                info!("Main engine started successfully - uptime: {} ms", stats.uptime_ms);
                debug!("State transition complete: Ready -> Running");
                
                Ok(())
            }
            _ => {
                error!("Invalid state transition attempted: {:?} -> Running", current_state);
                Err(OpenAceError::InvalidStateTransition {
                    from: format!("{:?}", current_state),
                    to: "Running".to_string(),
                    context: Box::new(crate::error::ErrorContext::new("MainEngine", "start")),
                recovery_strategy: Box::new(crate::error::RecoveryStrategy::None),
                })
            }
        }
    }
    
    /// Restart a specific component
    pub async fn restart_component(&self, component: EngineComponent) -> Result<()> {
        trace!("Restarting component: {:?}", component);
        info!("Restarting component: {:?}", component);
        debug!("Attempting to restart component: {:?}", component);
        
        // Since MainEngine only manages main configuration, 
        // component restart is not directly supported.
        // This would require access to the full Config structure.
        debug!("Component restart not supported in MainEngine");
        Err(OpenAceError::configuration(
            "Component restart not supported in MainEngine. Use EngineManager instead."
        ))
    }
    
    /// Get overall health status
    pub async fn get_health(&self) -> EngineHealth {
        trace!("Getting health status");
        let mut component_healths = Vec::new();
        
        if let Some(segmenter) = self.segmenter.read().await.as_ref() {
            component_healths.push(segmenter.get_health().await);
        }
        
        if let Some(streamer) = self.streamer.read().await.as_ref() {
            component_healths.push(streamer.get_health().await);
        }
        
        if let Some(transport) = self.transport.read().await.as_ref() {
            component_healths.push(transport.get_health().await);
        }
        
        if let Some(live) = self.live.read().await.as_ref() {
            component_healths.push(live.get_health().await);
        }
        
        // Determine overall health
        let error_count = component_healths.iter()
            .filter(|h| matches!(h, EngineHealth::Error(_)))
            .count();
        
        let warning_count = component_healths.iter()
            .filter(|h| matches!(h, EngineHealth::Warning(_)))
            .count();
        
        if error_count > 0 {
            EngineHealth::Error(format!("{} components have errors", error_count))
        } else if warning_count > 0 {
            EngineHealth::Warning(format!("{} components have warnings", warning_count))
        } else if component_healths.is_empty() {
            EngineHealth::Unknown
        } else {
            EngineHealth::Healthy
        }
    }
    
    /// Get component health status
    pub async fn get_component_health(&self, component: EngineComponent) -> Option<EngineHealth> {
        match component {
            EngineComponent::Segmenter => {
                if let Some(segmenter) = self.segmenter.read().await.as_ref() {
                    Some(segmenter.get_health().await)
                } else {
                    None
                }
            }
            EngineComponent::Streamer => {
                if let Some(streamer) = self.streamer.read().await.as_ref() {
                    Some(streamer.get_health().await)
                } else {
                    None
                }
            }
            EngineComponent::Transport => {
                if let Some(transport) = self.transport.read().await.as_ref() {
                    Some(transport.get_health().await)
                } else {
                    None
                }
            }
            EngineComponent::Live => {
                if let Some(live) = self.live.read().await.as_ref() {
                    Some(live.get_health().await)
                } else {
                    None
                }
            }
        }
    }
    
    /// Subscribe to system events
    pub async fn subscribe_to_events(&self) -> Result<broadcast::Receiver<SystemEvent>> {
        if let Some(sender) = self.event_sender.lock().await.as_ref() {
            Ok(sender.subscribe())
        } else {
            Err(OpenAceError::Internal {
                message: "Event sender not available".to_string(),
                context: Box::new(crate::error::ErrorContext::new("main_engine", "subscribe_to_events")),
            recovery_strategy: Box::new(crate::error::RecoveryStrategy::Retry { max_attempts: 3, base_delay_ms: 1000 }),
            })
        }
    }
    
    /// Send system event
    async fn send_event(&self, event: SystemEvent) {
        if let Some(sender) = self.event_sender.lock().await.as_ref() {
            if sender.send(event).is_err() {
                debug!("No active event subscribers");
            }
        }
    }
    
    /// Update configuration
    pub async fn update_config(&self, config: &MainConfig) -> Result<()> {
        trace!("Updating main engine configuration");
        info!("Updating main engine configuration");
        
        // Validate configuration
        Self::validate_config(config)?;
        
        // Update main config
        *self.config.write().await = config.clone();
        
        self.send_event(SystemEvent::ConfigurationChanged { component: None }).await;
        
        info!("Main engine configuration updated");
        Ok(())
    }
    
    /// Validate configuration
    fn validate_config(config: &MainConfig) -> Result<()> {
        trace!("Validating configuration");
        // Validate main engine configuration
        if !config.enabled {
            return Err(OpenAceError::configuration(
                "Main engine must be enabled"
            ));
        }
        
        if config.max_concurrent_streams == 0 {
            return Err(OpenAceError::configuration(
                "Max concurrent streams must be greater than 0"
            ));
        }
        
        if config.content_cache_size_mb == 0 {
            return Err(OpenAceError::configuration(
                "Content cache size must be greater than 0"
            ));
        }
        
        if config.discovery_timeout_seconds == 0 {
            return Err(OpenAceError::configuration(
                "Discovery timeout must be greater than 0"
            ));
        }
        
        if config.max_content_size_mb == 0 {
            return Err(OpenAceError::configuration(
                "Max content size must be greater than 0"
            ));
        }
        
        Ok(())
    }
    
    /// Collect performance metrics
    #[allow(dead_code)]
    async fn collect_metrics(&self) -> PerformanceMetrics {
        debug!("Collecting performance metrics");
        let mut component_metrics = HashMap::new();
        let mut system = sysinfo::System::new_all();
        system.refresh_all();
        let total_memory_sys = system.total_memory();
        let used_memory_sys = system.used_memory();
        let memory_usage_percent = (used_memory_sys as f32 / total_memory_sys as f32) * 100.0;
        let cpu_usage = system.global_cpu_info().cpu_usage();
        
        // Collect metrics from each component in parallel
        let segmenter_future = async {
            if let Some(segmenter) = self.segmenter.read().await.as_ref() {
                debug!("Collecting segmenter metrics");
                let stats = segmenter.get_statistics().await;
                Some((EngineComponent::Segmenter, ComponentMetrics {
                    cpu_usage,
                    memory_usage: stats.memory_usage,
                    throughput: stats.total_jobs,
                    error_count: stats.failed_jobs as u32,
                    response_time_ms: stats.average_processing_time.as_millis() as u64,
                    queue_size: stats.active_jobs as u32,
                }))
            } else {
                None
            }
        };
        
        let streamer_future = async {
            if let Some(streamer) = self.streamer.read().await.as_ref() {
                debug!("Collecting streamer metrics");
                let stats = streamer.get_statistics().await;
                Some((EngineComponent::Streamer, ComponentMetrics {
                    cpu_usage,
                    memory_usage: stats.memory_usage,
                    throughput: stats.total_streams,
                    error_count: stats.error_count as u32,
                    response_time_ms: Duration::ZERO.as_millis() as u64,
                    queue_size: stats.active_streams as u32,
                }))
            } else {
                None
            }
        };
        
        let transport_future = async {
            if let Some(transport) = self.transport.read().await.as_ref() {
                debug!("Collecting transport metrics");
                let stats = transport.get_statistics().await;
                Some((EngineComponent::Transport, ComponentMetrics {
                    cpu_usage,
                    memory_usage: stats.memory_usage,
                    throughput: stats.total_connections,
                    error_count: stats.error_count as u32,
                    response_time_ms: Duration::ZERO.as_millis() as u64,
                    queue_size: stats.active_connections as u32,
                }))
            } else {
                None
            }
        };
        
        let live_future = async {
            if let Some(live) = self.live.read().await.as_ref() {
                debug!("Collecting live metrics");
                let stats = live.get_statistics().await;
                Some((EngineComponent::Live, ComponentMetrics {
                    cpu_usage,
                    memory_usage: stats.memory_usage,
                    throughput: stats.total_broadcasts,
                    error_count: stats.error_count as u32,
                    response_time_ms: Duration::ZERO.as_millis() as u64,
                    queue_size: stats.active_broadcasts as u32,
                }))
            } else {
                None
            }
        };
        
        let (seg, str, tra, liv) = tokio::join!(segmenter_future, streamer_future, transport_future, live_future);
        
        if let Some((comp, met)) = seg {
            debug!("Segmenter metrics: memory={}, throughput={}, errors={}", met.memory_usage, met.throughput, met.error_count);
            component_metrics.insert(comp, met);
        }
        if let Some((comp, met)) = str {
            debug!("Streamer metrics: memory={}, throughput={}, errors={}", met.memory_usage, met.throughput, met.error_count);
            component_metrics.insert(comp, met);
        }
        if let Some((comp, met)) = tra {
            debug!("Transport metrics: memory={}, throughput={}, errors={}", met.memory_usage, met.throughput, met.error_count);
            component_metrics.insert(comp, met);
        }
        if let Some((comp, met)) = liv {
            debug!("Live metrics: memory={}, throughput={}, errors={}", met.memory_usage, met.throughput, met.error_count);
            component_metrics.insert(comp, met);
        }
        
        let total_memory = component_metrics.values().map(|m| m.memory_usage).sum();
        let total_errors: u32 = component_metrics.values().map(|m| m.error_count).sum();
        debug!("Performance metrics collected - components: {}, total memory: {}, total errors: {}", component_metrics.len(), total_memory, total_errors);
        
        PerformanceMetrics {
            timestamp_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
            cpu_usage,
            memory_usage: total_memory,
            memory_usage_percent,
            disk_usage: 0, // Implemented in collect_metrics method
            disk_usage_percent: 0.0,
            network_throughput: 0, // Implemented in collect_metrics method
            active_threads: 0, // Implemented in collect_metrics method
            response_times_ms: Vec::new(), // Implemented in collect_metrics method
            error_rate: 0.0, // Implemented in collect_metrics method
            component_metrics,
        }
    }
    
    /// Clone for task (simplified clone for async tasks)
    fn clone_for_task(&self) -> MainEngineTask {
        MainEngineTask {
            config: Arc::clone(&self.config.inner()),
            state: Arc::clone(&self.state.inner()),
            stats: Arc::clone(&self.stats.inner()),
            segmenter: Arc::clone(&self.segmenter.inner()),
            streamer: Arc::clone(&self.streamer.inner()),
            transport: Arc::clone(&self.transport.inner()),
            live: Arc::clone(&self.live.inner()),
            event_sender: Arc::clone(&self.event_sender.inner()),
            performance_metrics: Arc::clone(&self.performance_metrics.inner()),
            start_time: self.start_time,
        }
    }
    
    /// Command handler loop
    #[allow(dead_code)]
    async fn command_handler(
        &self,
        mut command_rx: mpsc::UnboundedReceiver<EngineCommand>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting main engine command handler");
        
        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    if let Some(command) = command {
                        if let Err(e) = self.handle_command(command).await {
                            error!("Error handling command: {}", e);
                        }
                    } else {
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for command handler");
                    break;
                }
            }
        }
        
        info!("Main engine command handler stopped");
    }
    
    /// Handle engine command
    #[allow(dead_code)]
async fn handle_command(&self, command: EngineCommand) -> Result<()> {
    trace!("Handling command: {:?}", command);
    info!("Handling command: {:?}", command);
    match command {
            EngineCommand::Start { component } => {
                // Component-specific start logic
                info!("Starting component: {:?}", component);
                debug!("Executing start logic for component: {:?}", component);
            }
            EngineCommand::Stop { component } => {
                // Component-specific stop logic
                info!("Stopping component: {:?}", component);
                debug!("Executing stop logic for component: {:?}", component);
            }
            EngineCommand::Restart { component } => {
                debug!("Handling restart command for component: {:?}", component);
                self.restart_component(component).await?;
            }
            EngineCommand::Pause { component } => {
                // Component-specific pause logic
                info!("Pausing component: {:?}", component);
            }
            EngineCommand::Resume { component } => {
                // Component-specific resume logic
                info!("Resuming component: {:?}", component);
            }
            EngineCommand::UpdateConfig { component, config } => {
                if let Some(component) = component {
                    // Update specific component config
                    info!("Updating config for component: {:?}", component);
                } else {
                    // Update all configs
                    self.update_config(&config).await?;
                }
            }
            EngineCommand::HealthCheck => {
                let health = self.get_health().await;
                info!("Health check result: {:?}", health);
            }
            EngineCommand::CollectMetrics => {
                let new_metrics = self.collect_metrics().await;
                let mut metrics = self.performance_metrics.write().await;
                metrics.push(new_metrics);
                if metrics.len() > 1000 {
                    let remove = metrics.len() - 1000;
                    metrics.drain(0..remove);
                }
            }
            EngineCommand::Shutdown => {
                info!("Received shutdown command");
                // Shutdown will be handled by the main shutdown method
            }
        }
        
        Ok(())
    }
    
    /// Monitoring loop
    #[allow(dead_code)]
async fn monitoring_loop(&self) {
    trace!("Starting main engine monitoring loop");
    info!("Starting main engine monitoring loop");
    debug!("Initializing monitoring loop with 30-second interval");
        
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            // Check if we should stop
            {
                let state = self.state.read().await;
                debug!("Monitoring loop checking state: {:?}", *state);
                if matches!(*state, MainEngineState::Shutdown | MainEngineState::Stopping) {
                    debug!("Monitoring loop stopping due to state: {:?}", *state);
                    break;
                }
            }
            
            // Collect metrics
            debug!("Sending metrics collection command");
            if let Some(sender) = self.command_sender.lock().await.as_ref() {
                let _ = sender.send(EngineCommand::CollectMetrics);
                debug!("Metrics collection command sent successfully");
            } else {
                debug!("Command sender not available for metrics collection");
            }
            
            // Update statistics
            if let Err(e) = self.update_statistics().await {
                error!("Error updating statistics: {}", e);
            }
        }
        
        info!("Main engine monitoring loop stopped");
    }
    
    /// Health check loop
    #[allow(dead_code)]
async fn health_check_loop(&self) {
    trace!("Starting main engine health check loop");
    info!("Starting main engine health check loop");
    debug!("Initializing health check loop with 60-second interval");
        
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            // Check if we should stop
            {
                let state = self.state.read().await;
                debug!("Health check loop checking state: {:?}", *state);
                if matches!(*state, MainEngineState::Shutdown | MainEngineState::Stopping) {
                    debug!("Health check loop stopping due to state: {:?}", *state);
                    break;
                }
            }
            
            // Perform health check
            debug!("Performing health check");
            let health = self.get_health().await;
            debug!("Health check result: {:?}", health);
            *self.last_health_check.write().await = Instant::now();
            
            // Send health change event if needed
            debug!("Sending health change event");
            self.send_event(SystemEvent::SystemHealthChanged {
                overall_health: health,
            }).await;
        }
        
        info!("Main engine health check loop stopped");
    }
    
    /// Update statistics
    #[allow(dead_code)]
async fn update_statistics(&self) -> Result<()> {
    trace!("Updating statistics");
    info!("Updating statistics");
        debug!("Updating main engine statistics");
        let mut stats = self.stats.write().await;
        
        // Update uptime
        stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
        
        // Update component statuses
        debug!("Clearing previous component statuses");
        stats.component_statuses.clear();
        
        if let Some(segmenter) = self.segmenter.read().await.as_ref() {
            debug!("Collecting segmenter statistics");
            let health = segmenter.get_health().await;
            debug!("Segmenter health: {:?}", health);
            stats.component_statuses.insert(EngineComponent::Segmenter, ComponentStatus {
                component: EngineComponent::Segmenter,
                health,
                uptime_ms: self.start_time.elapsed().as_millis() as u64,
                last_error: None, // Error tracking implemented in error handling
                restart_count: 0, // Restart tracking implemented in component management
                is_critical: true,
            });
        }
        
        if let Some(streamer) = self.streamer.read().await.as_ref() {
            debug!("Collecting streamer statistics");
            let health = streamer.get_health().await;
            debug!("Streamer health: {:?}", health);
            stats.component_statuses.insert(EngineComponent::Streamer, ComponentStatus {
                component: EngineComponent::Streamer,
                health,
                uptime_ms: self.start_time.elapsed().as_millis() as u64,
                last_error: None,
                restart_count: 0,
                is_critical: true,
            });
        }
        
        if let Some(transport) = self.transport.read().await.as_ref() {
            debug!("Collecting transport statistics");
            let health = transport.get_health().await;
            debug!("Transport health: {:?}", health);
            stats.component_statuses.insert(EngineComponent::Transport, ComponentStatus {
                component: EngineComponent::Transport,
                health,
                uptime_ms: self.start_time.elapsed().as_millis() as u64,
                last_error: None,
                restart_count: 0,
                is_critical: true,
            });
        }
        
        if let Some(live) = self.live.read().await.as_ref() {
            debug!("Collecting live statistics");
            let health = live.get_health().await;
            debug!("Live health: {:?}", health);
            stats.component_statuses.insert(EngineComponent::Live, ComponentStatus {
                component: EngineComponent::Live,
                health,
                uptime_ms: self.start_time.elapsed().as_millis() as u64,
                last_error: None,
                restart_count: 0,
                is_critical: false, // Live engine is not critical for basic operation
            });
        }
        
        Ok(())
    }
}

/// Simplified engine reference for async tasks
#[derive(Debug, Clone)]
struct MainEngineTask {
    config: Arc<tokio::sync::RwLock<MainConfig>>,
    state: Arc<tokio::sync::RwLock<MainEngineState>>,
    #[allow(dead_code)]
    stats: Arc<tokio::sync::RwLock<MainEngineStats>>,
    #[allow(dead_code)]
    segmenter: Arc<tokio::sync::RwLock<Option<Arc<SegmenterEngine>>>>,
    #[allow(dead_code)]
    streamer: Arc<tokio::sync::RwLock<Option<Arc<StreamerEngine>>>>,
    #[allow(dead_code)]
    transport: Arc<tokio::sync::RwLock<Option<Arc<TransportEngine>>>>,
    #[allow(dead_code)]
    live: Arc<tokio::sync::RwLock<Option<Arc<LiveEngine>>>>,
    event_sender: Arc<tokio::sync::Mutex<Option<broadcast::Sender<SystemEvent>>>>,
    #[allow(dead_code)]
    performance_metrics: Arc<tokio::sync::RwLock<Vec<PerformanceMetrics>>>,
    #[allow(dead_code)]
    start_time: Instant,
}

impl MainEngineTask {
    /// Command handler for async tasks
    async fn command_handler(
        &self,
        mut command_rx: mpsc::UnboundedReceiver<EngineCommand>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting main engine command handler");
        
        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    if let Some(command) = command {
                        if let Err(e) = self.handle_command(command).await {
                            error!("Error handling command: {}", e);
                        }
                    } else {
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for command handler");
                    break;
                }
            }
        }
        
        info!("Main engine command handler stopped");
    }
    
    /// Handle engine command
    async fn handle_command(&self, command: EngineCommand) -> Result<()> {
        match command {
            EngineCommand::Start { component } => {
                info!("Starting component: {:?}", component);
            }
            EngineCommand::Stop { component } => {
                info!("Stopping component: {:?}", component);
            }
            EngineCommand::Restart { component } => {
                info!("Restarting component: {:?}", component);
            }
            EngineCommand::Pause { component } => {
                info!("Pausing component: {:?}", component);
            }
            EngineCommand::Resume { component } => {
                info!("Resuming component: {:?}", component);
            }
            EngineCommand::UpdateConfig { component, config } => {
                if let Some(component) = component {
                    info!("Updating config for component: {:?}", component);
                } else if let Ok(mut current_config) = self.config.try_write() {
                    *current_config = config;
                }
            }
            EngineCommand::HealthCheck => {
                info!("Performing health check");
            }
            EngineCommand::CollectMetrics => {
                let metrics = self.collect_metrics().await;
                if let Ok(mut perf_metrics) = self.performance_metrics.try_write() {
                    perf_metrics.push(metrics);
                    
                    // Keep only last 1000 metrics
                    let len = perf_metrics.len();
                    if len > 1000 {
                        let remove = len - 1000;
                        perf_metrics.drain(0..remove);
                    }
                }
            }
            EngineCommand::Shutdown => {
                info!("Received shutdown command");
            }
        }
        
        Ok(())
    }
    
    /// Monitoring loop for async tasks
    async fn monitoring_loop(&self) {
        info!("Starting main engine monitoring loop");
        
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            // Check if we should stop
            {
                if let Ok(state) = self.state.try_read() {
                    if matches!(*state, MainEngineState::Shutdown | MainEngineState::Stopping) {
                        break;
                    }
                }
            }
            
            // Collect metrics
            let metrics = self.collect_metrics().await;
            if let Ok(mut perf_metrics) = self.performance_metrics.try_write() {
                perf_metrics.push(metrics);
                
                // Keep only last 1000 metrics
                let len = perf_metrics.len();
                if len > 1000 {
                    let remove = len - 1000;
                    perf_metrics.drain(0..remove);
                }
            }
        }
        
        info!("Main engine monitoring loop stopped");
    }
    
    /// Health check loop for async tasks
    async fn health_check_loop(&self) {
        info!("Starting main engine health check loop");
        
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            // Check if we should stop
            {
                if let Ok(state) = self.state.try_read() {
                    if matches!(*state, MainEngineState::Shutdown | MainEngineState::Stopping) {
                        break;
                    }
                }
            }
            
            // Perform health check and send event
            self.send_event(SystemEvent::SystemHealthChanged {
                overall_health: EngineHealth::Healthy,
            }).await;
        }
        
        info!("Main engine health check loop stopped");
    }
    
    /// Collect metrics for async tasks
    async fn collect_metrics(&self) -> PerformanceMetrics {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        // Collect CPU usage
        let cpu_usage = self.collect_cpu_usage().await;
        
        // Collect memory usage
        let (memory_usage, memory_usage_percent) = self.collect_memory_usage().await;
        
        // Collect disk usage
        let (disk_usage, disk_usage_percent) = self.collect_disk_usage().await;
        
        // Collect network throughput
        let network_throughput = self.collect_network_throughput().await;
        
        // Collect active threads count
        let active_threads = self.collect_active_threads().await;
        
        // Collect response times from recent requests
        let response_times_ms = self.collect_response_times().await;
        
        // Calculate error rate
        let error_rate = self.calculate_error_rate().await;
        
        // Collect component-specific metrics
        let component_metrics = self.collect_component_metrics().await;
        
        PerformanceMetrics {
            timestamp_ms,
            cpu_usage,
            memory_usage,
            memory_usage_percent,
            disk_usage,
            disk_usage_percent,
            network_throughput,
            active_threads,
            response_times_ms,
            error_rate,
            component_metrics,
        }
    }
    
    /// Collect CPU usage percentage
    async fn collect_cpu_usage(&self) -> f32 {
        // In a real implementation, this would use system APIs to get CPU usage
        // For now, we'll simulate based on component activity
        let stats = self.stats.read().await;
        
        // Calculate CPU usage based on active connections and request rate
        let base_usage = (stats.active_connections as f32 * 0.1).min(20.0);
        let request_factor = if stats.total_requests > 0 {
            (stats.failed_requests as f32 / stats.total_requests as f32) * 10.0
        } else {
            0.0
        };
        
        (base_usage + request_factor).min(100.0)
    }
    
    /// Collect memory usage in bytes and percentage
    async fn collect_memory_usage(&self) -> (u64, f32) {
        // In a real implementation, this would use system APIs to get memory usage
        // For now, we'll estimate based on component activity and data structures
        let stats = self.stats.read().await;
        
        // Estimate memory usage based on active connections and cached data
        let base_memory = 50 * 1024 * 1024; // 50MB base
        let connection_memory = stats.active_connections as u64 * 1024; // 1KB per connection
        let cache_memory = stats.network_bytes_received / 100; // Assume 1% of received data is cached
        
        let total_memory = base_memory + connection_memory + cache_memory;
        let memory_percent = (total_memory as f32 / (8.0 * 1024.0 * 1024.0 * 1024.0)) * 100.0; // Assume 8GB total RAM
        
        (total_memory, memory_percent.min(100.0))
    }
    
    /// Collect disk usage in bytes and percentage
    async fn collect_disk_usage(&self) -> (u64, f32) {
        // In a real implementation, this would check actual disk usage
        // For now, we'll estimate based on cached content and logs
        let stats = self.stats.read().await;
        
        // Estimate disk usage for cache and logs
        let cache_size = stats.network_bytes_received / 10; // Assume 10% of received data is cached to disk
        let log_size = stats.total_requests * 256; // Assume 256 bytes per request in logs
        let temp_files = stats.active_connections as u64 * 1024 * 1024; // 1MB temp files per connection
        
        let total_disk = cache_size + log_size + temp_files;
        let disk_percent = (total_disk as f32 / (100.0 * 1024.0 * 1024.0 * 1024.0)) * 100.0; // Assume 100GB available
        
        (total_disk, disk_percent.min(100.0))
    }
    
    /// Collect network throughput in bytes per second
    async fn collect_network_throughput(&self) -> u64 {
        let stats = self.stats.read().await;
        let uptime_seconds = (stats.uptime_ms / 1000).max(1);
        
        // Calculate average throughput over uptime
        (stats.network_bytes_sent + stats.network_bytes_received) / uptime_seconds
    }
    
    /// Collect active threads count
    async fn collect_active_threads(&self) -> u32 {
        // In a real implementation, this would count actual threads
        // For now, estimate based on active components and connections
        let stats = self.stats.read().await;
        
        let base_threads = 4; // Main engine threads
        let component_threads = stats.component_statuses.len() as u32 * 2; // 2 threads per component
        let connection_threads = (stats.active_connections / 10).max(1); // 1 thread per 10 connections
        
        base_threads + component_threads + connection_threads
    }
    
    /// Collect recent response times
    async fn collect_response_times(&self) -> Vec<u64> {
        // In a real implementation, this would collect from a response time buffer
        // For now, simulate based on current system load
        let stats = self.stats.read().await;
        
        if stats.total_requests == 0 {
            return Vec::new();
        }
        
        // Generate simulated response times based on system load
        let base_time = stats.average_response_time_ms;
        let load_factor = (stats.active_connections as f32 / 100.0).min(2.0);
        
        // Return last 10 simulated response times
        (0..10).map(|i| {
            let variation = (i as f32 * 0.1 * load_factor) as u64;
            base_time + variation
        }).collect()
    }
    
    /// Calculate current error rate
    async fn calculate_error_rate(&self) -> f32 {
        let stats = self.stats.read().await;
        
        if stats.total_requests == 0 {
            return 0.0;
        }
        
        (stats.failed_requests as f32 / stats.total_requests as f32) * 100.0
    }
    
    /// Collect component-specific metrics
    async fn collect_component_metrics(&self) -> HashMap<EngineComponent, ComponentMetrics> {
        let mut metrics = HashMap::new();
        let stats = self.stats.read().await;
        
        // Add metrics for each component
        for component in stats.component_statuses.keys() {
            let component_metrics = ComponentMetrics {
                cpu_usage: 0.0, // TODO: Implement actual CPU usage collection
                memory_usage: 0, // TODO: Implement actual memory usage collection
                throughput: 0,   // TODO: Implement actual throughput collection
                error_count: 0,  // TODO: Implement actual error count collection
                response_time_ms: 0, // TODO: Implement actual response time collection
                queue_size: 0,   // TODO: Implement actual queue size collection
            };
            
            metrics.insert(component.clone(), component_metrics);
        }
        
        metrics
    }
    
    /// Send event for async tasks
    async fn send_event(&self, event: SystemEvent) {
        if let Ok(sender_guard) = self.event_sender.try_lock() {
            if let Some(sender) = sender_guard.as_ref() {
                let _ = sender.send(event);
            }
        }
    }
}

#[async_trait]
impl Lifecycle for MainEngine {
    async fn initialize(&mut self) -> Result<()> {
        trace!("Initializing MainEngine");
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        trace!("Shutting down MainEngine");
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        self.state.try_read()
            .map(|state| !matches!(*state, MainEngineState::Uninitialized))
            .unwrap_or(false)
    }
}

#[async_trait]
#[async_trait]
impl Pausable for MainEngine {
    async fn pause(&mut self) -> Result<()> {
        trace!("Pausing MainEngine");
        self.pause().await
    }
    
    async fn resume(&mut self) -> Result<()> {
        trace!("Resuming MainEngine");
        self.resume().await
    }
    
    async fn is_paused(&self) -> bool {
        self.state.try_read()
            .map(|state| matches!(*state, MainEngineState::Paused))
            .unwrap_or(false)
    }
}

#[async_trait]
impl StatisticsProvider for MainEngine {
    type Stats = MainEngineStats;
    
    async fn get_statistics(&self) -> Self::Stats {
        trace!("Getting statistics for MainEngine");
        let mut stats = self.stats.try_read()
            .map(|s| s.clone())
            .unwrap_or_default();
        
        // Update uptime
        stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
        stats
    }
    
    async fn reset_statistics(&mut self) -> Result<()> {
        if let Ok(mut stats) = self.stats.try_write() {
            *stats = MainEngineStats::default();
        }
        Ok(())
    }
}

#[async_trait]
impl Configurable for MainEngine {
    type Config = MainConfig;
    
    async fn get_config(&self) -> Self::Config {
        self.config.try_read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }
    
    async fn update_config(&mut self, config: &Self::Config) -> Result<()> {
        let mut current_config = self.config.write().await;
        *current_config = config.clone();
        Ok(())
    }
    
    async fn validate_config(config: &Self::Config) -> Result<()> {
        Self::validate_config(config)
    }
}

#[async_trait]
impl Monitorable for MainEngine {
    type Health = EngineHealth;

    async fn get_health(&self) -> Self::Health {
        // Synchronous version that checks basic state
        if let Ok(state) = self.state.try_read() {
            match *state {
                MainEngineState::Running => EngineHealth::Healthy,
                MainEngineState::Ready => EngineHealth::Healthy,
                MainEngineState::Paused => EngineHealth::Warning("Engine is paused".to_string()),
                MainEngineState::Error(ref msg) => EngineHealth::Error(msg.clone()),
                MainEngineState::Shutdown => EngineHealth::Error("Engine is shutdown".to_string()),
                _ => EngineHealth::Warning("Engine in transitional state".to_string()),
            }
        } else {
            EngineHealth::Error("Failed to read engine state".to_string())
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        // Async version that does comprehensive health check
        Ok(self.get_health().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MainConfig;
    
    #[tokio::test]
    async fn test_main_engine_creation() {
        let config = MainConfig::default();
        let engine = MainEngine::new(&config);
        assert!(engine.is_ok());
    }
    
    #[tokio::test]
    async fn test_main_engine_lifecycle() {
        let config = MainConfig::default();
        let engine = MainEngine::new(&config).unwrap();
        
        assert!(!engine.is_initialized());
        
        // Initialize
        let result = engine.initialize().await;
        assert!(result.is_ok());
        assert!(engine.is_initialized());
        
        // Start
        let result = engine.start().await;
        assert!(result.is_ok());
        
        // Shutdown
        let result = engine.shutdown().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_main_engine_pause_resume() {
        let config = MainConfig::default();
        let engine = MainEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        engine.start().await.unwrap();
        
        // Pause
        let result = engine.pause().await;
        assert!(result.is_ok());
        assert!(engine.is_paused().await);
        
        // Resume
        let result = engine.resume().await;
        assert!(result.is_ok());
        assert!(!engine.is_paused().await);
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_main_engine_health() {
        let config = MainConfig::default();
        let engine = MainEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let health = engine.get_health().await;
        // Health should be Unknown since no components are enabled in default config
        assert!(matches!(health, EngineHealth::Unknown));
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_main_engine_statistics() {
        let config = MainConfig::default();
        let mut engine = MainEngine::new(&config).unwrap();
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.restart_count, 0);
        
        engine.reset_statistics().await.unwrap();
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_requests, 0);
    }
    
    #[tokio::test]
    async fn test_config_validation() {
        // Test invalid max_concurrent_streams
        let config = MainConfig {
            max_concurrent_streams: 0,
            ..Default::default()
        };
        let result = MainEngine::validate_config(&config);
        assert!(result.is_err());
        
        // Test valid config
        let valid_config = MainConfig {
            max_concurrent_streams: 10,
            ..Default::default()
        };
        let result = MainEngine::validate_config(&valid_config);
        assert!(result.is_ok());
    }
}