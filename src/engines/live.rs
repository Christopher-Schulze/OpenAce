//! Live Engine for OpenAce Rust
//!
//! This module implements the live engine that handles real-time streaming,
//! live broadcast management, and viewer interaction with modern async patterns.

use crate::error::{Result, OpenAceError};
use crate::config::LiveConfig;
use crate::core::traits::*;
use crate::core::types::*;
use crate::utils::threading::{SafeMutex, SafeRwLock};
use crate::engines::EngineHealth;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, broadcast};
use tracing::{debug, info, warn, error, instrument, trace};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Live engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiveState {
    Uninitialized,
    Initializing,
    Ready,
    Broadcasting,
    Paused,
    Error(String),
    Shutdown,
}

/// Live broadcast information
#[derive(Debug, Clone)]
pub struct LiveBroadcast {
    pub id: Id,
    pub title: String,
    pub description: Option<String>,
    pub broadcaster_id: Id,
    pub broadcaster_name: String,
    pub category: String,
    pub tags: Vec<String>,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub bitrate: u32,
    pub status: BroadcastStatus,
    pub created_at: Instant,
    pub started_at: Option<Instant>,
    pub ended_at: Option<Instant>,
    pub viewer_count: u32,
    pub peak_viewers: u32,
    pub total_views: u64,
    pub duration: Duration,
    pub is_private: bool,
    pub is_mature: bool,
    pub thumbnail_url: Option<String>,
    pub stream_key: String,
}

/// Broadcast status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BroadcastStatus {
    Created,
    Starting,
    Live,
    Paused,
    Ending,
    Ended,
    Error(String),
}

/// Parameters for creating a broadcast
#[derive(Debug, Clone)]
pub struct BroadcastParams {
    pub title: String,
    pub description: Option<String>,
    pub broadcaster_id: Id,
    pub broadcaster_name: String,
    pub category: String,
    pub tags: Vec<String>,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub bitrate: u32,
    pub is_private: bool,
    pub is_mature: bool,
}

/// Live viewer information
#[derive(Debug, Clone)]
pub struct LiveViewer {
    pub id: Id,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub broadcast_id: Id,
    pub joined_at: Instant,
    pub last_activity: Instant,
    pub is_moderator: bool,
    pub is_subscriber: bool,
    pub is_vip: bool,
    pub chat_color: Option<String>,
    pub badges: Vec<String>,
    pub connection_quality: ConnectionQuality,
    pub device_info: Option<DeviceInfo>,
}

/// Connection quality
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Fair,
    Poor,
    Unknown,
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_type: String,
    pub os: String,
    pub browser: Option<String>,
    pub resolution: Option<(u32, u32)>,
    pub user_agent: Option<String>,
}

/// Chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: Id,
    pub broadcast_id: Id,
    pub viewer_id: Id,
    pub username: String,
    pub message: String,
    pub timestamp: Instant,
    pub message_type: ChatMessageType,
    pub is_deleted: bool,
    pub emotes: Vec<Emote>,
    pub mentions: Vec<String>,
}

/// Serializable chat message for events (without Instant fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableChatMessage {
    pub id: Id,
    pub broadcast_id: Id,
    pub viewer_id: Id,
    pub username: String,
    pub message: String,
    pub message_type: ChatMessageType,
    pub is_deleted: bool,
    pub emotes: Vec<Emote>,
    pub mentions: Vec<String>,
}

impl From<ChatMessage> for SerializableChatMessage {
    fn from(chat_message: ChatMessage) -> Self {
        SerializableChatMessage {
            id: chat_message.id,
            broadcast_id: chat_message.broadcast_id,
            viewer_id: chat_message.viewer_id,
            username: chat_message.username,
            message: chat_message.message,
            message_type: chat_message.message_type,
            is_deleted: chat_message.is_deleted,
            emotes: chat_message.emotes,
            mentions: chat_message.mentions,
        }
    }
}

/// Chat message type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChatMessageType {
    Normal,
    Whisper,
    System,
    Moderator,
    Announcement,
    Donation,
    Subscription,
}

/// Emote information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Emote {
    pub id: String,
    pub name: String,
    pub url: String,
    pub start_index: usize,
    pub end_index: usize,
}

/// Live statistics
#[derive(Debug, Clone)]
pub struct LiveStats {
    pub total_broadcasts: u64,
    pub active_broadcasts: u64,
    pub total_viewers: u64,
    pub concurrent_viewers: u64,
    pub peak_concurrent_viewers: u64,
    pub total_chat_messages: u64,
    pub average_view_duration: Duration,
    pub total_stream_time: Duration,
    pub bandwidth_usage: u64,
    pub storage_usage: u64,
    pub error_count: u64,
    pub uptime: Duration,
    pub memory_usage: u64,
}

impl Default for LiveStats {
    fn default() -> Self {
        Self {
            total_broadcasts: 0,
            active_broadcasts: 0,
            total_viewers: 0,
            concurrent_viewers: 0,
            peak_concurrent_viewers: 0,
            total_chat_messages: 0,
            average_view_duration: Duration::ZERO,
            total_stream_time: Duration::ZERO,
            bandwidth_usage: 0,
            storage_usage: 0,
            error_count: 0,
            uptime: Duration::ZERO,
            memory_usage: 0,
        }
    }
}

/// Live event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiveEvent {
    BroadcastStarted { broadcast_id: Id, title: String },
    BroadcastEnded { broadcast_id: Id, duration: Duration },
    ViewerJoined { broadcast_id: Id, viewer_id: Id, username: Option<String> },
    ViewerLeft { broadcast_id: Id, viewer_id: Id },
    ChatMessage { broadcast_id: Id, message: SerializableChatMessage },
    Donation { broadcast_id: Id, viewer_id: Id, amount: f64, currency: String, message: Option<String> },
    Subscription { broadcast_id: Id, viewer_id: Id, tier: u32, months: u32 },
    Follow { broadcast_id: Id, viewer_id: Id },
    Raid { from_broadcast_id: Id, to_broadcast_id: Id, viewer_count: u32 },
    ModeratorAction { broadcast_id: Id, moderator_id: Id, action: String, target_id: Option<Id> },
}

/// Live engine implementation
#[derive(Debug)]
pub struct LiveEngine {
    config: SafeRwLock<LiveConfig>,
    state: SafeRwLock<LiveState>,
    stats: SafeRwLock<LiveStats>,
    broadcasts: SafeRwLock<HashMap<Id, LiveBroadcast>>,
    viewers: SafeRwLock<HashMap<Id, LiveViewer>>,
    chat_messages: SafeRwLock<Vec<ChatMessage>>,
    event_sender: SafeMutex<Option<broadcast::Sender<LiveEvent>>>,
    shutdown_sender: SafeMutex<Option<oneshot::Sender<()>>>,
    start_time: Instant,
}

impl LiveEngine {
    /// Create a new live engine
    pub fn new(config: &LiveConfig) -> Result<Self> {
        info!("Creating live engine");
        
        Ok(Self {
            config: SafeRwLock::new(config.clone(), "live_config"),
            state: SafeRwLock::new(LiveState::Uninitialized, "live_state"),
            stats: SafeRwLock::new(LiveStats::default(), "live_stats"),
            broadcasts: SafeRwLock::new(HashMap::new(), "live_broadcasts"),
            viewers: SafeRwLock::new(HashMap::new(), "live_viewers"),
            chat_messages: SafeRwLock::new(Vec::new(), "live_chat_messages"),
            event_sender: SafeMutex::new(None, "live_event_sender"),
            shutdown_sender: SafeMutex::new(None, "live_shutdown_sender"),
            start_time: Instant::now(),
        })
    }
    
