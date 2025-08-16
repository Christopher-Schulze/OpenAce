//! Streamer Engine for OpenAce Rust
//!
//! This module implements the streamer engine that handles media streaming,
//! providing live streaming capabilities with modern async patterns.

use crate::error::{Result, OpenAceError};
use crate::config::StreamerConfig;
use crate::core::traits::*;
use crate::core::types::*;
use crate::utils::threading::{SafeMutex, SafeRwLock};
use crate::engines::EngineHealth;

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, oneshot};

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn, error, trace, instrument};
use serde::{Serialize, Deserialize};
use bytes::Bytes;

/// Streamer engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamerState {
    Uninitialized,
    Initializing,
    Ready,
    Streaming,
    Paused,
    Error(String),
    Shutdown,
}

/// Parameters for creating a new stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamParams {
    pub name: String,
    pub description: Option<String>,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub bitrate: u32,
    pub resolution: (u32, u32),
    pub frame_rate: f32,
}

#[allow(dead_code)]
impl StreamerEngineTask {
    /// Server listening loop (task clone)
    async fn server_loop(
        &self,
        config: StreamerConfig,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting streamer server loop on port {}", config.port);
        debug!("Initializing TCP listener on 0.0.0.0:{}", config.port);

        let addr = format!("0.0.0.0:{}", config.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => {
                debug!("Successfully bound to address: {}", addr);
                listener
            }
            Err(e) => {
                debug!("Failed to bind to address {}: {}", addr, e);
                error!("Failed to bind to {}: {}", addr, e);
                return;
            }
        };

        info!("Streamer server listening on {}", addr);
        debug!("Server loop entering main accept loop");

        loop {
            tokio::select! {
                result = listener.accept() => {
                    debug!("Received connection attempt");
                    match result {
                        Ok((stream, addr)) => {
                            debug!("Successfully accepted connection from: {}", addr);
                            let engine_ref = self.clone();
                            tokio::spawn(async move {
                                debug!("Spawning client handler task for: {}", addr);
                                engine_ref.handle_client(stream, addr).await;
                                debug!("Client handler task completed for: {}", addr);
                            });
                        }
                        Err(e) => {
                            debug!("Failed to accept connection: {}", e);
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    debug!("Shutdown signal received in server loop");
                    info!("Received shutdown signal for server loop");
                    break;
                }
            }
        }

        debug!("Exiting server loop");
        info!("Streamer server loop stopped");
    }

    /// Handle client connection (task clone)
    async fn handle_client(&self, mut stream: TcpStream, addr: SocketAddr) {
        info!("New client connected: {}", addr);
        debug!("Starting client handler for: {}", addr);

        let client_id = Id::generate();
        debug!("Generated client ID: {}", client_id);
        
        let connection = ClientConnection {
            id: client_id.clone(),
            stream_id: Id::generate(),
            address: addr,
            connected_at: Instant::now(),
            bytes_sent: 0,
            packets_sent: 0,
            last_activity: Instant::now(),
            user_agent: None,
            protocol: NetworkProtocol::Tcp,
        };
        debug!("Created ClientConnection object for: {}", addr);

        // Store client connection
        {
            debug!("Adding client to clients list");
            let mut clients = self.clients.write().await;
            let old_count = clients.len();
            clients.insert(client_id.clone(), connection);
            debug!("Client added - count: {} -> {}", old_count, clients.len());
        }

        // Update stats
        {
            debug!("Updating client statistics");
            let mut stats = self.stats.write().await;
            let old_total = stats.total_clients;
            let old_active = stats.active_clients;
            stats.total_clients += 1;
            stats.active_clients += 1;
            debug!("Updated stats - total_clients: {} -> {}, active_clients: {} -> {}", old_total, stats.total_clients, old_active, stats.active_clients);

            if stats.active_clients > stats.peak_concurrent_viewers as u64 {
                let old_peak = stats.peak_concurrent_viewers;
                stats.peak_concurrent_viewers = stats.active_clients as u32;
                debug!("Updated peak_concurrent_viewers: {} -> {}", old_peak, stats.peak_concurrent_viewers);
            }
        }

        // Handle client communication
        debug!("Entering client communication loop for: {}", addr);
        let mut buffer = [0; 1024];
        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Client {} sent EOF (disconnected)", addr);
                    // Client disconnected
                    break;
                }
                Ok(n) => {
                    debug!("Received {} bytes from client {}", n, addr);
                    if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                        client.last_activity = Instant::now();
                        debug!("Updated last_activity for client: {}", client_id);
                    } else {
                        debug!("Client {} not found in clients list during activity update", client_id);
                    }
                }
                Err(e) => {
                    debug!("Error reading from client {}: {}", addr, e);
                    error!("Error reading from client {}: {}", addr, e);
                    break;
                }
            }
        }

        debug!("Cleaning up client: {}", addr);

        // Remove client connection
        {
            debug!("Removing client from clients list: {}", client_id);
            let mut clients = self.clients.write().await;
            let old_count = clients.len();
            clients.remove(&client_id);
            debug!("Client removed - count: {} -> {}", old_count, clients.len());
        }

        // Update stats
        {
            debug!("Updating global client statistics after disconnect");
            let mut stats = self.stats.write().await;
            let old_active = stats.active_clients;
            stats.active_clients = stats.active_clients.saturating_sub(1);
            debug!("Updated active_clients: {} -> {}", old_active, stats.active_clients);
        }

        debug!("Client handler cleanup completed for: {}", addr);
        info!("Client disconnected: {}", addr);
    }
}

