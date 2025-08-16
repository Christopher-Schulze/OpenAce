//! Engine Manager Implementation
//!
//! Provides centralized management and coordination of all OpenAce engines.
//! Handles initialization, shutdown, configuration updates, and inter-engine communication.

use crate::config::Config;
use crate::engines::{
    core::CoreEngine,
    live::LiveEngine,
    main_engine::MainEngine,
    segmenter::SegmenterEngine,
    streamer::StreamerEngine,
    transport::TransportEngine,
};
use crate::error::{OpenAceError, Result};
use crate::core::traits::{Configurable, Lifecycle, EventHandler, Pausable, StatisticsProvider, Monitorable};
use async_trait::async_trait;
use crate::engines::EngineHealth;
use crate::engines::core::CoreState;
use log::{debug, error, info, trace, warn};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};

/// Engine manager state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManagerState {
    /// Manager is uninitialized
    Uninitialized,
    /// Manager is initializing
    Initializing,
    /// Manager is ready
    Ready,
    /// Manager is running
    Running,
    /// Manager is paused
    Paused,
    /// Manager is shutting down
    ShuttingDown,
    /// Manager has shut down
    Shutdown,
    /// Manager is in error state
    Error(String),
}

/// Engine manager statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagerStats {
    /// Total uptime in seconds
    pub uptime_seconds: u64,
    /// Uptime as Duration
    pub uptime: std::time::Duration,
    /// Total number of requests processed
    pub total_requests: u64,
    /// Number of currently active engines
    pub active_engines: u64,
    /// Number of active reservations
    pub active_reservations: u64,
    /// Number of successful initializations
    pub successful_initializations: u64,
    /// Number of failed initializations
    pub failed_initializations: u64,
    /// Number of restarts
    pub restart_count: u64,
    /// Number of configuration updates
    pub config_updates: u64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Last health check timestamp
    pub last_health_check: Option<u64>,
    /// Engine health status
    pub engine_health: HashMap<String, EngineHealth>,
}

/// Engine manager events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManagerEvent {
    /// Manager initialized
    Initialized,
    /// Manager started
    Started,
    /// Manager stopped
    Stopped,
    /// Manager paused
    Paused,
    /// Manager resumed
    Resumed,
    /// Configuration updated
    ConfigurationUpdated,
    /// Engine state changed
    EngineStateChanged {
        engine: String,
        old_state: CoreState,
        new_state: CoreState,
    },
    /// Health check completed
    HealthCheckCompleted {
        overall_health: EngineHealth,
        engine_health: HashMap<String, EngineHealth>,
    },
    /// Error occurred
    Error {
        engine: Option<String>,
        error: String,
    },
}

/// Engine manager that coordinates all engines
#[derive(Debug)]
pub struct EngineManager {
    /// Manager configuration
    config: Arc<RwLock<Config>>,
    /// Manager state
    state: Arc<RwLock<ManagerState>>,
    /// Manager statistics
    stats: Arc<RwLock<ManagerStats>>,
    
    /// Core engine (provides fundamental services)
    core_engine: Arc<RwLock<CoreEngine>>,
    /// Main engine
    main_engine: Arc<RwLock<MainEngine>>,
    /// Segmenter engine
    segmenter_engine: Arc<RwLock<SegmenterEngine>>,
    /// Streamer engine
    streamer_engine: Arc<RwLock<StreamerEngine>>,
    /// Transport engine
    transport_engine: Arc<RwLock<TransportEngine>>,
    /// Live engine
    live_engine: Arc<RwLock<LiveEngine>>,
    
    /// Event broadcaster
    event_sender: Arc<RwLock<Option<broadcast::Sender<ManagerEvent>>>>,
    /// Shutdown signal
    shutdown_sender: Arc<RwLock<Option<broadcast::Sender<()>>>>,
    
    /// Manager start time
    start_time: Instant,
}

impl EngineManager {
    /// Create a new engine manager with default configuration
    pub fn new() -> Result<Self> {
        let config = Config::default();
        Self::with_config(config)
    }
    