    /// Initialize the live engine
    pub async fn initialize(&self) -> Result<()> {
        trace!("Entering initialize method");
        debug!("Current state: {:?}", *self.state.read().await);
        info!("Initializing live engine");
        
        {
            let mut state = self.state.write().await;
            if *state != LiveState::Uninitialized {
            return Err(OpenAceError::invalid_state_transition(
                format!("Current state: {:?}", *state),
                "Uninitialized"
            ));
            }
            *state = LiveState::Initializing;
            debug!("State changed to Initializing");
            trace!("Live engine state transition: Uninitialized -> Initializing");
        }
        
        // Create event broadcast channel
        let (event_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        debug!("Event broadcast channel created with capacity: 1000");
        trace!("Event broadcaster created and stored");
        
        // Store senders
        *self.event_sender.lock().await = Some(event_tx);
        *self.shutdown_sender.lock().await = Some(shutdown_tx);
        debug!("Senders stored");
        trace!("Event sender stored in engine state");
        trace!("Shutdown channel created for monitoring loop");
        info!("Communication channels established successfully");
        
        // Start monitoring tasks
        let engine_ref = self.clone_for_task();
        trace!("Creating monitoring task with engine reference");
        tokio::spawn(async move {
            debug!("Monitoring loop task started");
            engine_ref.monitoring_loop(shutdown_rx).await;
            debug!("Monitoring loop task completed");
        });
        debug!("Monitoring loop spawned");
        info!("Background monitoring task initialized");
        
        // Start chat cleanup task
        let engine_ref = self.clone_for_task();
        trace!("Creating chat cleanup task with engine reference");
        tokio::spawn(async move {
            debug!("Chat cleanup loop task started");
            engine_ref.chat_cleanup_loop().await;
            debug!("Chat cleanup loop task completed");
        });
        debug!("Chat cleanup loop spawned");
        info!("Chat cleanup background task initialized");
        
        // Update state and statistics
        {
            let mut state = self.state.write().await;
            *state = LiveState::Ready;
            debug!("State changed to Ready");
            trace!("Live engine state transition: Initializing -> Ready");
        }
        
        // Update initialization statistics
        {
            let mut stats = self.stats.write().await;
            stats.uptime = self.start_time.elapsed();
            trace!("Updated uptime statistics: {:?}", stats.uptime);
            debug!("Live engine statistics updated after initialization");
        }

        info!("Live engine initialized successfully with all background tasks running");
        Ok(())
    }
    
    /// Shutdown the live engine
    pub async fn shutdown(&self) -> Result<()> {
        trace!("Entering shutdown method");
        debug!("Current state: {:?}", *self.state.read().await);
        info!("Shutting down live engine");
        
        // Update state
        {
            let mut state = self.state.write().await;
            *state = LiveState::Shutdown;
            debug!("State changed to Shutdown");
        }
        
        // Send shutdown signal
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            let _ = sender.send(());
            debug!("Shutdown signal sent");
        }
        
        // End all active broadcasts
        if let Err(e) = self.end_all_broadcasts().await {
            error!("Error ending broadcasts: {}", e);
        } else {
            debug!("All broadcasts ended");
        }
        
        // Clear all viewers
        if let Err(e) = self.clear_all_viewers().await {
            error!("Error clearing viewers: {}", e);
        } else {
            debug!("All viewers cleared");
        }
        
        info!("Live engine shut down successfully");
        Ok(())
    }
    
    /// Pause the live engine
    pub async fn pause(&self) -> Result<()> {
        trace!("Entering pause method");
        info!("Pausing live engine");
        
        let mut state = self.state.write().await;
        match *state {
            LiveState::Ready | LiveState::Broadcasting => {
                *state = LiveState::Paused;
                Ok(())
            }
            _ => Err(OpenAceError::invalid_state_transition(
                format!("Current state: {:?}", *state),
                "Ready or Broadcasting"
            ))
        }
    }
    
    /// Resume the live engine
    pub async fn resume(&self) -> Result<()> {
        trace!("Entering resume method");
        info!("Resuming live engine");
        
        let mut state = self.state.write().await;
        match *state {
            LiveState::Paused => {
                *state = LiveState::Ready;
                Ok(())
            }
            _ => Err(OpenAceError::invalid_state_transition(
                format!("Current state: {:?}", *state),
                "Paused"
            ))
        }
    }
    
    /// Create a new broadcast
    #[instrument(skip(self))]
    pub async fn create_broadcast(
        &self,
        params: BroadcastParams,
    ) -> Result<Id> {
        trace!("Entering create_broadcast with title: {}", params.title);
        debug!("Parameters: broadcaster_id={}, category={}, tags={:?}, format={:?}, quality={:?}", params.broadcaster_id, params.category, params.tags, params.format, params.quality);
        
        // Check state
        let state = self.state.read().await;
        debug!("Current state: {:?}", *state);
        if !matches!(*state, LiveState::Ready | LiveState::Broadcasting) {
            return Err(OpenAceError::invalid_state_transition(
                format!("Current state: {:?}", *state),
                "Ready or Broadcasting"
            ));
        }
        drop(state);
        
        let broadcast_id = Id::generate();
        debug!("Generated broadcast_id: {}", broadcast_id);
        let stream_key = Uuid::new_v4().to_string();
        trace!("Generated stream_key for broadcast: {}", stream_key);
        debug!("Broadcast configuration: resolution={}x{}, fps={}, bitrate={}", params.resolution.0, params.resolution.1, params.fps, params.bitrate);
        
        let broadcast = LiveBroadcast {
            id: broadcast_id.clone(),
            title: params.title.clone(),
            description: params.description,
            broadcaster_id: params.broadcaster_id,
            broadcaster_name: params.broadcaster_name.clone(),
            category: params.category.clone(),
            tags: params.tags,
            format: params.format,
            quality: params.quality,
            resolution: params.resolution,
            fps: params.fps,
            bitrate: params.bitrate,
            status: BroadcastStatus::Created,
            created_at: Instant::now(),
            started_at: None,
            ended_at: None,
            viewer_count: 0,
            peak_viewers: 0,
            total_views: 0,
            duration: Duration::ZERO,
            is_private: params.is_private,
            is_mature: params.is_mature,
            thumbnail_url: None,
            stream_key,
        };
        debug!("Broadcast object created with status: {:?}", BroadcastStatus::Created);
        trace!("Broadcast details: private={}, mature={}, category={}", params.is_private, params.is_mature, params.category);
        
        // Store broadcast
        {
            let mut broadcasts = self.broadcasts.write().await;
            let broadcast_count_before = broadcasts.len();
            broadcasts.insert(broadcast_id.clone(), broadcast);
            let broadcast_count_after = broadcasts.len();
            debug!("Broadcast inserted into map (count: {} -> {})", broadcast_count_before, broadcast_count_after);
            trace!("Broadcast storage operation completed successfully");
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            let old_count = stats.total_broadcasts;
            stats.total_broadcasts += 1;
            debug!("Total broadcasts updated: {} -> {}", old_count, stats.total_broadcasts);
            trace!("Broadcast statistics updated successfully");
        }
        
        info!("Broadcast created successfully: {} - '{}' by {}", broadcast_id, params.title, params.broadcaster_name);
        trace!("create_broadcast completed successfully for broadcast_id: {}", broadcast_id);
        Ok(broadcast_id)
    }
    