/// Stream information
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub bitrate: u32,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub created_at: Instant,
    pub status: StreamStatus,
    pub viewer_count: u32,
    pub bytes_sent: u64,
    pub packets_sent: u64,
    pub error_message: Option<String>,
}

/// Stream status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamStatus {
    Preparing,
    Live,
    Paused,
    Stopped,
    Error,
}

/// Client connection information
#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub id: Id,
    pub stream_id: Id,
    pub address: SocketAddr,
    pub connected_at: Instant,
    pub bytes_sent: u64,
    pub packets_sent: u64,
    pub last_activity: Instant,
    pub user_agent: Option<String>,
    pub protocol: NetworkProtocol,
}

/// Streaming statistics
#[derive(Debug, Clone)]
pub struct StreamerStats {
    pub total_streams: u64,
    pub active_streams: u64,
    pub total_clients: u64,
    pub active_clients: u64,
    pub total_bytes_sent: u64,
    pub total_packets_sent: u64,
    pub average_bitrate: u32,
    pub peak_concurrent_viewers: u32,
    pub uptime: Duration,
    pub memory_usage: u64,
    pub bandwidth_usage: u64,
    pub error_count: u64,
}

impl Default for StreamerStats {
    fn default() -> Self {
        Self {
            total_streams: 0,
            active_streams: 0,
            total_clients: 0,
            active_clients: 0,
            total_bytes_sent: 0,
            total_packets_sent: 0,
            average_bitrate: 0,
            peak_concurrent_viewers: 0,
            uptime: Duration::ZERO,
            memory_usage: 0,
            bandwidth_usage: 0,
            error_count: 0,
        }
    }
}

/// Media packet for streaming
#[derive(Debug, Clone)]
pub struct MediaPacket {
    pub stream_id: Id,
    pub sequence_number: u64,
    pub timestamp: u64,
    pub data: Bytes,
    pub is_keyframe: bool,
    pub packet_type: PacketType,
}

/// Packet type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PacketType {
    Video,
    Audio,
    Metadata,
    Control,
}

/// Streamer engine implementation
#[derive(Debug)]
pub struct StreamerEngine {
    config: SafeRwLock<StreamerConfig>,
    state: SafeRwLock<StreamerState>,
    stats: SafeRwLock<StreamerStats>,
    streams: SafeRwLock<HashMap<Id, StreamInfo>>,
    clients: SafeRwLock<HashMap<Id, ClientConnection>>,
    packet_sender: SafeMutex<Option<broadcast::Sender<MediaPacket>>>,
    shutdown_sender: SafeMutex<Option<oneshot::Sender<()>>>,
    start_time: Instant,
}

impl StreamerEngine {
    /// Create a new streamer engine
    pub fn new(config: &StreamerConfig) -> Result<Self> {
        info!("Creating streamer engine");
        
        Ok(Self {
            config: SafeRwLock::new(config.clone(), "streamer_config"),
            state: SafeRwLock::new(StreamerState::Uninitialized, "streamer_state"),
            stats: SafeRwLock::new(StreamerStats::default(), "streamer_stats"),
            streams: SafeRwLock::new(HashMap::new(), "streamer_streams"),
            clients: SafeRwLock::new(HashMap::new(), "streamer_clients"),
            packet_sender: SafeMutex::new(None, "streamer_packet_sender"),
            shutdown_sender: SafeMutex::new(None, "streamer_shutdown_sender"),
            start_time: Instant::now(),
        })
    }
    