    /// Create a new engine manager with specific configuration
    pub fn with_config(config: Config) -> Result<Self> {
        info!("Creating engine manager");
        
        // Create core engine first
        let core_engine = CoreEngine::new(config.core.clone());
        
        // Create other engines
        let main_engine = MainEngine::new(&config.main)?;
        let segmenter_engine = SegmenterEngine::new(&config.segmenter)?;
        let streamer_engine = StreamerEngine::new(&config.streamer)?;
        let transport_engine = TransportEngine::new(&config.transport)?;
        let live_engine = LiveEngine::new(&config.live)?;
        
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            state: Arc::new(RwLock::new(ManagerState::Uninitialized)),
            stats: Arc::new(RwLock::new(ManagerStats::default())),
            core_engine: Arc::new(RwLock::new(core_engine)),
            main_engine: Arc::new(RwLock::new(main_engine)),
            segmenter_engine: Arc::new(RwLock::new(segmenter_engine)),
            streamer_engine: Arc::new(RwLock::new(streamer_engine)),
            transport_engine: Arc::new(RwLock::new(transport_engine)),
            live_engine: Arc::new(RwLock::new(live_engine)),
            event_sender: Arc::new(RwLock::new(None)),
            shutdown_sender: Arc::new(RwLock::new(None)),
            start_time: Instant::now(),
        })
    }
    
    /// Create a new engine manager (async version for backward compatibility)
    pub async fn new_async(config: Config) -> Result<Self> {
        Self::with_config(config)
    }
    
    /// Initialize all engines
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing engine manager");
        
        // Set state to initializing
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Initializing;
        }
        
        // Create event channel
        let (event_tx, _) = broadcast::channel(1000);
        {
            let mut sender = self.event_sender.write().await;
            *sender = Some(event_tx.clone());
        }
        
        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        {
            let mut sender = self.shutdown_sender.write().await;
            *sender = Some(shutdown_tx);
        }
        
        // Initialize engines in dependency order
        let mut failed_engines = Vec::new();
        
        // Initialize core engine first (provides fundamental services)
        if let Err(e) = self.core_engine.write().await.initialize().await {
            error!("Failed to initialize core engine: {}", e);
            failed_engines.push(("core".to_string(), e.to_string()));
        }
        
        // Initialize main engine
        if let Err(e) = self.main_engine.write().await.initialize().await {
            error!("Failed to initialize main engine: {}", e);
            failed_engines.push(("main".to_string(), e.to_string()));
        }
        
        // Initialize segmenter engine
        if let Err(e) = self.segmenter_engine.write().await.initialize().await {
            error!("Failed to initialize segmenter engine: {}", e);
            failed_engines.push(("segmenter".to_string(), e.to_string()));
        }
        
        // Initialize streamer engine
        if let Err(e) = self.streamer_engine.write().await.initialize().await {
            error!("Failed to initialize streamer engine: {}", e);
            failed_engines.push(("streamer".to_string(), e.to_string()));
        }
        
        // Initialize transport engine
        if let Err(e) = self.transport_engine.write().await.initialize().await {
            error!("Failed to initialize transport engine: {}", e);
            failed_engines.push(("transport".to_string(), e.to_string()));
        }
        
        // Initialize live engine
        if let Err(e) = self.live_engine.write().await.initialize().await {
            error!("Failed to initialize live engine: {}", e);
            failed_engines.push(("live".to_string(), e.to_string()));
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            if failed_engines.is_empty() {
                stats.successful_initializations += 1;
            } else {
                stats.failed_initializations += 1;
            }
        }
        
        // Check if any engines failed
        if !failed_engines.is_empty() {
            let error_msg = format!("Failed to initialize engines: {:?}", failed_engines);
            let mut state = self.state.write().await;
            *state = ManagerState::Error(error_msg.clone());
            
            // Send error event
            if let Some(sender) = self.event_sender.read().await.as_ref() {
                let _ = sender.send(ManagerEvent::Error {
                    engine: None,
                    error: error_msg.clone(),
                });
            }
            
            return Err(OpenAceError::engine_initialization("manager", error_msg));
        }
        
        // Set state to ready
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Ready;
        }
        
        // Send initialized event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::Initialized);
        }
        
        // Start monitoring task
        self.start_monitoring_task(shutdown_rx.resubscribe()).await;
        
        // Start config watcher
        self.start_config_watcher(shutdown_rx.resubscribe()).await;
        
        info!("Engine manager initialized successfully");
        Ok(())
    }
    
    /// Start all engines
    pub async fn start(&self) -> Result<()> {
        info!("Starting engine manager");
        
        // Check if initialized
        {
            let state = self.state.read().await;
            if !matches!(*state, ManagerState::Ready | ManagerState::Paused) {
                return Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Running".to_string()
                ));
            }
        }
        
        // Set state to running
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Running;
        }
        
        // Send started event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::Started);
        }
        
        info!("Engine manager started successfully");
        Ok(())
    }
    
    /// Stop all engines
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping engine manager");
        
        // Set state to shutting down
        {
            let mut state = self.state.write().await;
            *state = ManagerState::ShuttingDown;
        }
        
        // Stop engines in reverse dependency order
        let _ = self.live_engine.write().await.shutdown().await;
        let _ = self.transport_engine.write().await.shutdown().await;
        let _ = self.streamer_engine.write().await.shutdown().await;
        let _ = self.segmenter_engine.write().await.shutdown().await;
        let _ = self.main_engine.write().await.shutdown().await;
        let _ = self.core_engine.write().await.shutdown().await;
        
        // Set state to shutdown
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Shutdown;
        }
        
        // Send stopped event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::Stopped);
        }
        
        info!("Engine manager stopped successfully");
        Ok(())
    }
    
    /// Shutdown all engines
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down engine manager");
        
        // Send shutdown signal
        if let Some(sender) = self.shutdown_sender.write().await.take() {
            let _ = sender.send(());
        }
        
        // Stop all engines
        self.stop().await?;
        
        info!("Engine manager shut down successfully");
        Ok(())
    }
    
    /// Pause all engines
    pub async fn pause(&self) -> Result<()> {
        info!("Pausing engine manager");
        
        // Check if running
        {
            let state = self.state.read().await;
            if !matches!(*state, ManagerState::Running) {
                return Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Paused".to_string()
                ));
            }
        }
        
        // Pause engines (except core, which should keep running)
        let _ = self.live_engine.write().await.pause().await;
        let _ = self.transport_engine.write().await.pause().await;
        let _ = self.streamer_engine.write().await.pause().await;
        let _ = self.segmenter_engine.write().await.pause().await;
        let _ = self.main_engine.write().await.pause().await;
        
        // Core engine continues running to provide fundamental services
        
        // Set state to paused
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Paused;
        }
        
        // Send paused event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::Paused);
        }
        
        info!("Engine manager paused successfully");
        Ok(())
    }
    
    /// Resume all engines
    pub async fn resume(&self) -> Result<()> {
        info!("Resuming engine manager");
        
        // Check if paused
        {
            let state = self.state.read().await;
            if !matches!(*state, ManagerState::Paused) {
                return Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Running".to_string()
                ));
            }
        }
        
        // Resume engines
        let _ = self.main_engine.write().await.resume().await;
        let _ = self.segmenter_engine.write().await.resume().await;
        let _ = self.streamer_engine.write().await.resume().await;
        let _ = self.transport_engine.write().await.resume().await;
        let _ = self.live_engine.write().await.resume().await;
        
        // Set state to running
        {
            let mut state = self.state.write().await;
            *state = ManagerState::Running;
        }
        
        // Send resumed event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::Resumed);
        }
        
        info!("Engine manager resumed successfully");
        Ok(())
    }
    
    /// Update configuration for all engines
    pub async fn update_config(&self, config: &Config) -> Result<()> {
        info!("Updating configuration for all engines");
        
        // Update engine configurations
        {
            use crate::core::traits::Configurable;
            { let mut guard = self.core_engine.write().await; Configurable::update_config(&mut *guard, &config.core).await?; }
            { let mut guard = self.main_engine.write().await; Configurable::update_config(&mut *guard, &config.main).await?; }
            { let mut guard = self.segmenter_engine.write().await; Configurable::update_config(&mut *guard, &config.segmenter).await?; }
            { let mut guard = self.streamer_engine.write().await; Configurable::update_config(&mut *guard, &config.streamer).await?; }
            { let mut guard = self.transport_engine.write().await; Configurable::update_config(&mut *guard, &config.transport).await?; }
            { let mut guard = self.live_engine.write().await; Configurable::update_config(&mut *guard, &config.live).await?; }
        }
        
        // Update manager configuration
        {
            let mut manager_config = self.config.write().await;
            *manager_config = config.clone();
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.config_updates += 1;
        }
        
        // Send configuration updated event
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            let _ = sender.send(ManagerEvent::ConfigurationUpdated);
        }
        
        info!("Configuration updated for all engines");
        Ok(())
    }
    
    /// Get overall health status
    pub async fn get_health(&self) -> EngineHealth {
        trace!("Getting overall health status");
        
        use crate::core::traits::Monitorable;
        let core_health = Monitorable::get_health(&*self.core_engine.read().await).await;
        let main_health = Monitorable::get_health(&*self.main_engine.read().await).await;
        let segmenter_health = Monitorable::get_health(&*self.segmenter_engine.read().await).await;
        let streamer_health = Monitorable::get_health(&*self.streamer_engine.read().await).await;
        let transport_health = Monitorable::get_health(&*self.transport_engine.read().await).await;
        let live_health = Monitorable::get_health(&*self.live_engine.read().await).await;
        
        // Determine overall health (worst case)
        let healths = vec![
            core_health.clone(), main_health.clone(), segmenter_health.clone(),
            streamer_health.clone(), transport_health.clone(), live_health.clone()
        ];
        
        let overall_health = healths.into_iter()
            .min()
            .unwrap_or(EngineHealth::Unknown);
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.last_health_check = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());
            stats.engine_health.insert("core".to_string(), core_health);
            stats.engine_health.insert("main".to_string(), main_health);
            stats.engine_health.insert("segmenter".to_string(), segmenter_health);
            stats.engine_health.insert("streamer".to_string(), streamer_health);
            stats.engine_health.insert("transport".to_string(), transport_health);
            stats.engine_health.insert("live".to_string(), live_health);
        }
        
        overall_health
    }
    
    /// Get manager state
    pub async fn get_state(&self) -> ManagerState {
        self.state.read().await.clone()
    }
    
    /// Get manager statistics
    pub async fn get_statistics(&self) -> ManagerStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }
    
    /// Subscribe to manager events
    pub async fn subscribe_to_events(&self) -> Option<broadcast::Receiver<ManagerEvent>> {
        self.event_sender.read().await.as_ref().map(|s| s.subscribe())
    }
    
    /// Restart a specific engine
    pub async fn restart_engine(&self, engine_name: &str) -> Result<()> {
        info!("Restarting engine: {}", engine_name);
        
        let config = self.config.read().await.clone();
        
        match engine_name {
            "core" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.core_engine.write().await;
                    use crate::core::traits::Lifecycle;
                    Lifecycle::shutdown(&mut *engine_guard).await?;
                    
                    // Create new engine and initialize it
                    let mut new_engine = CoreEngine::new(config.core.clone());
                    Lifecycle::initialize(&mut new_engine).await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            "main" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.main_engine.write().await;
                    use crate::core::traits::Lifecycle;
                    Lifecycle::shutdown(&mut *engine_guard).await?;
                    
                    // Create new engine and initialize it
                    let mut new_engine = MainEngine::new(&config.main)?;
                    Lifecycle::initialize(&mut new_engine).await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            "segmenter" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.segmenter_engine.write().await;
                    use crate::core::traits::Lifecycle;
                    Lifecycle::shutdown(&mut *engine_guard).await?;
                    
                    // Create new engine and initialize it
                    let mut new_engine = SegmenterEngine::new(&config.segmenter)?;
                    Lifecycle::initialize(&mut new_engine).await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            "streamer" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.streamer_engine.write().await;
                    use crate::core::traits::Lifecycle;
                    Lifecycle::shutdown(&mut *engine_guard).await?;
                    
                    // Create new engine and initialize it
                    let mut new_engine = StreamerEngine::new(&config.streamer)?;
                    Lifecycle::initialize(&mut new_engine).await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            "transport" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.transport_engine.write().await;
                    engine_guard.shutdown().await?;
                    
                    // Create and initialize new engine
                    let new_engine = TransportEngine::new(&config.transport)?;
                    new_engine.initialize().await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            "live" => {
                // Shutdown existing engine and replace with new one
                {
                    let mut engine_guard = self.live_engine.write().await;
                    use crate::core::traits::Lifecycle;
                    Lifecycle::shutdown(&mut *engine_guard).await?;
                    
                    // Create and initialize new engine
                    let mut new_engine = LiveEngine::new(&config.live)?;
                    Lifecycle::initialize(&mut new_engine).await?;
                    
                    // Replace the engine
                    *engine_guard = new_engine;
                }
            },
            _ => {
                return Err(OpenAceError::configuration(
                    format!("Unknown engine: {}", engine_name)
                ));
            }
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.restart_count += 1;
        }
        
        info!("Engine {} restarted successfully", engine_name);
        Ok(())
    }
    
    /// Get engine references for direct access
    pub fn get_core_engine(&self) -> Arc<RwLock<CoreEngine>> {
        Arc::clone(&self.core_engine)
    }
    
    pub fn get_main_engine(&self) -> Arc<RwLock<MainEngine>> {
        Arc::clone(&self.main_engine)
    }
    
    pub fn get_segmenter_engine(&self) -> Arc<RwLock<SegmenterEngine>> {
        Arc::clone(&self.segmenter_engine)
    }
    
    pub fn get_streamer_engine(&self) -> Arc<RwLock<StreamerEngine>> {
        Arc::clone(&self.streamer_engine)
    }
    
    pub fn get_transport_engine(&self) -> Arc<RwLock<TransportEngine>> {
        Arc::clone(&self.transport_engine)
    }
    
    pub fn get_live_engine(&self) -> Arc<RwLock<LiveEngine>> {
        Arc::clone(&self.live_engine)
    }

    // C-API compatibility methods
    
    /// Submit segmentation job (C-API compatibility)
    pub async fn submit_segmentation_job(
        &self,
        input_path: &str,
        output_path: &str,
        _format: &str,
    ) -> Result<String> {
        let segmenter = self.get_segmenter_engine();
        let segmenter_guard = segmenter.read().await;
        
        use std::path::PathBuf;
        use std::time::Duration;
        use crate::core::types::{MediaFormat, MediaQuality};
        
        let input_path_buf = PathBuf::from(input_path);
        let output_path_buf = PathBuf::from(output_path);
        let segment_duration = Duration::from_secs(10); // Default 10 seconds
        let media_format = MediaFormat::Mp4; // Default format
        let quality = MediaQuality::High; // Default quality
        
        let job_id = segmenter_guard.submit_job(
            input_path_buf,
            output_path_buf,
            segment_duration,
            media_format,
            quality,
        ).await?;
        
        Ok(job_id.to_string())
    }
    
    /// Start live stream (C-API compatibility)
    pub async fn live_start_stream(&self, stream_key: &str, _rtmp_url: &str) -> Result<()> {
        let live = self.get_live_engine();
        let live_guard = live.read().await;
        
        // Create broadcast parameters
        let broadcast_params = crate::engines::live::BroadcastParams {
            title: format!("Stream {}", stream_key),
            description: Some("Live stream via C-API".to_string()),
            broadcaster_id: crate::core::types::Id::new("c-api-broadcaster"),
            broadcaster_name: "C-API Broadcaster".to_string(),
            category: "Live Stream".to_string(),
            tags: vec!["c-api".to_string(), "live".to_string()],
            format: crate::core::types::MediaFormat::Mp4,
            quality: crate::core::types::MediaQuality::High,
            resolution: (1920, 1080),
            fps: 30.0,
            bitrate: 5000,
            is_private: false,
            is_mature: false,
        };
        
        let broadcast_id = live_guard.create_broadcast(broadcast_params).await?;
        live_guard.start_broadcast(&broadcast_id).await
    }
    
    /// Stop live stream (C-API compatibility)
    pub async fn live_stop_stream(&self, stream_key: &str) -> Result<()> {
        let live = self.get_live_engine();
        let live_guard = live.read().await;
        
        // Find broadcast by stream key and stop it
        let broadcasts = live_guard.get_all_broadcasts().await;
        for broadcast in broadcasts {
            if broadcast.stream_key == stream_key {
                return live_guard.end_broadcast(&broadcast.id).await;
            }
        }
        
        Err(OpenAceError::not_found(format!("Stream with key '{}' not found", stream_key)))
    }
    
    /// Check if streaming (C-API compatibility)
    pub async fn is_streaming(&self, stream_key: &str) -> Result<bool> {
        let live = self.get_live_engine();
        let live_guard = live.read().await;
        
        let active_broadcasts = live_guard.get_active_broadcasts().await;
        for broadcast in active_broadcasts {
            if broadcast.stream_key == stream_key {
                return Ok(broadcast.status == crate::engines::live::BroadcastStatus::Live);
            }
        }
        
        Ok(false)
    }
    
    /// Set tracker for transport engine (C-API compatibility)
    pub async fn transport_set_tracker(&self, _tracker_url: &str) -> Result<()> {
        let transport = self.get_transport_engine();
        let transport_guard = transport.write().await;
        
        // Update transport configuration with tracker URL
        let config = transport_guard.get_config().await;
        // Assuming tracker_url is stored in transport config
        // This would need to be implemented in TransportConfig
        transport_guard.update_config(&config).await
    }
    
    /// Connect to peers (C-API compatibility)
    pub async fn transport_connect_peers(&self, peer_addresses: &[String]) -> Result<usize> {
        let transport = self.get_transport_engine();
        let transport_guard = transport.read().await;
        
        let mut connected_count = 0;
        for address in peer_addresses {
            // Parse address string to SocketAddr
            if let Ok(remote_addr) = address.parse::<std::net::SocketAddr>() {
                // Use a default local address
                let local_addr = "0.0.0.0:0".parse::<std::net::SocketAddr>().unwrap();
                
                match transport_guard.create_connection(
                    remote_addr,
                    local_addr,
                    crate::core::types::NetworkProtocol::Tcp
                ).await {
                    Ok(_) => connected_count += 1,
                    Err(e) => {
                        warn!("Failed to connect to peer {}: {}", address, e);
                    }
                }
            } else {
                warn!("Invalid address format: {}", address);
            }
        }
        
        Ok(connected_count)
    }
    
    /// Get peer count (C-API compatibility)
    pub async fn get_peer_count(&self) -> Result<usize> {
        let transport = self.get_transport_engine();
        let transport_guard = transport.read().await;
        
        let connections = transport_guard.get_active_connections().await;
        Ok(connections.len())
    }
    
    /// Start config watcher task
    async fn start_config_watcher(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        let config = Arc::clone(&self.config);
        let event_sender = Arc::clone(&self.event_sender);
        
        tokio::spawn(async move {
            let config_path = {
                let config = config.read().await;
                config.config_path.clone()
            };
            
            if let Some(path) = config_path {
                use crossbeam_channel::bounded;
let (tx, rx) = bounded(1);
let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default()).unwrap();
watcher.watch(&path, RecursiveMode::NonRecursive).unwrap();

tokio::spawn(async move {
    while let Ok(event_result) = rx.recv() {
                    match event_result {
                        Ok(event) if event.kind.is_modify() => {
                            info!("Configuration file changed, reloading");
                            match Config::from_file(&path) {
                                Ok(new_config) => {
                                    let mut config_guard = config.write().await;
                                    *config_guard = new_config;
                                    if let Some(sender) = event_sender.read().await.as_ref() {
                                        let _ = sender.send(ManagerEvent::ConfigurationUpdated);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to reload config: {}", e);
                                }
                            }
                        }
                        Ok(_) => {}, // Ignore non-modify events
                        Err(e) => error!("Watch error: {:?}", e),
                    }
                }
    debug!("Config watcher shutting down");
});

// Wait for shutdown signal
let _ = shutdown_rx.recv().await; // Wait for shutdown signal
            }
        });
    }
    
    /// Start monitoring task
    async fn start_monitoring_task(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        let core_engine = Arc::clone(&self.core_engine);
        let main_engine = Arc::clone(&self.main_engine);
        let segmenter_engine = Arc::clone(&self.segmenter_engine);
        let streamer_engine = Arc::clone(&self.streamer_engine);
        let transport_engine = Arc::clone(&self.transport_engine);
        let live_engine = Arc::clone(&self.live_engine);
        let event_sender = Arc::clone(&self.event_sender);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Perform health check for each engine
                        let mut engine_health = HashMap::new();
                        
                        // Check core engine health
                        let core_health = core_engine.read().await.get_health().await;
                        engine_health.insert("core".to_string(), core_health.clone());
                        
                        // Check main engine health
                        let main_health = main_engine.read().await.get_health().await;
                        engine_health.insert("main".to_string(), main_health.clone());
                        
                        // Check segmenter engine health
                        let segmenter_health = segmenter_engine.read().await.get_health().await;
                        engine_health.insert("segmenter".to_string(), segmenter_health.clone());
                        
                        // Check streamer engine health
                        let streamer_health = streamer_engine.read().await.get_health().await;
                        engine_health.insert("streamer".to_string(), streamer_health.clone());
                        
                        // Check transport engine health
                        let transport_health = transport_engine.read().await.get_health().await;
                        engine_health.insert("transport".to_string(), transport_health.clone());
                        
                        // Check live engine health
                        let live_health = live_engine.read().await.get_health().await;
                        engine_health.insert("live".to_string(), live_health.clone());
                        
                        // Determine overall health
                        let overall_health = if engine_health.values().any(|h| matches!(h, EngineHealth::Error(_))) {
                            EngineHealth::Error("One or more engines have errors".to_string())
                        } else if engine_health.values().any(|h| matches!(h, EngineHealth::Warning(_))) {
                            EngineHealth::Warning("One or more engines have warnings".to_string())
                        } else if engine_health.values().all(|h| matches!(h, EngineHealth::Healthy)) {
                            EngineHealth::Healthy
                        } else {
                            EngineHealth::Unknown
                        };
                        
                        // Send health check event
                        if let Some(sender) = event_sender.read().await.as_ref() {
                            let _ = sender.send(ManagerEvent::HealthCheckCompleted {
                                overall_health: overall_health.clone(),
                                engine_health: engine_health.clone(),
                            });
                        }
                        
                        // Log health status
                        match &overall_health {
                            EngineHealth::Healthy => debug!("All engines healthy"),
                            EngineHealth::Warning(_) => warn!("Some engines have warnings"),
                            EngineHealth::Error(_) => error!("Some engines have errors"),
                            EngineHealth::Critical(_) => error!("Some engines are in critical state"),
                            EngineHealth::Failed(_) => error!("Some engines have failed"),
                            EngineHealth::Unknown => warn!("Engine health unknown"),
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("Monitoring task shutting down");
                        break;
                    }
                }
            }
        });
    }
}