    /// Start a broadcast
    pub async fn start_broadcast(&self, broadcast_id: &Id) -> Result<()> {
        trace!("Entering start_broadcast for {}", broadcast_id);
        debug!("Starting broadcast lifecycle for stream: {}", broadcast_id);
        
        debug!("Checking broadcast existence and acquiring write lock");
        let mut broadcasts = self.broadcasts.write().await;
        if let Some(broadcast) = broadcasts.get_mut(broadcast_id) {
            debug!("Broadcast found, current status: {:?}, title: '{}', broadcaster: {}", 
                   broadcast.status, broadcast.title, broadcast.broadcaster_name);
            trace!("Broadcast details: format={:?}, quality={:?}, resolution={}x{}, fps={}, bitrate={}",
                   broadcast.format, broadcast.quality, broadcast.resolution.0, broadcast.resolution.1,
                   broadcast.fps, broadcast.bitrate);
            
            if broadcast.status == BroadcastStatus::Created {
                debug!("Transitioning broadcast status from Created to Live");
                broadcast.status = BroadcastStatus::Live;
                let start_time = Instant::now();
                broadcast.started_at = Some(start_time);
                debug!("Broadcast status changed to Live, started_at set to {:?}", start_time);
                trace!("Stream lifecycle: broadcast {} is now live and accepting viewers", broadcast_id);
                
                // Update engine state
                debug!("Updating engine state to Broadcasting");
                *self.state.write().await = LiveState::Broadcasting;
                debug!("Engine state changed to Broadcasting");
                trace!("Live engine now in broadcasting mode");
                
                // Update stats
                {
                    debug!("Updating broadcast statistics");
                    let mut stats = self.stats.write().await;
                    let prev_active = stats.active_broadcasts;
                    stats.active_broadcasts += 1;
                    debug!("Active broadcasts updated from {} to {}", prev_active, stats.active_broadcasts);
                    trace!("Statistics update: total_broadcasts={}, active_broadcasts={}", 
                           stats.total_broadcasts, stats.active_broadcasts);
                }
                
                // Send event
                debug!("Sending BroadcastStarted event for stream lifecycle tracking");
                self.send_event(LiveEvent::BroadcastStarted {
                    broadcast_id: broadcast_id.clone(),
                    title: broadcast.title.clone(),
                }).await;
                debug!("BroadcastStarted event sent successfully");
                trace!("Stream lifecycle event dispatched: BroadcastStarted for {}", broadcast_id);
                
                info!("Broadcast started successfully: {} - '{}' by {}", 
                      broadcast_id, broadcast.title, broadcast.broadcaster_name);
                Ok(())
            } else {
                warn!("Cannot start broadcast {} - invalid status transition from {:?} to Live", 
                      broadcast_id, broadcast.status);
                Err(OpenAceError::invalid_state_transition(
                    "Broadcast is not in created state",
                    "Live"
                ))
            }
        } else {
            error!("Cannot start broadcast - broadcast not found: {}", broadcast_id);
            Err(OpenAceError::not_found(format!("Broadcast not found: {}", broadcast_id)))
        }
    }
    