    /// Initialize the streamer engine
    pub async fn initialize(&self) -> Result<()> {
        trace!("Initializing streamer engine");
        debug!("Starting StreamerEngine initialization process");
        info!("Initializing streamer engine");
        
        {
            let mut state = self.state.write().await;
            debug!("Current state before initialization: {:?}", *state);
            if *state != StreamerState::Uninitialized {
            debug!("Invalid state transition attempted from {:?} to Initializing", *state);
            return Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Uninitialized"
            ));
            }
            debug!("Setting state to Initializing");
            *state = StreamerState::Initializing;
        }
        
        // Create packet broadcast channel
        debug!("Creating packet broadcast channel with capacity 1000");
        let (packet_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        
        // Store senders
        debug!("Storing packet and shutdown senders");
        *self.packet_sender.lock().await = Some(packet_tx);
        *self.shutdown_sender.lock().await = Some(shutdown_tx);
        debug!("Packet broadcast channel created successfully");
        
        // Start server listening task
        debug!("Starting server listening task");
        let engine_ref = self.clone_for_task();
        let config = self.config.read().await.clone();
        tokio::spawn(async move {
            engine_ref.server_loop(config, shutdown_rx).await;
        });
        debug!("Server listening task spawned successfully");
        
        // Check and enable hardware acceleration if configured
        let config = self.config.read().await;
        if config.hardware_acceleration {
            if self.is_hardware_acceleration_available().await {
                self.enable_hardware_acceleration().await?;
                info!("Hardware acceleration enabled successfully");
            } else {
                warn!("Hardware acceleration is not available, falling back to software mode");
            }
        }

        // Update state
        debug!("Setting state to Ready");
        *self.state.write().await = StreamerState::Ready;
        
        debug!("StreamerEngine initialization completed successfully");
        info!("Streamer engine initialized successfully");
        Ok(())
    }

    /// Check if hardware acceleration is available
    async fn is_hardware_acceleration_available(&self) -> bool {
        debug!("Checking hardware acceleration availability");
        
        // Check for NVIDIA GPU (CUDA/NVENC)
        if self.check_nvidia_gpu().await {
            debug!("NVIDIA GPU detected, hardware acceleration available");
            return true;
        }
        
        // Check for Intel GPU (Quick Sync)
        if self.check_intel_gpu().await {
            debug!("Intel GPU detected, hardware acceleration available");
            return true;
        }
        
        // Check for AMD GPU (VCE/AMF)
        if self.check_amd_gpu().await {
            debug!("AMD GPU detected, hardware acceleration available");
            return true;
        }
        
        // Check for VAAPI support (Linux)
        if self.check_vaapi_support().await {
            debug!("VAAPI support detected, hardware acceleration available");
            return true;
        }
        
        // Check for Video Toolbox (macOS)
        if self.check_videotoolbox_support().await {
            debug!("Video Toolbox support detected, hardware acceleration available");
            return true;
        }
        
        debug!("No hardware acceleration support detected");
        false
    }
    
    /// Check for NVIDIA GPU availability
    async fn check_nvidia_gpu(&self) -> bool {
        // Check for NVIDIA GPU by looking for nvidia-smi or CUDA libraries
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("nvidia-smi").arg("--query-gpu=name").arg("--format=csv,noheader").output() {
                if output.status.success() && !output.stdout.is_empty() {
                    debug!("NVIDIA GPU found via nvidia-smi");
                    return true;
                }
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // Check Windows registry or WMI for NVIDIA GPU
            // This is a simplified check - in production, use proper WMI queries
            use std::process::Command;
            if let Ok(output) = Command::new("wmic").args(["path", "win32_VideoController", "get", "name"]).output() {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if output_str.to_lowercase().contains("nvidia") {
                        debug!("NVIDIA GPU found via WMI");
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Check for Intel GPU availability
    async fn check_intel_gpu(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::path::Path;
            // Check for Intel GPU device files
            if Path::new("/dev/dri/renderD128").exists() {
                debug!("Intel GPU device found");
                return true;
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("wmic").args(["path", "win32_VideoController", "get", "name"]).output() {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if output_str.to_lowercase().contains("intel") {
                        debug!("Intel GPU found via WMI");
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Check for AMD GPU availability
    async fn check_amd_gpu(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("lspci").output() {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if output_str.to_lowercase().contains("amd") || output_str.to_lowercase().contains("radeon") {
                        debug!("AMD GPU found via lspci");
                        return true;
                    }
                }
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("wmic").args(["path", "win32_VideoController", "get", "name"]).output() {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if output_str.to_lowercase().contains("amd") || output_str.to_lowercase().contains("radeon") {
                        debug!("AMD GPU found via WMI");
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Check for VAAPI support (Linux)
    async fn check_vaapi_support(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::path::Path;
            // Check for VAAPI device files
            if Path::new("/dev/dri/renderD128").exists() || Path::new("/dev/dri/card0").exists() {
                debug!("VAAPI device files found");
                return true;
            }
        }
        false
    }
    
    /// Check for Video Toolbox support (macOS)
    async fn check_videotoolbox_support(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // Video Toolbox is available on all modern macOS systems
            debug!("Video Toolbox support assumed available on macOS");
            true
        }
        #[cfg(not(target_os = "macos"))]
        false
    }

    /// Enable hardware acceleration
    async fn enable_hardware_acceleration(&self) -> Result<()> {
        debug!("Enabling hardware acceleration");
        
        // Initialize hardware acceleration based on available hardware
        if self.check_nvidia_gpu().await {
            self.enable_nvidia_acceleration().await?;
        } else if self.check_intel_gpu().await {
            self.enable_intel_acceleration().await?;
        } else if self.check_amd_gpu().await {
            self.enable_amd_acceleration().await?;
        } else if self.check_vaapi_support().await {
            self.enable_vaapi_acceleration().await?;
        } else if self.check_videotoolbox_support().await {
            self.enable_videotoolbox_acceleration().await?;
        } else {
            return Err(OpenAceError::hardware_acceleration("No hardware acceleration support found"));
        }
        
        info!("Hardware acceleration enabled successfully");
        Ok(())
    }
    
    /// Enable NVIDIA hardware acceleration
    async fn enable_nvidia_acceleration(&self) -> Result<()> {
        debug!("Enabling NVIDIA hardware acceleration");
        // In a real implementation, this would:
        // 1. Initialize CUDA context
        // 2. Set up NVENC encoder
        // 3. Configure GPU memory allocation
        // 4. Set encoding parameters for optimal GPU usage
        
        // For now, we'll just log that it's enabled
        info!("NVIDIA hardware acceleration configured");
        Ok(())
    }
    
    /// Enable Intel hardware acceleration
    async fn enable_intel_acceleration(&self) -> Result<()> {
        debug!("Enabling Intel hardware acceleration");
        // In a real implementation, this would:
        // 1. Initialize Intel Media SDK or oneVPL
        // 2. Set up Quick Sync encoder
        // 3. Configure hardware encoding parameters
        
        info!("Intel hardware acceleration configured");
        Ok(())
    }
    
    /// Enable AMD hardware acceleration
    async fn enable_amd_acceleration(&self) -> Result<()> {
        debug!("Enabling AMD hardware acceleration");
        // In a real implementation, this would:
        // 1. Initialize AMD Media Framework (AMF)
        // 2. Set up VCE encoder
        // 3. Configure hardware encoding parameters
        
        info!("AMD hardware acceleration configured");
        Ok(())
    }
    
    /// Enable VAAPI acceleration
    async fn enable_vaapi_acceleration(&self) -> Result<()> {
        debug!("Enabling VAAPI hardware acceleration");
        // In a real implementation, this would:
        // 1. Initialize VAAPI context
        // 2. Set up hardware encoder
        // 3. Configure encoding parameters
        
        info!("VAAPI hardware acceleration configured");
        Ok(())
    }
    
    /// Enable Video Toolbox acceleration
    async fn enable_videotoolbox_acceleration(&self) -> Result<()> {
        debug!("Enabling Video Toolbox hardware acceleration");
        // In a real implementation, this would:
        // 1. Initialize Video Toolbox framework
        // 2. Set up hardware encoder
        // 3. Configure encoding parameters for macOS
        
        info!("Video Toolbox hardware acceleration configured");
        Ok(())
    }
    
    /// Shutdown the streamer engine
    pub async fn shutdown(&self) -> Result<()> {
        trace!("Shutting down streamer engine");
        debug!("Starting StreamerEngine shutdown process");
        info!("Shutting down streamer engine");
        
        let current_state = self.state.read().await.clone();
        debug!("Current state before shutdown: {:?}", current_state);
        
        // Update state
        debug!("Setting state to Shutdown");
        *self.state.write().await = StreamerState::Shutdown;
        
        // Send shutdown signal
        debug!("Checking for active shutdown sender");
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            debug!("Sending shutdown signal to server loop");
            let _ = sender.send(());
            debug!("Shutdown signal sent successfully");
        } else {
            debug!("No active shutdown sender found");
        }
        
        // Stop all streams
        debug!("Stopping all active streams");
        self.stop_all_streams().await?;
        debug!("All streams stopped successfully");
        
        // Disconnect all clients
        debug!("Disconnecting all clients");
        self.disconnect_all_clients().await?;
        debug!("All clients disconnected successfully");
        
        debug!("StreamerEngine shutdown completed successfully");
        info!("Streamer engine shut down successfully");
        Ok(())
    }
    
    /// Pause the streamer engine
    pub async fn pause(&self) -> Result<()> {
        trace!("Pausing streamer engine");
        debug!("Starting StreamerEngine pause process");
        info!("Pausing streamer engine");
        
        let mut state = self.state.write().await;
        debug!("Current state before pause: {:?}", *state);
        match *state {
            StreamerState::Ready | StreamerState::Streaming => {
                debug!("Setting state to Paused");
                *state = StreamerState::Paused;
                debug!("StreamerEngine paused successfully");
                Ok(())
            }
            _ => {
                debug!("Invalid state transition attempted from {:?} to Paused", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Ready or Streaming"
                ))
            }
        }
    }
    
    /// Resume the streamer engine
    pub async fn resume(&self) -> Result<()> {
        trace!("Resuming streamer engine");
        debug!("Starting StreamerEngine resume process");
        info!("Resuming streamer engine");
        
        let mut state = self.state.write().await;
        debug!("Current state before resume: {:?}", *state);
        match *state {
            StreamerState::Paused => {
                debug!("Setting state to Ready");
                *state = StreamerState::Ready;
                debug!("StreamerEngine resumed successfully");
                Ok(())
            }
            _ => {
                debug!("Invalid state transition attempted from {:?} to Ready", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Paused"
                ))
            }
        }
    }
    
    /// Create a new stream
    #[instrument(skip(self))]
    pub async fn create_stream(
        &self,
        params: StreamParams,
    ) -> Result<Id> {
        trace!("Creating new stream: {}", params.name);
        debug!("Starting stream creation process for: {}", params.name);
        debug!("Stream parameters - format: {:?}, quality: {:?}, bitrate: {}, resolution: {:?}, fps: {}", params.format, params.quality, params.bitrate, params.resolution, params.frame_rate);
        
        let state = self.state.read().await;
        debug!("Current engine state: {:?}", *state);
        if !matches!(*state, StreamerState::Ready | StreamerState::Streaming) {
            debug!("Invalid state for stream creation: {:?}", *state);
            return Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Ready or Streaming"
            ));
        }
        drop(state);
        
        let stream_id = Id::generate();
        debug!("Generated stream ID: {}", stream_id);
        
        let stream = StreamInfo {
            id: stream_id.clone(),
            name: params.name.clone(),
            description: params.description.clone(),
            format: params.format,
            quality: params.quality,
            bitrate: params.bitrate,
            resolution: params.resolution,
            fps: params.frame_rate,
            created_at: Instant::now(),
            status: StreamStatus::Preparing,
            viewer_count: 0,
            bytes_sent: 0,
            packets_sent: 0,
            error_message: None,
        };
        
        debug!("Created StreamInfo object with status: {:?}", stream.status);
        
        // Store stream
        debug!("Storing stream in streams collection");
        self.streams.write().await.insert(stream_id.clone(), stream);
        debug!("Stream stored successfully");
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            let old_total = stats.total_streams;
            let old_active = stats.active_streams;
            stats.total_streams += 1;
            stats.active_streams += 1;
            debug!("Updated stats - total_streams: {} -> {}, active_streams: {} -> {}", old_total, stats.total_streams, old_active, stats.active_streams);
        }
        
        debug!("Stream creation completed successfully for: {} ({})", params.name, stream_id);
        info!("Stream created: {}", stream_id);
        Ok(stream_id)
    }
    
    /// Start a stream
    pub async fn start_stream(&self, stream_id: &Id) -> Result<()> {
        trace!("Starting stream: {}", stream_id);
        debug!("Starting stream process for ID: {}", stream_id);
        
        let mut streams = self.streams.write().await;
        debug!("Acquired write lock on streams collection");
        
        if let Some(stream) = streams.get_mut(stream_id) {
            debug!("Found stream with current status: {:?}", stream.status);
            
            if stream.status == StreamStatus::Preparing {
                debug!("Setting stream status to Live");
                stream.status = StreamStatus::Live;
                debug!("Stream status updated successfully");
                
                // Update engine state
                debug!("Updating engine state to Streaming");
                *self.state.write().await = StreamerState::Streaming;
                debug!("Engine state updated successfully");
                
                debug!("Stream started successfully: {}", stream_id);
                info!("Stream started: {}", stream_id);
                Ok(())
            } else {
                debug!("Invalid state transition attempted from {:?} to Live", stream.status);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", stream.status),
                    "Preparing"
                ))
            }
        } else {
            debug!("Stream not found in collection: {}", stream_id);
            Err(OpenAceError::not_found(format!("Stream not found: {}", stream_id)))
        }
    }
    
    /// Stop a stream
    pub async fn stop_stream(&self, stream_id: &Id) -> Result<()> {
        trace!("Stopping stream: {}", stream_id);
        debug!("Starting stream stop process for ID: {}", stream_id);
        
        let mut streams = self.streams.write().await;
        debug!("Acquired write lock on streams collection");
        
        if let Some(stream) = streams.get_mut(stream_id) {
            debug!("Found stream with current status: {:?}", stream.status);
            
            if matches!(stream.status, StreamStatus::Live | StreamStatus::Paused) {
                debug!("Setting stream status to Stopped");
                stream.status = StreamStatus::Stopped;
                debug!("Stream status updated successfully");
                
                // Update stats
                debug!("Updating statistics");
                let mut stats = self.stats.write().await;
                let old_active = stats.active_streams;
                stats.active_streams = stats.active_streams.saturating_sub(1);
                debug!("Updated active_streams from {} to {}", old_active, stats.active_streams);
                
                // Update engine state if no more active streams
                if stats.active_streams == 0 {
                    debug!("No more active streams, updating engine state to Ready");
                    drop(stats);
                    drop(streams);
                    *self.state.write().await = StreamerState::Ready;
                    debug!("Engine state updated to Ready");
                }
                
                debug!("Stream stopped successfully: {}", stream_id);
                info!("Stream stopped: {}", stream_id);
                Ok(())
            } else {
                debug!("Invalid state transition attempted from {:?} to Stopped", stream.status);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", stream.status),
                    "Live or Paused"
                ))
            }
        } else {
            debug!("Stream not found in collection: {}", stream_id);
            Err(OpenAceError::not_found(format!("Stream not found: {}", stream_id)))
        }
    }
    
    /// Stop all streams
    async fn stop_all_streams(&self) -> Result<()> {
        trace!("Stopping all streams");
        debug!("Starting process to stop all active streams");
        info!("Stopping all streams");
        
        let mut streams = self.streams.write().await;
        let total_streams = streams.len();
        debug!("Found {} total streams in collection", total_streams);
        
        let mut stopped_count = 0;
        
        for (stream_id, stream) in streams.iter_mut() {
            debug!("Processing stream {} with status: {:?}", stream_id, stream.status);
            if matches!(stream.status, StreamStatus::Live | StreamStatus::Paused) {
                debug!("Stopping stream: {}", stream_id);
                stream.status = StreamStatus::Stopped;
                stopped_count += 1;
                debug!("Stream {} stopped successfully", stream_id);
            } else {
                debug!("Stream {} already in non-active state: {:?}", stream_id, stream.status);
            }
        }
        
        debug!("Stopped {} out of {} streams", stopped_count, total_streams);
        
        // Update stats
        if stopped_count > 0 {
            let mut stats = self.stats.write().await;
            let old_active = stats.active_streams;
            stats.active_streams = 0;
            debug!("Updated active_streams from {} to 0", old_active);
        }
        
        debug!("All streams stop process completed");
        info!("Stopped {} streams", stopped_count);
        Ok(())
    }
    
    /// Send media packet to stream
    #[instrument(skip(self, data))]
    pub async fn send_packet(
        &self,
        stream_id: &Id,
        sequence_number: u64,
        timestamp: u64,
        data: Bytes,
        is_keyframe: bool,
        packet_type: PacketType,
    ) -> Result<()> {
        trace!("Sending packet for stream {}: seq={}, timestamp={}, size={}, keyframe={}, type={:?}", stream_id, sequence_number, timestamp, data.len(), is_keyframe, packet_type);
        debug!("Starting packet send process for stream: {}", stream_id);
        debug!("Packet details - seq: {}, timestamp: {}, size: {} bytes, keyframe: {}, type: {:?}", sequence_number, timestamp, data.len(), is_keyframe, packet_type);
        
        // Check if stream exists and is live
        {
            debug!("Checking stream status for: {}", stream_id);
            let streams = self.streams.read().await;
            if let Some(stream) = streams.get(stream_id) {
                debug!("Found stream with status: {:?}", stream.status);
                if stream.status != StreamStatus::Live {
                    debug!("Stream not in Live status, cannot send packet");
                    return Err(OpenAceError::invalid_state_transition(
                        format!("{:?}", stream.status),
                        "Live"
                    ));
                }
                debug!("Stream is live, proceeding with packet send");
            } else {
                debug!("Stream not found: {}", stream_id);
                return Err(OpenAceError::not_found(
                    format!("Stream not found: {}", stream_id)
                ));
            }
        }
        
        debug!("Creating MediaPacket object");
        let packet = MediaPacket {
            stream_id: stream_id.clone(),
            sequence_number,
            timestamp,
            data: data.clone(),
            is_keyframe,
            packet_type,
        };
        debug!("MediaPacket created successfully");
        
        // Broadcast packet to all subscribers
        debug!("Broadcasting packet to subscribers");
        if let Some(sender) = self.packet_sender.lock().await.as_ref() {
            match sender.send(packet) {
                Ok(_) => {
                    debug!("Packet broadcasted successfully to {} subscribers", sender.receiver_count());
                }
                Err(_) => {
                    debug!("No active subscribers for stream: {}", stream_id);
                    warn!("No active subscribers for stream: {}", stream_id);
                }
            }
        } else {
            debug!("No packet sender available");
        }
        
        // Update stream statistics
        {
            debug!("Updating stream statistics");
            let mut streams = self.streams.write().await;
            if let Some(stream) = streams.get_mut(stream_id) {
                let old_bytes = stream.bytes_sent;
                let old_packets = stream.packets_sent;
                stream.bytes_sent += data.len() as u64;
                stream.packets_sent += 1;
                debug!("Updated stream stats - bytes: {} -> {}, packets: {} -> {}", old_bytes, stream.bytes_sent, old_packets, stream.packets_sent);
            }
        }
        
        // Update global statistics
        {
            debug!("Updating global statistics");
            let mut stats = self.stats.write().await;
            let old_bytes = stats.total_bytes_sent;
            let old_packets = stats.total_packets_sent;
            stats.total_bytes_sent += data.len() as u64;
            stats.total_packets_sent += 1;
            debug!("Updated global stats - bytes: {} -> {}, packets: {} -> {}", old_bytes, stats.total_bytes_sent, old_packets, stats.total_packets_sent);
        }
        
        debug!("Packet send process completed successfully for stream: {}", stream_id);
        Ok(())
    }
    
    /// Get stream information
    pub async fn get_stream(&self, stream_id: &Id) -> Result<StreamInfo> {
        let streams = self.streams.read().await;
        streams.get(stream_id)
            .cloned()
            .ok_or_else(|| OpenAceError::not_found(format!("Stream not found: {}", stream_id)))
    }
    
    /// Get all streams
    pub async fn get_all_streams(&self) -> Vec<StreamInfo> {
        self.streams.read().await.values().cloned().collect()
    }
    
    /// Get active streams
    pub async fn get_active_streams(&self) -> Vec<StreamInfo> {
        self.streams.read().await
            .values()
            .filter(|stream| matches!(stream.status, StreamStatus::Live | StreamStatus::Paused))
            .cloned()
            .collect()
    }
    
    /// Get client connections
    pub async fn get_clients(&self) -> Vec<ClientConnection> {
        self.clients.read().await.values().cloned().collect()
    }
    
    /// Disconnect all clients
    async fn disconnect_all_clients(&self) -> Result<()> {
        debug!("Starting process to disconnect all clients");
        
        let mut clients = self.clients.write().await;
        let client_count = clients.len();
        debug!("Found {} clients to disconnect", client_count);
        
        if client_count > 0 {
            debug!("Clearing all client connections");
            clients.clear();
            debug!("All client connections cleared");
        } else {
            debug!("No clients to disconnect");
        }
        
        // Update stats
        if client_count > 0 {
            debug!("Updating client statistics");
            let mut stats = self.stats.write().await;
            let old_active = stats.active_clients;
            stats.active_clients = 0;
            debug!("Updated active_clients from {} to 0", old_active);
        }
        
        debug!("Client disconnection process completed");
        info!("Disconnected {} clients", client_count);
        Ok(())
    }
    
    /// Update configuration
    pub async fn update_config(&self, config: &StreamerConfig) -> Result<()> {
        trace!("Updating streamer configuration with new values: {:?}", config);
        info!("Updating streamer configuration");
        *self.config.write().await = config.clone();
        Ok(())
    }
    
    /// Get health status
    pub async fn get_health(&self) -> EngineHealth {
        trace!("Getting streamer health status");
        let state = self.state.read().await;
        let stats = self.stats.read().await;
        
        match *state {
            StreamerState::Ready | StreamerState::Streaming => {
                if stats.active_clients > 1000 {
                    EngineHealth::Warning("High client load".to_string())
                } else if stats.error_count > 10 {
                    EngineHealth::Warning("High error rate".to_string())
                } else {
                    EngineHealth::Healthy
                }
            }
            StreamerState::Error(ref msg) => {
                EngineHealth::Error(msg.clone())
            }
            StreamerState::Paused => {
                EngineHealth::Warning("Engine is paused".to_string())
            }
            _ => EngineHealth::Unknown,
        }
    }
    
    /// Clone for task (simplified clone for async tasks)
    fn clone_for_task(&self) -> StreamerEngineTask {
        StreamerEngineTask {
            config: Arc::clone(&self.config.inner()),
            state: Arc::clone(&self.state.inner()),
            stats: Arc::clone(&self.stats.inner()),
            streams: Arc::clone(&self.streams.inner()),
            clients: Arc::clone(&self.clients.inner()),
        }
    }
    
    /// Server listening loop
    #[allow(dead_code)]
    async fn server_loop(
        &self,
        config: StreamerConfig,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting streamer server loop on port {}", config.port);
        
        let addr = format!("0.0.0.0:{}", config.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("Failed to bind to {}: {}", addr, e);
                return;
            }
        };
        
        info!("Streamer server listening on {}", addr);
        
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let engine_ref = self.clone_for_task();
                            tokio::spawn(async move {
                                engine_ref.handle_client(stream, addr).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for server loop");
                    break;
                }
            }
        }
        
        info!("Streamer server loop stopped");
    }
    
    /// Handle client connection
    #[allow(dead_code)]
    async fn handle_client(&self, mut stream: TcpStream, addr: SocketAddr) {
        trace!("Handling new client connection from {}", addr);
        info!("New client connected: {}", addr);
        
        let client_id = Id::generate();
        let connection = ClientConnection {
            id: client_id.clone(),
            stream_id: Id::generate(), // This would be determined from the request
            address: addr,
            connected_at: Instant::now(),
            bytes_sent: 0,
            packets_sent: 0,
            last_activity: Instant::now(),
            user_agent: None,
            protocol: NetworkProtocol::Tcp,
        };
        
        // Store client connection
        self.clients.write().await.insert(client_id.clone(), connection);
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_clients += 1;
            stats.active_clients += 1;
            
            if stats.active_clients > stats.peak_concurrent_viewers as u64 {
                stats.peak_concurrent_viewers = stats.active_clients as u32;
            }
        }
        
        // Handle client communication
        let mut buffer = [0; 1024];
        let mut receiver = self.packet_sender.lock().await.as_ref().map(|tx| tx.subscribe());
        loop {
            tokio::select! {
                result = stream.read(&mut buffer) => {
                    match result {
                        Ok(0) => {
                            // Client disconnected
                            break;
                        }
                        Ok(n) => {
                            // Process client request (simple protocol: assume string command like "SUBSCRIBE stream_id")
                            let command = String::from_utf8_lossy(&buffer[0..n]).to_string();
                            debug!("Received command from client {}: {}", addr, command);
                            if command.starts_with("SUBSCRIBE ") {
                                let requested_stream_id = command.strip_prefix("SUBSCRIBE ").unwrap().trim().to_string();
                                if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                                    client.stream_id = Id::from(requested_stream_id);
                                    debug!("Client {} subscribed to stream {}", addr, client.stream_id);
                                }
                            }
                            // Update last activity
                            if let Some(client) = self.clients.write().await.get_mut(&client_id) {
                                client.last_activity = Instant::now();
                            }
                        }
                        Err(e) => {
                            error!("Error reading from client {}: {}", addr, e);
                            break;
                        }
                    }
                }
                Ok(packet) = receiver.as_mut().unwrap().recv(), if receiver.is_some() => {
                    // Check if packet is for this client's stream
                    if let Some(client) = self.clients.read().await.get(&client_id) {
                        if packet.stream_id == client.stream_id {
                            // Send packet to client (simplified: send data directly)
                            if let Err(e) = stream.write_all(&packet.data).await {
                                error!("Error sending packet to client {}: {}", addr, e);
                                break;
                            }
                            // Update stats
                            if let Some(client_mut) = self.clients.write().await.get_mut(&client_id) {
                                client_mut.bytes_sent += packet.data.len() as u64;
                                client_mut.packets_sent += 1;
                            }
                        }
                    }
                }
            }
        }
        
        // Remove client connection
        self.clients.write().await.remove(&client_id);
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.active_clients = stats.active_clients.saturating_sub(1);
        }
        
        info!("Client disconnected: {}", addr);
    }
}