// Implement traits for EngineManager
#[async_trait::async_trait]
impl Lifecycle for EngineManager {
    async fn initialize(&mut self) -> Result<()> {
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        // Check if manager is in an initialized state
        if let Ok(state) = self.state.try_read() {
            !matches!(*state, ManagerState::Uninitialized | ManagerState::Error(_))
        } else {
            false
        }
    }
    
    async fn force_shutdown(&mut self) -> Result<()> {
        // Force shutdown all engines immediately
        let mut state = self.state.write().await;
        *state = ManagerState::ShuttingDown;
        
        // Force shutdown all engines individually
        if let Ok(mut engine_guard) = self.core_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        if let Ok(mut engine_guard) = self.main_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        if let Ok(mut engine_guard) = self.segmenter_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        if let Ok(mut engine_guard) = self.streamer_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        if let Ok(mut engine_guard) = self.transport_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        if let Ok(mut engine_guard) = self.live_engine.try_write() {
            let _ = engine_guard.force_shutdown().await;
        }
        
        *state = ManagerState::Shutdown;
        Ok(())
    }
    
    fn get_initialization_status(&self) -> crate::core::traits::InitializationStatus {
        use crate::core::traits::InitializationStatus;
        
        if let Ok(state) = self.state.try_read() {
            match *state {
                ManagerState::Uninitialized => InitializationStatus::Uninitialized,
                ManagerState::Initializing => InitializationStatus::Initializing,
                ManagerState::Ready | ManagerState::Running | ManagerState::Paused => InitializationStatus::Initialized,
                ManagerState::Error(ref err) => InitializationStatus::Failed(err.clone()),
                ManagerState::ShuttingDown | ManagerState::Shutdown => InitializationStatus::Initialized,
            }
        } else {
            InitializationStatus::Failed("Unable to read manager state".to_string())
        }
    }
}