    /// End a broadcast
    pub async fn end_broadcast(&self, broadcast_id: &Id) -> Result<()> {
        trace!("Entering end_broadcast for {}", broadcast_id);
        debug!("Checking broadcast existence");
        let mut broadcasts = self.broadcasts.write().await;
        if let Some(broadcast) = broadcasts.get_mut(broadcast_id) {
            debug!("Broadcast found, current status: {:?}", broadcast.status);
            if matches!(broadcast.status, BroadcastStatus::Live | BroadcastStatus::Paused) {
                let duration = broadcast.started_at
                    .map(|start| start.elapsed())
                    .unwrap_or(Duration::ZERO);
                
                broadcast.status = BroadcastStatus::Ended;
                broadcast.ended_at = Some(Instant::now());
                broadcast.duration = duration;
                debug!("Broadcast status changed to Ended, ended_at set, duration set to {:?}", duration);
                
                // Update stats
                {
                    let mut stats = self.stats.write().await;
                    stats.active_broadcasts = stats.active_broadcasts.saturating_sub(1);
                    stats.total_stream_time += duration;
                    debug!("Statistics updated: active_broadcasts={}, total_stream_time={:?}", stats.active_broadcasts, stats.total_stream_time);
                    
                    // Update engine state if no more active broadcasts
                    if stats.active_broadcasts == 0 {
                        drop(stats);
                        drop(broadcasts);
                        *self.state.write().await = LiveState::Ready;
                        debug!("Engine state changed to Ready");
                    }
                }
                
                // Send event
                self.send_event(LiveEvent::BroadcastEnded {
                    broadcast_id: broadcast_id.clone(),
                    duration,
                }).await;
                debug!("BroadcastEnded event sent");
                
                // Remove all viewers from this broadcast
                self.remove_viewers_from_broadcast(broadcast_id).await?;
                debug!("Viewers removed from broadcast");
                
                info!("Broadcast ended: {} (duration: {:?})", broadcast_id, duration);
                Ok(())
            } else {
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", broadcast.status),
                    "Ended"
                ))
            }
        } else {
            Err(OpenAceError::not_found(format!("Broadcast not found: {}", broadcast_id)))
        }
    }
    
    /// End all broadcasts
    async fn end_all_broadcasts(&self) -> Result<()> {
        let mut broadcasts = self.broadcasts.write().await;
        let mut ended_count = 0;
        
        for broadcast in broadcasts.values_mut() {
            if matches!(broadcast.status, BroadcastStatus::Live | BroadcastStatus::Paused) {
                let duration = broadcast.started_at
                    .map(|start| start.elapsed())
                    .unwrap_or(Duration::ZERO);
                
                broadcast.status = BroadcastStatus::Ended;
                broadcast.ended_at = Some(Instant::now());
                broadcast.duration = duration;
                ended_count += 1;
            }
        }
        
        // Update stats
        if ended_count > 0 {
            let mut stats = self.stats.write().await;
            stats.active_broadcasts = 0;
        }
        
        info!("Ended {} broadcasts", ended_count);
        Ok(())
    }
    
    /// Add viewer to broadcast
    #[instrument(skip(self))]
    pub async fn add_viewer(
        &self,
        broadcast_id: &Id,
        username: Option<String>,
        display_name: Option<String>,
        device_info: Option<DeviceInfo>,
    ) -> Result<Id> {
        trace!("Entering add_viewer for broadcast {}", broadcast_id);
        debug!("Parameters: username={:?}, display_name={:?}, device_info={:?}", username, display_name, device_info);
        if let Some(ref device) = device_info {
            trace!("Viewer device info: type={}, os={}, browser={:?}, resolution={:?}", 
                   device.device_type, device.os, device.browser, device.resolution);
        }
        
        // Check if broadcast exists and is live
        debug!("Checking broadcast existence and status");
        {
            let broadcasts = self.broadcasts.read().await;
            if let Some(broadcast) = broadcasts.get(broadcast_id) {
                debug!("Broadcast found, status: {:?}, title: '{}', broadcaster: {}", 
                       broadcast.status, broadcast.title, broadcast.broadcaster_name);
                trace!("Broadcast details: viewer_count={}, peak_viewers={}, total_views={}",
                       broadcast.viewer_count, broadcast.peak_viewers, broadcast.total_views);
                if broadcast.status != BroadcastStatus::Live {
                    warn!("Cannot add viewer to broadcast {} - invalid status: {:?}", broadcast_id, broadcast.status);
                    return Err(OpenAceError::invalid_state_transition(
                        format!("{:?}", broadcast.status),
                        "Live"
                    ));
                }
            } else {
                error!("Cannot add viewer - broadcast not found: {}", broadcast_id);
                return Err(OpenAceError::not_found(
                format!("Broadcast not found: {}", broadcast_id)
            ));
            }
        }
        
        let viewer_id = Id::generate();
        debug!("Generated viewer_id: {}", viewer_id);
        let connection_time = Instant::now();
        let viewer = LiveViewer {
            id: viewer_id.clone(),
            username: username.clone(),
            display_name: display_name.clone(),
            broadcast_id: broadcast_id.clone(),
            joined_at: connection_time,
            last_activity: connection_time,
            is_moderator: false,
            is_subscriber: false,
            is_vip: false,
            chat_color: None,
            badges: Vec::new(),
            connection_quality: ConnectionQuality::Unknown,
            device_info: device_info.clone(),
        };
        debug!("Viewer object created with connection_time: {:?}", connection_time);
        trace!("Viewer instance: id={}, username={:?}, display_name={:?}, connection_quality={:?}",
               viewer.id, viewer.username, viewer.display_name, viewer.connection_quality);
        
        // Store viewer
        debug!("Storing viewer in global viewers map");
        self.viewers.write().await.insert(viewer_id.clone(), viewer);
        debug!("Viewer {} inserted into global map", viewer_id);
        trace!("Viewer connection established for broadcast {}", broadcast_id);
        
        // Update broadcast viewer count
        debug!("Updating broadcast viewer statistics");
        {
            let mut broadcasts = self.broadcasts.write().await;
            if let Some(broadcast) = broadcasts.get_mut(broadcast_id) {
                let prev_count = broadcast.viewer_count;
                let prev_total = broadcast.total_views;
                let prev_peak = broadcast.peak_viewers;
                
                broadcast.viewer_count += 1;
                broadcast.total_views += 1;
                
                if broadcast.viewer_count > broadcast.peak_viewers {
                    broadcast.peak_viewers = broadcast.viewer_count;
                }
                debug!("Broadcast stats updated: viewer_count {} -> {}, total_views {} -> {}, peak_viewers {} -> {}", 
                       prev_count, broadcast.viewer_count, prev_total, broadcast.total_views, 
                       prev_peak, broadcast.peak_viewers);
                trace!("Stream {} now has {} active viewers (peak: {})", 
                       broadcast_id, broadcast.viewer_count, broadcast.peak_viewers);
            }
        }
        
        // Update global stats
        debug!("Updating global viewer statistics");
        {
            let mut stats = self.stats.write().await;
            let prev_total = stats.total_viewers;
            let prev_concurrent = stats.concurrent_viewers;
            let prev_peak = stats.peak_concurrent_viewers;
            
            stats.total_viewers += 1;
            stats.concurrent_viewers += 1;
            
            if stats.concurrent_viewers > stats.peak_concurrent_viewers {
                stats.peak_concurrent_viewers = stats.concurrent_viewers;
            }
            debug!("Global stats updated: total_viewers {} -> {}, concurrent_viewers {} -> {}, peak_concurrent_viewers {} -> {}", 
                   prev_total, stats.total_viewers, prev_concurrent, stats.concurrent_viewers, 
                   prev_peak, stats.peak_concurrent_viewers);
            trace!("Platform now has {} concurrent viewers across all streams (peak: {})",
                   stats.concurrent_viewers, stats.peak_concurrent_viewers);
        }
        
        // Send event
        debug!("Sending ViewerJoined event for connection tracking");
        self.send_event(LiveEvent::ViewerJoined {
            broadcast_id: broadcast_id.clone(),
            viewer_id: viewer_id.clone(),
            username: username.clone(),
        }).await;
        debug!("ViewerJoined event sent successfully");
        trace!("Connection event dispatched: ViewerJoined for {} -> {}", viewer_id, broadcast_id);
        
        info!("Viewer {} successfully joined broadcast {} (username: {:?}, device: {:?})", 
              viewer_id, broadcast_id, username, 
              device_info.as_ref().map(|d| &d.device_type));
        Ok(viewer_id)
    }
    
    /// Remove viewer from broadcast
    pub async fn remove_viewer(&self, viewer_id: &Id) -> Result<()> {
        debug!("Removing viewer: {}", viewer_id);
        trace!("Starting viewer disconnection process");
        
        // Get viewer info before removal
        debug!("Retrieving viewer information before removal");
        let viewer = {
            let viewers = self.viewers.read().await;
            viewers.get(viewer_id).cloned()
        };
        
        if let Some(viewer) = viewer {
            debug!("Viewer found, broadcast_id: {}, username: {:?}, joined_at: {:?}", 
                   viewer.broadcast_id, viewer.username, viewer.joined_at);
            let session_duration = viewer.joined_at.elapsed();
            trace!("Viewer session duration: {:?}, last_activity: {:?}", 
                   session_duration, viewer.last_activity);
            
            // Remove from viewers map
            debug!("Removing viewer from global viewers map");
            self.viewers.write().await.remove(viewer_id);
            debug!("Viewer {} removed from global map", viewer_id);
            trace!("Viewer cleanup: removed from global registry");
            
            // Update broadcast viewer count
            debug!("Updating broadcast statistics for viewer removal");
            {
                let mut broadcasts = self.broadcasts.write().await;
                if let Some(broadcast) = broadcasts.get_mut(&viewer.broadcast_id) {
                    let prev_count = broadcast.viewer_count;
                    broadcast.viewer_count = broadcast.viewer_count.saturating_sub(1);
                    debug!("Broadcast viewer_count updated from {} to {} for stream {}", 
                           prev_count, broadcast.viewer_count, viewer.broadcast_id);
                    trace!("Stream {} now has {} active viewers", 
                           viewer.broadcast_id, broadcast.viewer_count);
                } else {
                    warn!("Broadcast {} not found during viewer removal stats update", viewer.broadcast_id);
                }
            }
            
            // Update stats
            debug!("Updating global viewer statistics");
            {
                let mut stats = self.stats.write().await;
                let prev_concurrent = stats.concurrent_viewers;
                stats.concurrent_viewers = stats.concurrent_viewers.saturating_sub(1);
                debug!("Global concurrent_viewers updated from {} to {}", 
                       prev_concurrent, stats.concurrent_viewers);
                trace!("Platform now has {} concurrent viewers across all streams", 
                       stats.concurrent_viewers);
                
                // Calculate view duration
                let view_duration = viewer.joined_at.elapsed();
                // Update average view duration (simplified calculation)
                if stats.total_viewers > 0 {
                    stats.average_view_duration = 
                        (stats.average_view_duration + view_duration) / 2;
                }
                debug!("View duration calculated: {:?}, average updated to {:?}", 
                       view_duration, stats.average_view_duration);
            }
            
            // Send event
            debug!("Sending ViewerLeft event for disconnection tracking");
            self.send_event(LiveEvent::ViewerLeft {
                broadcast_id: viewer.broadcast_id.clone(),
                viewer_id: viewer_id.clone(),
            }).await;
            debug!("ViewerLeft event sent successfully");
            trace!("Disconnection event dispatched: ViewerLeft for {} <- {}", 
                   viewer.broadcast_id, viewer_id);
            
            info!("Viewer {} left broadcast {} after {:?} (username: {:?})", 
                  viewer_id, viewer.broadcast_id, session_duration, viewer.username);
            Ok(())
        } else {
            error!("Cannot remove viewer - viewer not found: {}", viewer_id);
            Err(OpenAceError::not_found(format!("Viewer not found: {}", viewer_id)))
        }
    }
    
    /// Remove all viewers from a broadcast
    async fn remove_viewers_from_broadcast(&self, broadcast_id: &Id) -> Result<()> {
        let mut viewers_to_remove = Vec::new();
        
        {
            let viewers = self.viewers.read().await;
            for (viewer_id, viewer) in viewers.iter() {
                if viewer.broadcast_id == *broadcast_id {
                    viewers_to_remove.push(viewer_id.clone());
                }
            }
        }
        
        for viewer_id in viewers_to_remove {
            self.remove_viewer(&viewer_id).await?;
        }
        
        Ok(())
    }
    
    /// Clear all viewers
    async fn clear_all_viewers(&self) -> Result<()> {
        let viewer_count = self.viewers.read().await.len();
        self.viewers.write().await.clear();
        
        // Update stats
        if viewer_count > 0 {
            let mut stats = self.stats.write().await;
            stats.concurrent_viewers = 0;
        }
        
        info!("Cleared {} viewers", viewer_count);
        Ok(())
    }
    
    /// Send chat message
    #[instrument(skip(self, message))]
    pub async fn send_chat_message(
        &self,
        broadcast_id: &Id,
        viewer_id: &Id,
        message: String,
        message_type: ChatMessageType,
    ) -> Result<Id> {
        debug!("Sending chat message from {} to broadcast {}", viewer_id, broadcast_id);
        trace!("Message content length: {} characters, type: {:?}", message.len(), message_type);
        
        // Check if broadcast exists and is live
        debug!("Verifying broadcast exists and is live");
        {
            let broadcasts = self.broadcasts.read().await;
            if let Some(broadcast) = broadcasts.get(broadcast_id) {
                debug!("Broadcast found: title='{}', status={:?}, viewer_count={}", 
                       broadcast.title, broadcast.status, broadcast.viewer_count);
                if broadcast.status != BroadcastStatus::Live {
                    warn!("Chat message rejected - broadcast {} not live (status: {:?})", 
                          broadcast_id, broadcast.status);
                    return Err(OpenAceError::invalid_state_transition(
                        format!("{:?}", broadcast.status),
                        "Live"
                    ));
                }
                trace!("Broadcast status check passed: {:?}", broadcast.status);
            } else {
                error!("Cannot send chat message - broadcast not found: {}", broadcast_id);
                return Err(OpenAceError::not_found(
                    format!("Broadcast not found: {}", broadcast_id)
                ));
            }
        }
        
        // Check if viewer exists
        debug!("Verifying viewer exists and retrieving username");
        let username = {
            let viewers = self.viewers.read().await;
            if let Some(viewer) = viewers.get(viewer_id) {
                let username = viewer.username.clone().unwrap_or_else(|| "Anonymous".to_string());
                debug!("Viewer found: username='{}', display_name={:?}, joined_at={:?}", 
                       username, viewer.display_name, viewer.joined_at);
                trace!("Viewer details: is_moderator={}, is_subscriber={}, connection_quality={:?}", 
                       viewer.is_moderator, viewer.is_subscriber, viewer.connection_quality);
                username
            } else {
                error!("Cannot send chat message - viewer not found: {}", viewer_id);
                return Err(OpenAceError::not_found(
                    format!("Viewer not found: {}", viewer_id)
                ));
            }
        };
        
        debug!("Creating chat message instance");
        let message_id = Id::generate();
        let timestamp = Instant::now();
        let chat_message = ChatMessage {
            id: message_id.clone(),
            broadcast_id: broadcast_id.clone(),
            viewer_id: viewer_id.clone(),
            username: username.clone(),
            message: message.clone(),
            timestamp,
            message_type: message_type.clone(),
            is_deleted: false,
            emotes: self.parse_emotes(&message),
            mentions: self.parse_mentions(&message)
        };
        
        trace!("Chat message created: id={}, username='{}', type={:?}, timestamp={:?}", 
               message_id, username, message_type, timestamp);
        
        // Store chat message
        debug!("Storing chat message in memory");
        {
            let mut messages = self.chat_messages.write().await;
            let message_count_before = messages.len();
            messages.push(chat_message.clone());
            debug!("Message storage: {} -> {} total messages", message_count_before, messages.len());
            trace!("Chat message stored successfully: id={}", message_id);
        }
        
        // Update stats
        debug!("Updating global chat statistics");
        {
            let mut stats = self.stats.write().await;
            let old_total = stats.total_chat_messages;
            stats.total_chat_messages += 1;
            debug!("Global message count: {} -> {}", old_total, stats.total_chat_messages);
            trace!("Global stats: broadcasts={}, viewers={}, messages={}", 
                   stats.total_broadcasts, stats.concurrent_viewers, stats.total_chat_messages);
        }
        
        // Send event
        debug!("Sending ChatMessage event");
        trace!("Event payload: broadcast={}, viewer={}, message={}, type={:?}", 
               broadcast_id, viewer_id, message_id, message_type);
        
        self.send_event(LiveEvent::ChatMessage {
            broadcast_id: broadcast_id.clone(),
            message: chat_message.clone().into(),
        }).await;
        
        trace!("ChatMessage event sent successfully");
        info!("Chat message sent: {} from {} in broadcast {}", message_id, viewer_id, broadcast_id);
        debug!("Chat message processing completed successfully: message_id={}", message_id);
        Ok(message_id)
    }
    
    /// Get broadcast information
    pub async fn get_broadcast(&self, broadcast_id: &Id) -> Result<LiveBroadcast> {
        let broadcasts = self.broadcasts.read().await;
        broadcasts.get(broadcast_id)
            .cloned()
            .ok_or_else(|| OpenAceError::not_found(format!("Broadcast not found: {}", broadcast_id)))
    }
    
    /// Get all broadcasts
    pub async fn get_all_broadcasts(&self) -> Vec<LiveBroadcast> {
        self.broadcasts.read().await.values().cloned().collect()
    }
    
    /// Get active broadcasts
    pub async fn get_active_broadcasts(&self) -> Vec<LiveBroadcast> {
        self.broadcasts.read().await
            .values()
            .filter(|broadcast| matches!(broadcast.status, BroadcastStatus::Live | BroadcastStatus::Paused))
            .cloned()
            .collect()
    }
    
    /// Get viewers for a broadcast
    pub async fn get_broadcast_viewers(&self, broadcast_id: &Id) -> Vec<LiveViewer> {
        self.viewers.read().await
            .values()
            .filter(|viewer| viewer.broadcast_id == *broadcast_id)
            .cloned()
            .collect()
    }
    
    /// Get chat messages for a broadcast
    pub async fn get_chat_messages(&self, broadcast_id: &Id, limit: Option<usize>) -> Vec<ChatMessage> {
        let messages = self.chat_messages.read().await;
        let filtered: Vec<_> = messages
            .iter()
            .filter(|msg| msg.broadcast_id == *broadcast_id && !msg.is_deleted)
            .cloned()
            .collect();
        
        if let Some(limit) = limit {
            filtered.into_iter().rev().take(limit).rev().collect()
        } else {
            filtered
        }
    }
    
    /// Subscribe to live events
    pub async fn subscribe_to_events(&self) -> Result<broadcast::Receiver<LiveEvent>> {
        if let Some(sender) = self.event_sender.lock().await.as_ref() {
            Ok(sender.subscribe())
        } else {
            Err(OpenAceError::invalid_state_transition("Event sender not available", "Initialized"))
        }
    }
    
    /// Send live event
    async fn send_event(&self, event: LiveEvent) {
        if let Some(sender) = self.event_sender.lock().await.as_ref() {
            if sender.send(event).is_err() {
                debug!("No active event subscribers");
            }
        }
    }
    
    /// Update configuration
    pub async fn update_config(&self, config: &LiveConfig) -> Result<()> {
        trace!("Entering update_config with config: {:?}", config);
        info!("Updating live configuration");
        *self.config.write().await = config.clone();
        Ok(())
    }
    
    /// Get health status
    pub async fn get_health(&self) -> EngineHealth {
        let state = self.state.read().await;
        let stats = self.stats.read().await;
        
        match *state {
            LiveState::Ready | LiveState::Broadcasting => {
                if stats.concurrent_viewers > 10000 {
                    EngineHealth::Warning("High viewer load".to_string())
                } else if stats.error_count > 50 {
                    EngineHealth::Warning("High error rate".to_string())
                } else {
                    EngineHealth::Healthy
                }
            }
            LiveState::Error(ref msg) => {
                EngineHealth::Error(msg.clone())
            }
            LiveState::Paused => {
                EngineHealth::Warning("Engine is paused".to_string())
            }
            _ => EngineHealth::Unknown,
        }
    }
    
    /// Clone for task (simplified clone for async tasks)
    fn clone_for_task(&self) -> LiveEngineTask {
        LiveEngineTask {
            config: Arc::clone(&self.config.inner()),
            state: Arc::clone(&self.state.inner()),
            stats: Arc::clone(&self.stats.inner()),
            broadcasts: Arc::clone(&self.broadcasts.inner()),
            viewers: Arc::clone(&self.viewers.inner()),
            chat_messages: Arc::clone(&self.chat_messages.inner()),
            start_time: self.start_time,
        }
    }
    
    /// Monitoring loop
    #[allow(dead_code)]
    async fn monitoring_loop(&self, mut shutdown_rx: oneshot::Receiver<()>) {
        info!("Starting live engine monitoring loop");
        
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.update_statistics().await {
                        error!("Error updating statistics: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for monitoring loop");
                    break;
                }
            }
        }
        
        info!("Live engine monitoring loop stopped");
    }
    
    /// Update statistics
    #[allow(dead_code)]
    async fn update_statistics(&self) -> Result<()> {
        let mut stats = self.stats.write().await;
        
        // Update uptime
        stats.uptime = self.start_time.elapsed();
        
        // Update memory usage (simplified)
        stats.memory_usage = (stats.concurrent_viewers * 1024) + (stats.total_chat_messages * 256);
        
        Ok(())
    }
    
    /// Chat cleanup loop
    #[allow(dead_code)]
    async fn chat_cleanup_loop(&self) {
        info!("Starting live engine chat cleanup loop");
        
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
        
        loop {
            interval.tick().await;
            
            // Check if we should stop
            {
                let state = self.state.read().await;
                if matches!(*state, LiveState::Shutdown) {
                    break;
                }
            }
            
            // Clean up old chat messages
            if let Err(e) = self.cleanup_old_chat_messages().await {
                error!("Error cleaning up chat messages: {}", e);
            }
        }
        
        info!("Live engine chat cleanup loop stopped");
    }
    
    /// Clean up old chat messages
    #[allow(dead_code)]
    async fn cleanup_old_chat_messages(&self) -> Result<()> {
        // Use a sane default TTL for chat history since it's not configurable here
        const CHAT_HISTORY_TTL_SECS: u64 = 3600; // 1 hour
        let max_age = Duration::from_secs(CHAT_HISTORY_TTL_SECS);
        let now = Instant::now();
        
        let mut messages = self.chat_messages.write().await;
        let initial_count = messages.len();
        
        messages.retain(|msg| now.duration_since(msg.timestamp) < max_age);
        
        let removed_count = initial_count - messages.len();
        if removed_count > 0 {
            debug!("Cleaned up {} old chat messages", removed_count);
        }
        
        Ok(())
    }
    
    /// Parse emotes from chat message
    fn parse_emotes(&self, message: &str) -> Vec<Emote> {
        let mut emotes = Vec::new();
        
        // Common emote patterns - this is a basic implementation
        // In a real implementation, this would use a proper emote database
        let emote_patterns = [
            (":)", "smile"),
            (":(", "sad"),
            (":D", "laugh"),
            (":P", "tongue"),
            (":o", "surprised"),
            (";)", "wink"),
            ("<3", "heart"),
            (":|", "neutral"),
            (":/", "confused"),
            (":*", "kiss"),
        ];
        
        for (pattern, name) in &emote_patterns {
            let mut start = 0;
            while let Some(pos) = message[start..].find(pattern) {
                let actual_start = start + pos;
                let actual_end = actual_start + pattern.len();
                
                emotes.push(Emote {
                    id: format!("{}_{}", name, actual_start),
                    name: name.to_string(),
                    url: format!("https://emotes.example.com/{}.png", name),
                    start_index: actual_start,
                    end_index: actual_end,
                });
                
                start = actual_end;
            }
        }
        
        emotes
    }
    
    /// Parse mentions from chat message
    fn parse_mentions(&self, message: &str) -> Vec<String> {
        let mut mentions = Vec::new();
        
        // Find @username patterns
        let mut chars = message.char_indices().peekable();
        
        while let Some((i, ch)) = chars.next() {
            if ch == '@' {
                let mut username = String::new();
                let mut valid_mention = true;
                
                // Collect username characters (alphanumeric + underscore)
                while let Some((_, next_ch)) = chars.peek() {
                    if next_ch.is_alphanumeric() || *next_ch == '_' {
                        username.push(*next_ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                
                // Validate mention (must have at least one character and not be too long)
                if username.is_empty() || username.len() > 25 {
                    valid_mention = false;
                }
                
                // Check if it's at word boundary (not preceded by alphanumeric)
                if i > 0 {
                    if let Some(prev_char) = message.chars().nth(i - 1) {
                        if prev_char.is_alphanumeric() {
                            valid_mention = false;
                        }
                    }
                }
                
                if valid_mention && !username.is_empty() {
                    mentions.push(username);
                }
            }
        }
        
        // Remove duplicates while preserving order
        let mut unique_mentions = Vec::new();
        for mention in mentions {
            if !unique_mentions.contains(&mention) {
                unique_mentions.push(mention);
            }
        }
        
        unique_mentions
    }
}

/// Simplified engine reference for async tasks
#[derive(Debug, Clone)]
struct LiveEngineTask {
    #[allow(dead_code)]
    config: Arc<tokio::sync::RwLock<LiveConfig>>,
    state: Arc<tokio::sync::RwLock<LiveState>>,
    stats: Arc<tokio::sync::RwLock<LiveStats>>,
    #[allow(dead_code)]
    broadcasts: Arc<tokio::sync::RwLock<HashMap<Id, LiveBroadcast>>>,
    #[allow(dead_code)]
    viewers: Arc<tokio::sync::RwLock<HashMap<Id, LiveViewer>>>,
    chat_messages: Arc<tokio::sync::RwLock<Vec<ChatMessage>>>,
    start_time: Instant,
}

impl LiveEngineTask {
    /// Monitoring loop (task clone)
    async fn monitoring_loop(&self, mut shutdown_rx: oneshot::Receiver<()>) {
        info!("Starting live engine monitoring loop");
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.update_statistics().await {
                        error!("Error updating statistics: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for monitoring loop");
                    break;
                }
            }
        }
        info!("Live engine monitoring loop stopped");
    }

    /// Chat cleanup loop (task clone)
    async fn chat_cleanup_loop(&self) {
        info!("Starting live engine chat cleanup loop");
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            // Stop condition
            {
                let state = self.state.read().await;
                if matches!(*state, LiveState::Shutdown) {
                    break;
                }
            }
            if let Err(e) = self.cleanup_old_chat_messages().await {
                error!("Error cleaning up chat messages: {}", e);
            }
        }
        info!("Live engine chat cleanup loop stopped");
    }

    async fn cleanup_old_chat_messages(&self) -> Result<()> {
        const CHAT_HISTORY_TTL_SECS: u64 = 3600; // 1 hour
        let max_age = Duration::from_secs(CHAT_HISTORY_TTL_SECS);
        let now = Instant::now();
        let mut messages = self.chat_messages.write().await;
        let initial_count = messages.len();
        messages.retain(|msg| now.duration_since(msg.timestamp) < max_age);
        let removed_count = initial_count - messages.len();
        if removed_count > 0 { debug!("Cleaned up {} old chat messages", removed_count); }
        Ok(())
    }

    async fn update_statistics(&self) -> Result<()> {
        let mut stats = self.stats.write().await;
        stats.uptime = self.start_time.elapsed();
        stats.memory_usage = (stats.concurrent_viewers * 1024) + (stats.total_chat_messages * 256);
        Ok(())
    }
}