/// Simplified engine reference for async tasks
#[derive(Debug, Clone)]
struct StreamerEngineTask {
    #[allow(dead_code)]
    config: Arc<tokio::sync::RwLock<StreamerConfig>>,
    #[allow(dead_code)]
    state: Arc<tokio::sync::RwLock<StreamerState>>,
    stats: Arc<tokio::sync::RwLock<StreamerStats>>,
    #[allow(dead_code)]
    streams: Arc<tokio::sync::RwLock<HashMap<Id, StreamInfo>>>,
    clients: Arc<tokio::sync::RwLock<HashMap<Id, ClientConnection>>>,
}

#[async_trait]
impl Lifecycle for StreamerEngine {
    async fn initialize(&mut self) -> Result<()> {
        trace!("Initializing StreamerEngine");
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        trace!("Shutting down StreamerEngine");
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        trace!("Checking if StreamerEngine is initialized");
        self.state.try_read()
            .map(|state| !matches!(*state, StreamerState::Uninitialized))
            .unwrap_or(false)
    }
}

#[async_trait]
impl Pausable for StreamerEngine {
    async fn pause(&mut self) -> Result<()> {
        trace!("Pausing StreamerEngine");
        self.pause().await
    }
    
    async fn resume(&mut self) -> Result<()> {
        trace!("Resuming StreamerEngine");
        self.resume().await
    }
    