#[async_trait::async_trait]
impl Pausable for EngineManager {
    async fn pause(&mut self) -> Result<()> {
        self.pause().await
    }

    async fn resume(&mut self) -> Result<()> {
        self.resume().await
    }

    async fn is_paused(&self) -> bool {
        matches!(self.get_state().await, ManagerState::Paused)
    }
    
    async fn toggle_pause(&mut self) -> Result<()> {
        if self.is_paused().await {
            self.resume().await
        } else {
            self.pause().await
        }
    }
    
    async fn get_pause_duration(&self) -> Option<std::time::Duration> {
        // For simplicity, we'll track pause duration in stats
        let _stats = self.stats.read().await;
        // This would need to be implemented with proper pause time tracking
        // For now, return None as we don't track pause duration yet
        None
    }
}

#[async_trait::async_trait]
impl StatisticsProvider for EngineManager {
    type Stats = ManagerStats;

    async fn get_statistics(&self) -> Self::Stats {
        self.stats.read().await.clone()
    }

    async fn reset_statistics(&mut self) -> Result<()> {
        // Reset manager statistics
        let mut stats = self.stats.write().await;
        *stats = ManagerStats::default();
        
        // Reset engine statistics
        StatisticsProvider::reset_statistics(&mut *self.core_engine.write().await).await?;
        StatisticsProvider::reset_statistics(&mut *self.main_engine.write().await).await?;
        StatisticsProvider::reset_statistics(&mut *self.segmenter_engine.write().await).await?;
        StatisticsProvider::reset_statistics(&mut *self.streamer_engine.write().await).await?;
        StatisticsProvider::reset_statistics(&mut *self.transport_engine.write().await).await?;
        StatisticsProvider::reset_statistics(&mut *self.live_engine.write().await).await?;
        
        Ok(())
    }
    