#[async_trait]
impl Lifecycle for LiveEngine {
    async fn initialize(&mut self) -> Result<()> {
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        self.state.try_read()
            .map(|state| !matches!(*state, LiveState::Uninitialized))
            .unwrap_or(false)
    }
    
    async fn force_shutdown(&mut self) -> Result<()> {
        warn!("Force shutdown requested for LiveEngine");
        
        // Cancel all active broadcasts immediately
        let broadcast_ids: Vec<Id> = {
            let broadcasts = self.broadcasts.read().await;
            broadcasts.keys().cloned().collect()
        };
        
        for broadcast_id in broadcast_ids {
            if let Err(e) = self.end_broadcast(&broadcast_id).await {
                error!("Error ending broadcast {} during force shutdown: {}", broadcast_id, e);
            }
        }
        
        // Force state to shutdown
        {
            let mut state = self.state.write().await;
            *state = LiveState::Shutdown;
        }
        
        // Send shutdown signal to monitoring task
        if let Some(shutdown_tx) = self.shutdown_sender.lock().await.take() {
            let _ = shutdown_tx.send(());
        }
        
        info!("LiveEngine force shutdown completed");
        Ok(())
    }
    
    fn get_initialization_status(&self) -> crate::core::traits::InitializationStatus {
        use crate::core::traits::InitializationStatus;
        
        match self.state.try_read() {
            Ok(state) => {
                match *state {
                    LiveState::Uninitialized => InitializationStatus::Uninitialized,
            LiveState::Initializing => InitializationStatus::Initializing,
            LiveState::Ready | LiveState::Broadcasting | LiveState::Paused => InitializationStatus::Initialized,
                    LiveState::Error(ref msg) => InitializationStatus::Failed(msg.clone()),
                    LiveState::Shutdown => InitializationStatus::Failed("Engine shutdown".to_string()),
                }
            },
            Err(_) => InitializationStatus::Failed("Failed to read state".to_string())
        }
    }
}