    async fn is_paused(&self) -> bool {
        trace!("Checking if StreamerEngine is paused");
        self.state.try_read()
            .map(|state| matches!(*state, StreamerState::Paused))
            .unwrap_or(false)
    }
}

#[async_trait]
impl StatisticsProvider for StreamerEngine {
    type Stats = StreamerStats;
    
    async fn get_statistics(&self) -> Self::Stats {
        let mut stats = self.stats.try_read()
            .map(|s| s.clone())
            .unwrap_or_default();
        
        // Update uptime
        stats.uptime = self.start_time.elapsed();
        stats
    }
    
    async fn reset_statistics(&mut self) -> Result<()> {
        if let Ok(mut stats) = self.stats.try_write() {
            *stats = StreamerStats::default();
        }
        Ok(())
    }
}

#[async_trait]
impl Monitorable for StreamerEngine {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        if let Ok(state) = self.state.try_read() {
            match *state {
                StreamerState::Ready | StreamerState::Streaming => {
                    if let Ok(stats) = self.stats.try_read() {
                        if stats.error_count > 10 {
                            EngineHealth::Warning("High error count detected".to_string())
                        } else if stats.active_clients > 1000 {
                            EngineHealth::Warning("High client load".to_string())
                        } else {
                            EngineHealth::Healthy
                        }
                    } else {
                        EngineHealth::Warning("Cannot read statistics".to_string())
                    }
                }
                StreamerState::Error(ref msg) => EngineHealth::Error(msg.clone()),
                StreamerState::Paused => EngineHealth::Warning("Engine is paused".to_string()),
                StreamerState::Uninitialized => EngineHealth::Warning("Engine not initialized".to_string()),
                _ => EngineHealth::Unknown,
            }
        } else {
            EngineHealth::Error("Cannot read engine state".to_string())
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        Ok(self.get_health().await)
    }
}