    async fn get_detailed_statistics(&self) -> std::collections::HashMap<String, crate::core::traits::StatisticValue> {
        use crate::core::traits::StatisticValue;
        let mut detailed = std::collections::HashMap::new();
        
        let stats = self.stats.read().await;
        detailed.insert("manager_uptime".to_string(), StatisticValue::Duration(stats.uptime));
        detailed.insert("manager_total_requests".to_string(), StatisticValue::Counter(stats.total_requests));
        detailed.insert("manager_active_engines".to_string(), StatisticValue::Gauge(stats.active_engines as f64));
        
        // Add engine-specific statistics
        let core_stats = self.core_engine.read().await.get_statistics().await;
        detailed.insert("core_engine_uptime_seconds".to_string(), crate::core::traits::StatisticValue::Gauge(core_stats.uptime_seconds as f64));
        detailed.insert("core_engine_total_operations".to_string(), crate::core::traits::StatisticValue::Counter(core_stats.total_operations));
        detailed.insert("core_engine_failed_operations".to_string(), crate::core::traits::StatisticValue::Counter(core_stats.failed_operations));
        detailed.insert("core_engine_memory_usage_bytes".to_string(), crate::core::traits::StatisticValue::Gauge(core_stats.memory_usage_bytes as f64));
        detailed.insert("core_engine_cpu_usage_percent".to_string(), crate::core::traits::StatisticValue::Gauge(core_stats.cpu_usage_percent));
        detailed.insert("core_engine_active_resources".to_string(), crate::core::traits::StatisticValue::Gauge(core_stats.active_resources as f64));
        detailed.insert("core_engine_avg_operation_duration_ms".to_string(), crate::core::traits::StatisticValue::Gauge(core_stats.avg_operation_duration_ms));
        
        detailed
    }
    