#[async_trait]
impl Pausable for LiveEngine {
    async fn pause(&mut self) -> Result<()> {
        self.pause().await
    }
    
    async fn resume(&mut self) -> Result<()> {
        self.resume().await
    }
    
    async fn is_paused(&self) -> bool {
        self.state.try_read()
            .map(|state| matches!(*state, LiveState::Paused))
            .unwrap_or(false)
    }
    
    async fn toggle_pause(&mut self) -> Result<()> {
        if self.is_paused().await {
            self.resume().await
        } else {
            self.pause().await
        }
    }
    
    async fn get_pause_duration(&self) -> Option<Duration> {
        // For LiveEngine, we track pause duration in stats
        self.stats.try_read()
            .ok()
            .and_then(|stats| {
                if matches!(*self.state.try_read().ok()?, LiveState::Paused) {
                    Some(stats.uptime) // Simplified - in real implementation, track pause start time
                } else {
                    None
                }
            })
    }
}

#[async_trait]
impl StatisticsProvider for LiveEngine {
    type Stats = LiveStats;
    
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
            *stats = LiveStats::default();
        }
        Ok(())
    }
    
    async fn get_detailed_statistics(&self) -> HashMap<String, crate::core::traits::StatisticValue> {
        use crate::core::traits::StatisticValue;
        let mut detailed = HashMap::new();
        
        if let Ok(stats) = self.stats.try_read() {
            detailed.insert("total_broadcasts".to_string(), StatisticValue::Integer(stats.total_broadcasts as i64));
            detailed.insert("active_broadcasts".to_string(), StatisticValue::Integer(stats.active_broadcasts as i64));
            detailed.insert("concurrent_viewers".to_string(), StatisticValue::Integer(stats.concurrent_viewers as i64));
            detailed.insert("total_viewers".to_string(), StatisticValue::Integer(stats.total_viewers as i64));
            detailed.insert("total_chat_messages".to_string(), StatisticValue::Integer(stats.total_chat_messages as i64));
            detailed.insert("memory_usage".to_string(), StatisticValue::Integer(stats.memory_usage as i64));
            detailed.insert("error_count".to_string(), StatisticValue::Integer(stats.error_count as i64));
            detailed.insert("uptime_seconds".to_string(), StatisticValue::Float(stats.uptime.as_secs_f64()));
        }
        
        if let Ok(broadcasts) = self.broadcasts.try_read() {
            detailed.insert("broadcast_count".to_string(), StatisticValue::Integer(broadcasts.len() as i64));
        }
        
        if let Ok(viewers) = self.viewers.try_read() {
            detailed.insert("viewer_count".to_string(), StatisticValue::Integer(viewers.len() as i64));
        }
        
        if let Ok(messages) = self.chat_messages.try_read() {
            detailed.insert("chat_message_count".to_string(), StatisticValue::Integer(messages.len() as i64));
        }
        
        detailed
    }
    
    async fn get_resource_usage(&self) -> Option<crate::core::traits::ResourceUsage> {
        use crate::core::traits::ResourceUsage;
        
        let memory_usage = if let Ok(stats) = self.stats.try_read() {
            stats.memory_usage as f64
        } else {
            0.0
        };
        
        Some(ResourceUsage {
            cpu_percent: 0.0, // Would need system monitoring
            memory_bytes: memory_usage as u64,
            network_bytes_in: 0, // Would need network monitoring
            network_bytes_out: 0, // Would need network monitoring
            disk_bytes_read: 0, // Would need disk monitoring
            disk_bytes_written: 0, // Would need disk monitoring
        })
    }
    
    async fn get_health_status(&self) -> crate::core::traits::HealthStatus {
        use crate::core::traits::HealthStatus;
        
        match self.state.try_read() {
            Ok(state) => {
                match *state {
                    LiveState::Ready | LiveState::Broadcasting => {
                        if let Ok(stats) = self.stats.try_read() {
                            if stats.error_count > 10 {
                                HealthStatus::Warning("High error count detected".to_string())
                            } else {
                                HealthStatus::Healthy
                            }
                        } else {
                            HealthStatus::Critical("Unable to read statistics".to_string())
                        }
                    },
                    LiveState::Paused => HealthStatus::Warning("Engine is paused".to_string()),
                    LiveState::Error(_) | LiveState::Shutdown => HealthStatus::Critical("Engine is in error or shutdown state".to_string()),
                    LiveState::Uninitialized | LiveState::Initializing => HealthStatus::Warning("Engine is not ready".to_string()),
                }
            },
            Err(_) => HealthStatus::Critical("Unable to read engine state".to_string())
        }
    }
    
    async fn get_statistics_range(&self, _start: Instant, _end: Instant) -> Self::Stats {
        // For LiveEngine, we don't store historical statistics
        // In a real implementation, this would query a time-series database
        self.get_statistics().await
    }
}