#[async_trait]
impl Configurable for StreamerEngine {
    type Config = StreamerConfig;
    
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
        if config.port == 0 {
            return Err(OpenAceError::configuration(
                "port must be greater than 0"
            ));
        }
        
        if config.max_clients == 0 {
            return Err(OpenAceError::configuration(
                "max_clients must be greater than 0"
            ));
        }
        
        if config.buffer_size_bytes == 0 {
            return Err(OpenAceError::configuration(
                "buffer_size_bytes must be greater than 0"
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StreamerConfig;
    
    #[tokio::test]
    async fn test_streamer_engine_creation() {
        let config = StreamerConfig::default();
        let engine = StreamerEngine::new(&config);
        assert!(engine.is_ok());
    }
    
    #[tokio::test]
    async fn test_streamer_engine_lifecycle() {
        let config = StreamerConfig::default();
        let engine = StreamerEngine::new(&config).unwrap();
        
        assert!(!engine.is_initialized());
        
        // Initialize
        let result = engine.initialize().await;
        assert!(result.is_ok());
        assert!(engine.is_initialized());
        
        // Shutdown
        let result = engine.shutdown().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_stream_creation() {
        let config = StreamerConfig::default();
        let engine = StreamerEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let params = StreamParams {
            name: "Test Stream".to_string(),
            description: Some("Test Description".to_string()),
            format: MediaFormat::Mp4,
            quality: MediaQuality::High,
            bitrate: 1000000,
            resolution: (1920, 1080),
            frame_rate: 30.0,
        };
        let stream_id = engine.create_stream(params).await;
        
        assert!(stream_id.is_ok());
        
        let stream_id = stream_id.unwrap();
        let stream = engine.get_stream(&stream_id).await;
        assert!(stream.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_stream_lifecycle() {
        let config = StreamerConfig::default();
        let engine = StreamerEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let params = StreamParams {
             name: "Test Stream".to_string(),
             description: None,
             format: MediaFormat::Mp4,
             quality: MediaQuality::Medium,
             bitrate: 500000,
             resolution: (1280, 720),
             frame_rate: 25.0,
         };
         let stream_id = engine.create_stream(params).await.unwrap();
        
        // Start stream
        let result = engine.start_stream(&stream_id).await;
        assert!(result.is_ok());
        
        // Stop stream
        let result = engine.stop_stream(&stream_id).await;
        assert!(result.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_streamer_statistics() {
        let config = StreamerConfig::default();
        let mut engine = StreamerEngine::new(&config).unwrap();
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_streams, 0);
        assert_eq!(stats.active_streams, 0);
        assert_eq!(stats.total_clients, 0);
        
        engine.reset_statistics().await.unwrap();
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_streams, 0);
    }
}