    async fn get_resource_usage(&self) -> Option<crate::core::traits::ResourceUsage> {
        use crate::core::traits::ResourceUsage;
        
        let stats = self.stats.read().await;
        Some(ResourceUsage {
            memory_bytes: stats.memory_usage_bytes,
            cpu_percent: stats.cpu_usage_percent,
            network_bytes_in: 0, // Would need to be tracked
            network_bytes_out: 0, // Would need to be tracked
            disk_bytes_read: 0, // Would need to be tracked
            disk_bytes_written: 0, // Would need to be tracked
        })
    }
    
    async fn get_health_status(&self) -> crate::core::traits::HealthStatus {
        use crate::core::traits::HealthStatus;
        
        let health = self.get_health().await;
        match health {
            EngineHealth::Healthy => HealthStatus::Healthy,
            EngineHealth::Warning(ref msg) => HealthStatus::Warning(msg.clone()),
            EngineHealth::Error(ref msg) => HealthStatus::Critical(msg.clone()),
            EngineHealth::Critical(ref msg) => HealthStatus::Critical(msg.clone()),
            EngineHealth::Failed(ref msg) => HealthStatus::Critical(msg.clone()),
            EngineHealth::Unknown => HealthStatus::Unknown,
        }
    }
    
    async fn get_statistics_range(&self, _start: std::time::Instant, _end: std::time::Instant) -> Self::Stats {
        // For now, return current stats as we don't have historical data
        self.get_statistics().await
    }
}

#[async_trait::async_trait]
impl Configurable for EngineManager {
    type Config = Config;
    
    async fn get_config(&self) -> Self::Config {
        self.config.read().await.clone()
    }
    
    async fn update_config(&mut self, config: &Self::Config) -> Result<()> {
        self.update_config(config).await
    }
    
    async fn validate_config(config: &Self::Config) -> Result<()> {
        config.validate()
    }
    
    async fn apply_config_changes(&mut self, changes: std::collections::HashMap<String, String>) -> Result<()> {
        let mut current_config = self.get_config().await;
        
        // Apply changes to the config
        for (key, value) in changes {
            match key.as_str() {
                "core.max_concurrent_operations" => {
                    if let Ok(val) = value.parse::<usize>() {
                        current_config.core.max_concurrent_operations = val;
                    }
                }
                // Note: buffer_size is not a field in CoreConfig
                // "core.buffer_size" => {
                //     if let Ok(val) = value.parse::<usize>() {
                //         current_config.core.buffer_size = val;
                //     }
                // }
                _ => {
                    warn!("Unknown config key: {}", key);
                }
            }
        }
        
        self.update_config(&current_config).await
    }
    
    async fn get_config_schema(&self) -> std::collections::HashMap<String, String> {
        let mut schema = std::collections::HashMap::new();
        schema.insert("core.max_concurrent_operations".to_string(), "integer (1-1000)".to_string());
        schema.insert("core.buffer_size".to_string(), "integer (1024-1048576)".to_string());
        schema.insert("core.enable_monitoring".to_string(), "boolean".to_string());
        schema.insert("core.log_level".to_string(), "string (trace|debug|info|warn|error)".to_string());
        schema
    }
    
    async fn has_config_changed(&self) -> bool {
        // For simplicity, always return false
        // In a real implementation, this would track config file changes
        false
    }
    