#[async_trait]
impl Configurable for LiveEngine {
    type Config = LiveConfig;
    
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
    
    async fn validate_config(_config: &Self::Config) -> Result<()> {
        Ok(())
    }
    
    async fn apply_config_changes(&mut self, changes: std::collections::HashMap<String, String>) -> Result<()> {
        let mut current_config = self.config.write().await;
        
        for (key, value) in changes {
            match key.as_str() {
                "buffer_duration_seconds" => {
                    if let Ok(val) = value.parse::<f64>() {
                        current_config.buffer_duration_seconds = val;
                    }
                },
                "max_latency_ms" => {
                    if let Ok(val) = value.parse::<u64>() {
                        current_config.max_latency_ms = val;
                    }
                },
                "low_latency_mode" => {
                    if let Ok(val) = value.parse::<bool>() {
                        current_config.low_latency_mode = val;
                    }
                },
                "retry_attempts" => {
                    if let Ok(val) = value.parse::<u32>() {
                        current_config.retry_attempts = val;
                    }
                },
                "retry_delay_ms" => {
                    if let Ok(val) = value.parse::<u64>() {
                        current_config.retry_delay_ms = val;
                    }
                },

                _ => {
                    warn!("Unknown config key: {}", key);
                }
            }
        }
        
        Ok(())
    }
    
    async fn get_config_schema(&self) -> std::collections::HashMap<String, String> {
        let mut schema = std::collections::HashMap::new();
        schema.insert("max_concurrent_broadcasts".to_string(), "integer (1-1000): Maximum number of concurrent broadcasts".to_string());
        schema.insert("max_viewers_per_broadcast".to_string(), "integer (1-100000): Maximum viewers per broadcast".to_string());
        schema.insert("chat_history_limit".to_string(), "integer (100-10000): Maximum chat messages to keep in history".to_string());
        schema.insert("enable_chat".to_string(), "boolean: Enable chat functionality".to_string());
        schema.insert("enable_recording".to_string(), "boolean: Enable broadcast recording".to_string());
        schema
    }
    
    async fn has_config_changed(&self) -> bool {
        // For LiveEngine, we don't track config changes
        // In a real implementation, this would compare with a baseline
        false
    }
    
    async fn reload_config(&mut self) -> Result<()> {
        // For LiveEngine, config is in-memory
        // In a real implementation, this would reload from file/database
        info!("Config reload requested for LiveEngine (no-op)");
        Ok(())
    }
}