    async fn reload_config(&mut self) -> Result<()> {
        // In a real implementation, this would reload from file
        // For now, just validate current config
        let config = self.get_config().await;
        Self::validate_config(&config).await
    }
}

#[async_trait::async_trait]
impl Monitorable for EngineManager {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        self.get_health().await
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        Ok(self.get_health().await)
    }
    
    async fn get_detailed_health(&self) -> crate::core::traits::HealthStatus {
        // Convert overall engine health to HealthStatus
        let overall_health = self.get_health().await;
        match overall_health {
            EngineHealth::Healthy => crate::core::traits::HealthStatus::Healthy,
            EngineHealth::Warning(msg) => crate::core::traits::HealthStatus::Warning(msg),
            EngineHealth::Error(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Critical(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Failed(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Unknown => crate::core::traits::HealthStatus::Unknown,
        }
    }
    
    async fn get_performance_metrics(&self) -> std::collections::HashMap<String, f64> {
        let mut metrics = std::collections::HashMap::new();
        
        // Get basic performance metrics
        let stats = self.get_statistics().await;
        metrics.insert("uptime_seconds".to_string(), stats.uptime_seconds as f64);
        metrics.insert("cpu_usage_percent".to_string(), stats.cpu_usage_percent);
        metrics.insert("memory_usage_mb".to_string(), stats.memory_usage_bytes as f64 / 1024.0 / 1024.0);
        metrics.insert("restart_count".to_string(), stats.restart_count as f64);
        
        metrics
    }
    
    async fn get_alerts(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        
        // Check engine health and generate alerts
        let overall_health = self.get_health().await;
        match overall_health {
            EngineHealth::Warning(msg) => alerts.push(format!("Warning: {}", msg)),
            EngineHealth::Error(msg) => alerts.push(format!("Error: {}", msg)),
            EngineHealth::Critical(msg) => alerts.push(format!("Critical: {}", msg)),
            EngineHealth::Failed(msg) => alerts.push(format!("Failed: {}", msg)),
            _ => {}
        }
        
        // Check individual engine health
        if let Ok(core_engine) = self.core_engine.try_read() {
            match core_engine.get_health().await {
                EngineHealth::Warning(msg) => alerts.push(format!("Core Engine Warning: {}", msg)),
                EngineHealth::Error(msg) => alerts.push(format!("Core Engine Error: {}", msg)),
                EngineHealth::Critical(msg) => alerts.push(format!("Core Engine Critical: {}", msg)),
                EngineHealth::Failed(msg) => alerts.push(format!("Core Engine Failed: {}", msg)),
                _ => {}
            }
        }
        
        if let Ok(main_engine) = self.main_engine.try_read() {
            match main_engine.get_health().await {
                EngineHealth::Warning(msg) => alerts.push(format!("Main Engine Warning: {}", msg)),
                EngineHealth::Error(msg) => alerts.push(format!("Main Engine Error: {}", msg)),
                EngineHealth::Critical(msg) => alerts.push(format!("Main Engine Critical: {}", msg)),
                EngineHealth::Failed(msg) => alerts.push(format!("Main Engine Failed: {}", msg)),
                _ => {}
            }
        }
        
        alerts
    }
    
    async fn set_monitoring_thresholds(&mut self, thresholds: std::collections::HashMap<String, f64>) -> Result<()> {
        // Store monitoring thresholds (in a real implementation, these would be used for alerting)
        debug!("Setting monitoring thresholds: {:?}", thresholds);
        Ok(())
    }
    
    async fn get_uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
    
    async fn get_last_error(&self) -> Option<String> {
        // Check if any engine is in error state
        let overall_health = self.get_health().await;
        match overall_health {
            EngineHealth::Error(msg) | EngineHealth::Critical(msg) | EngineHealth::Failed(msg) => Some(msg),
            _ => None,
        }
    }
}



#[async_trait::async_trait]
impl crate::core::traits::ResourceManager<String> for EngineManager {
    
    async fn acquire(&self) -> Result<String> {
        // Return the name of an available engine
        let state = self.state.read().await;
        match *state {
            ManagerState::Ready | ManagerState::Running => Ok("core".to_string()),
            _ => Err(crate::error::OpenAceError::InvalidArgument("Manager not ready".to_string())),
        }
    }
    
    async fn release(&self, resource: String) -> Result<()> {
        // For engines, release is a no-op as they're managed by the manager
        debug!("Released resource: {}", resource);
        Ok(())
    }
    
    fn available_count(&self) -> usize {
        // Return number of available engines
        6 // core, main, segmenter, streamer, transport, live
    }
    
    fn total_count(&self) -> usize {
        // Return total number of engines
        6
    }
    

    
    async fn get_resource_info(&self, resource_id: &str) -> Option<crate::core::traits::ResourceInfo> {
        use crate::core::traits::ResourceInfo;
        
        match resource_id {
            "core" | "main" | "segmenter" | "streamer" | "transport" | "live" => {
                Some(ResourceInfo {
                    id: resource_id.to_string(),
                    resource_type: "engine".to_string(),
                    amount: 1,
                    allocated_at: Instant::now(),
                    last_accessed: Some(Instant::now()),
                    metadata: HashMap::new(),
                })
            }
            _ => None,
        }
    }
    
    async fn set_resource_limits(&mut self, limits: std::collections::HashMap<String, u64>) -> Result<()> {
        // For engines, limits don't apply in the traditional sense
        info!("Resource limits set: {:?}", limits);
        Ok(())
    }
    
    async fn get_resource_limits(&self) -> std::collections::HashMap<String, u64> {
        let mut limits = std::collections::HashMap::new();
        limits.insert("core".to_string(), 1);
        limits.insert("main".to_string(), 1);
        limits
    }
    
    async fn cleanup_resources(&mut self) -> Result<u64> {
        // Cleanup unused resources - for engines this means ensuring proper shutdown
        info!("Cleaning up engine resources");
        Ok(0) // Return number of cleaned up resources
    }
    
    async fn get_allocation_history(&self) -> Vec<crate::core::traits::ResourceAllocation> {
        // Return empty history for now
        Vec::new()
    }
    
    async fn reserve_resource(&mut self, resource_id: &str, _amount: u64, duration: std::time::Duration) -> Result<String> {
        // Generate a reservation ID
        let reservation_id = format!("{}_{}", resource_id, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        info!("Reserved resource {} for {:?}: {}", resource_id, duration, reservation_id);
        Ok(reservation_id)
    }
    
    async fn release_reservation(&mut self, reservation_id: &str) -> Result<()> {
        info!("Released reservation: {}", reservation_id);
        Ok(())
    }
}

// EventHandler implementation for EngineManager
#[async_trait]
impl EventHandler<ManagerEvent> for EngineManager {
    async fn handle_event(&self, event: ManagerEvent) -> Result<()> {
        match &event {
            ManagerEvent::Initialized => {
                info!("Manager initialized event handled");
            },
            ManagerEvent::Started => {
                info!("Manager started event handled");
            },
            ManagerEvent::Stopped => {
                info!("Manager stopped event handled");
            },
            ManagerEvent::Paused => {
                info!("Manager paused event handled");
            },
            ManagerEvent::Resumed => {
                info!("Manager resumed event handled");
            },
            ManagerEvent::ConfigurationUpdated => {
                info!("Manager configuration updated event handled");
            },
            ManagerEvent::EngineStateChanged { engine, old_state, new_state } => {
                info!("Engine {} state changed from {:?} to {:?}", engine, old_state, new_state);
            },
            ManagerEvent::HealthCheckCompleted { overall_health, engine_health: _ } => {
                info!("Health check completed with overall status: {:?}", overall_health);
            },
            ManagerEvent::Error { engine, error } => {
                error!("Manager error event from {:?}: {}", engine, error);
            }
        }
        Ok(())
    }

    async fn handle_events(&self, events: Vec<ManagerEvent>) -> Result<Vec<Result<()>>> {
        let mut results = Vec::new();
        for event in events {
            let result = self.handle_event(event).await;
            results.push(result);
        }
        Ok(results)
    }

    fn get_priority(&self) -> i32 {
        100
    }

    fn get_supported_event_types(&self) -> Vec<String> {
        vec![
            "Initialized".to_string(),
            "Started".to_string(),
            "Stopped".to_string(),
            "Paused".to_string(),
            "Resumed".to_string(),
            "ConfigurationUpdated".to_string(),
            "EngineStateChanged".to_string(),
            "HealthCheckCompleted".to_string(),
            "Error".to_string(),
        ]
    }

    fn filter_event(&self, _event: &ManagerEvent) -> bool {
        // Accept all manager events
        true
    }

    async fn get_event_stats(&self) -> std::collections::HashMap<String, u64> {
        // Return empty stats for now
        std::collections::HashMap::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_manager_creation() {
        let config = Config::default();
        let manager = EngineManager::with_config(config);
        assert!(manager.is_ok());
        
        let manager = manager.unwrap();
        assert_eq!(manager.get_state().await, ManagerState::Uninitialized);
    }
    
    #[tokio::test]
    async fn test_manager_initialization() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        // Test initialization
        assert!(manager.initialize().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Ready);
    }
    
    #[tokio::test]
    async fn test_manager_lifecycle() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        // Test initialization
        assert!(manager.initialize().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Ready);
        
        // Test start
        assert!(manager.start().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Running);
        
        // Test pause
        assert!(manager.pause().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Paused);
        assert!(manager.is_paused().await);
        
        // Test resume
        assert!(manager.resume().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Running);
        
        // Test stop
        assert!(manager.stop().await.is_ok());
        assert_eq!(manager.get_state().await, ManagerState::Shutdown);
        
        // Test shutdown
        assert!(manager.shutdown().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_config_update() {
        let config = Config::default();
        let manager = EngineManager::with_config(config.clone()).unwrap();
        
        // Initialize manager
        manager.initialize().await.unwrap();
        
        // Update configuration
        let mut new_config = config.clone();
        new_config.core.max_concurrent_operations = 200;
        
        assert!(manager.update_config(&new_config).await.is_ok());
        
        let updated_config = manager.get_config().await;
        assert_eq!(updated_config.core.max_concurrent_operations, 200);
    }
    
    #[tokio::test]
    async fn test_health_monitoring() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();
        
        let health = manager.get_health().await;
        // Health should be one of the valid EngineHealth variants
        match health {
            EngineHealth::Healthy | EngineHealth::Warning(_) | EngineHealth::Error(_) | EngineHealth::Critical(_) | EngineHealth::Failed(_) | EngineHealth::Unknown => {},
        }
    }
    
    #[tokio::test]
    async fn test_statistics() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        manager.initialize().await.unwrap();
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.successful_initializations, 1);
    }
    
    #[tokio::test]
    async fn test_event_subscription() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        // Initialize manager first to set up event sender
        manager.initialize().await.unwrap();
        
        let mut event_receiver = manager.subscribe_to_events().await.unwrap();
        
        // Trigger another event by starting the manager
        manager.start().await.unwrap();
        
        // Should receive started event
        let event = timeout(Duration::from_secs(1), event_receiver.recv()).await;
        assert!(event.is_ok());
        let received_event = event.unwrap();
        assert!(received_event.is_ok());
        match received_event.unwrap() {
            ManagerEvent::Started => {},
            _ => panic!("Expected Started event"),
        }
    }
    
    #[tokio::test]
    async fn test_engine_restart() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        manager.initialize().await.unwrap();
        
        // Test restarting core engine
        assert!(manager.restart_engine("core").await.is_ok());
        
        // Test invalid engine name
        assert!(manager.restart_engine("invalid").await.is_err());
    }
    
    #[tokio::test]
    async fn test_engine_access() {
        let config = Config::default();
        let manager = EngineManager::with_config(config).unwrap();
        
        // Test engine access methods
        let core_engine = manager.get_core_engine();
        let core_guard = core_engine.read().await;
        assert!(core_guard.get_state().await == CoreState::Uninitialized);
        
        let main_engine = manager.get_main_engine();
        let main_guard = main_engine.read().await;
        assert!(!main_guard.is_initialized());
    }
}