#[async_trait]
impl Monitorable for LiveEngine {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        match self.state.try_read() {
            Ok(state) => {
                match *state {
                    LiveState::Uninitialized => EngineHealth::Warning("Engine not initialized".to_string()),
                    LiveState::Initializing => EngineHealth::Warning("Engine initializing".to_string()),
                    LiveState::Ready => {
                        if let Ok(stats) = self.stats.try_read() {
                            if stats.active_broadcasts > 100 {
                                EngineHealth::Warning(format!("High broadcast load: {} active broadcasts", stats.active_broadcasts))
                            } else if stats.error_count > 10 {
                                EngineHealth::Warning(format!("High error count: {}", stats.error_count))
                            } else {
                                EngineHealth::Healthy
                            }
                        } else {
                            EngineHealth::Warning("Cannot read statistics".to_string())
                        }
                    },
                    LiveState::Broadcasting => {
                        if let Ok(stats) = self.stats.try_read() {
                            if stats.concurrent_viewers > 10000 {
                                EngineHealth::Warning(format!("High viewer load: {} concurrent viewers", stats.concurrent_viewers))
                            } else {
                                EngineHealth::Healthy
                            }
                        } else {
                            EngineHealth::Warning("Cannot read statistics during broadcast".to_string())
                        }
                    },
                    LiveState::Paused => EngineHealth::Warning("Engine paused".to_string()),
                    LiveState::Error(ref msg) => EngineHealth::Error(format!("Engine error: {}", msg)),
                    LiveState::Shutdown => EngineHealth::Error("Engine shutdown".to_string()),
                }
            },
            Err(_) => EngineHealth::Error("Cannot read engine state".to_string())
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        Ok(self.get_health().await)
    }
    
    async fn get_detailed_health(&self) -> crate::core::traits::HealthStatus {
        let health = self.get_health().await;
        
        // Convert EngineHealth to HealthStatus
        match health {
            EngineHealth::Healthy => crate::core::traits::HealthStatus::Healthy,
            EngineHealth::Warning(msg) => crate::core::traits::HealthStatus::Warning(msg),
            EngineHealth::Error(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Critical(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Failed(msg) => crate::core::traits::HealthStatus::Critical(msg),
            EngineHealth::Unknown => crate::core::traits::HealthStatus::Unknown,
        }
    }
    
    async fn get_performance_metrics(&self) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();
        
        if let Ok(stats) = self.stats.try_read() {
            metrics.insert("broadcasts_per_second".to_string(), 
                stats.total_broadcasts as f64 / stats.uptime.as_secs_f64().max(1.0));
            metrics.insert("viewers_per_broadcast".to_string(), 
                if stats.active_broadcasts > 0 {
                    stats.concurrent_viewers as f64 / stats.active_broadcasts as f64
                } else {
                    0.0
                });
            metrics.insert("messages_per_second".to_string(), 
                stats.total_chat_messages as f64 / stats.uptime.as_secs_f64().max(1.0));
            metrics.insert("error_rate".to_string(), 
                stats.error_count as f64 / stats.total_broadcasts.max(1) as f64);
            metrics.insert("memory_per_viewer".to_string(), 
                if stats.concurrent_viewers > 0 {
                    stats.memory_usage as f64 / stats.concurrent_viewers as f64
                } else {
                    0.0
                });
        }
        
        metrics
    }
    
    async fn get_alerts(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        
        if let Ok(stats) = self.stats.try_read() {
            if stats.error_count > 10 {
                alerts.push(format!("High error count: {}", stats.error_count));
            }
            
            if stats.concurrent_viewers > 10000 {
                alerts.push(format!("High viewer load: {} concurrent viewers", stats.concurrent_viewers));
            }
            
            if stats.active_broadcasts > 100 {
                alerts.push(format!("High broadcast load: {} active broadcasts", stats.active_broadcasts));
            }
            
            if stats.memory_usage > 1_000_000_000 { // 1GB
                alerts.push(format!("High memory usage: {} bytes", stats.memory_usage));
            }
        }
        
        if let Ok(state) = self.state.try_read() {
            if matches!(*state, LiveState::Error(_)) {
                alerts.push("Engine is in error state".to_string());
            }
        }
        
        alerts
    }
    
    async fn set_monitoring_thresholds(&mut self, thresholds: HashMap<String, f64>) -> Result<()> {
        // For LiveEngine, we would store thresholds in config
        // This is a simplified implementation
        info!("Setting monitoring thresholds: {:?}", thresholds);
        Ok(())
    }
    
    async fn get_uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    async fn get_last_error(&self) -> Option<String> {
        if let Ok(state) = self.state.try_read() {
            if let LiveState::Error(ref msg) = *state {
                Some(msg.clone())
            } else {
                None
            }
        } else {
            Some("Cannot read engine state".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LiveConfig;
    
    #[tokio::test]
    async fn test_live_engine_creation() {
        let config = LiveConfig::default();
        let engine = LiveEngine::new(&config);
        assert!(engine.is_ok());
    }
    
    #[tokio::test]
    async fn test_live_engine_lifecycle() {
        let config = LiveConfig::default();
        let engine = LiveEngine::new(&config).unwrap();
        
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
    async fn test_broadcast_creation() {
        let config = LiveConfig::default();
        let engine = LiveEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let params = BroadcastParams {
            title: "Test Broadcast".to_string(),
            description: Some("Test Description".to_string()),
            broadcaster_id: Id::generate(),
            broadcaster_name: "TestUser".to_string(),
            category: "Gaming".to_string(),
            tags: vec!["test".to_string()],
            format: MediaFormat::Mp4,
            quality: MediaQuality::High,
            resolution: (1920, 1080),
            fps: 30.0,
            bitrate: 1000000,
            is_private: false,
            is_mature: false,
        };
        let broadcast_id = engine.create_broadcast(params).await;
        
        assert!(broadcast_id.is_ok());
        
        let broadcast_id = broadcast_id.unwrap();
        let broadcast = engine.get_broadcast(&broadcast_id).await;
        assert!(broadcast.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_broadcast_lifecycle() {
        let config = LiveConfig::default();
        let engine = LiveEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let params = BroadcastParams {
            title: "Test Broadcast".to_string(),
            description: None,
            broadcaster_id: Id::generate(),
            broadcaster_name: "TestUser".to_string(),
            category: "Gaming".to_string(),
            tags: vec![],
            format: MediaFormat::Mp4,
            quality: MediaQuality::Medium,
            resolution: (1280, 720),
            fps: 25.0,
            bitrate: 500000,
            is_private: false,
            is_mature: false,
        };
        let broadcast_id = engine.create_broadcast(params).await.unwrap();
        
        // Start broadcast
        let result = engine.start_broadcast(&broadcast_id).await;
        assert!(result.is_ok());
        
        // End broadcast
        let result = engine.end_broadcast(&broadcast_id).await;
        assert!(result.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_viewer_management() {
        let config = LiveConfig::default();
        let engine = LiveEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let params = BroadcastParams {
            title: "Test Broadcast".to_string(),
            description: None,
            broadcaster_id: Id::generate(),
            broadcaster_name: "TestUser".to_string(),
            category: "Gaming".to_string(),
            tags: vec![],
            format: MediaFormat::Mp4,
            quality: MediaQuality::Medium,
            resolution: (1280, 720),
            fps: 25.0,
            bitrate: 500000,
            is_private: false,
            is_mature: false,
        };
        let broadcast_id = engine.create_broadcast(params).await.unwrap();
        
        engine.start_broadcast(&broadcast_id).await.unwrap();
        
        // Add viewer
        let viewer_id = engine.add_viewer(
            &broadcast_id,
            Some("TestViewer".to_string()),
            Some("Test Viewer".to_string()),
            None,
        ).await;
        assert!(viewer_id.is_ok());
        
        let viewer_id = viewer_id.unwrap();
        
        // Remove viewer
        let result = engine.remove_viewer(&viewer_id).await;
        assert!(result.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_live_statistics() {
        let config = LiveConfig::default();
        let mut engine = LiveEngine::new(&config).unwrap();
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_broadcasts, 0);
        assert_eq!(stats.active_broadcasts, 0);
        assert_eq!(stats.concurrent_viewers, 0);
        
        engine.reset_statistics().await.unwrap();
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_broadcasts, 0);
    }
}