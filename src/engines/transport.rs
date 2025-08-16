//! Transport Engine for OpenAce Rust
//!
//! This module implements the transport engine that handles network communication,
//! data transmission, and protocol management with modern async patterns.

use crate::error::{Result, OpenAceError};
use crate::config::TransportConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use crate::core::traits::*;
use crate::core::types::*;
use crate::utils::threading::{SafeMutex, SafeRwLock};
use crate::engines::EngineHealth;

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::{SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::net::{TcpStream, UdpSocket};
use tokio_rustls::TlsConnector;
use rustls::{ClientConfig, RootCertStore};

use tracing::{debug, info, warn, error, trace, instrument};
use serde::{Serialize, Deserialize};
use bytes::Bytes;

/// Transport engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransportState {
    Uninitialized,
    Initializing,
    Ready,
    Active,
    Paused,
    Error(String),
    Shutdown,
}

/// Connection information
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: Id,
    pub remote_addr: SocketAddr,
    pub local_addr: SocketAddr,
    pub protocol: NetworkProtocol,
    pub status: ConnectionStatus,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub latency: Option<Duration>,
    pub bandwidth: Option<u64>,
    pub error_count: u32,
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnecting,
    Disconnected,
    Error(String),
}

/// Transport message
#[derive(Debug, Clone)]
pub struct TransportMessage {
    pub id: Id,
    pub connection_id: Id,
    pub data: Bytes,
    pub message_type: MessageType,
    pub priority: Priority,
    pub timestamp: Instant,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout: Duration,
}

/// Message type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageType {
    Data,
    Control,
    Heartbeat,
    Acknowledgment,
    Error,
}

/// Transport statistics
#[derive(Debug, Clone)]
pub struct TransportStats {
    pub total_connections: u64,
    pub active_connections: u64,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub average_latency: Duration,
    pub peak_bandwidth: u64,
    pub error_count: u64,
    pub retry_count: u64,
    pub timeout_count: u64,
    pub uptime: Duration,
    pub memory_usage: u64,
}

impl Default for TransportStats {
    fn default() -> Self {
        Self {
            total_connections: 0,
            active_connections: 0,
            total_messages_sent: 0,
            total_messages_received: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            average_latency: Duration::ZERO,
            peak_bandwidth: 0,
            error_count: 0,
            retry_count: 0,
            timeout_count: 0,
            uptime: Duration::ZERO,
            memory_usage: 0,
        }
    }
}

/// Protocol handler trait
#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    async fn handle_connection(&self, connection: &mut Connection) -> Result<()>;
    async fn send_message(&self, connection: &Connection, message: &TransportMessage) -> Result<()>;
    async fn receive_message(&self, connection: &Connection) -> Result<Option<TransportMessage>>;
    async fn close_connection(&self, connection: &mut Connection) -> Result<()>;
}

/// TCP protocol handler
#[derive(Debug)]
pub struct TcpHandler {
    streams: SafeRwLock<HashMap<Id, Arc<Mutex<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>>>>,
}

impl Default for TcpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpHandler {
    pub fn new() -> Self {
        Self {
            streams: SafeRwLock::new(HashMap::new(), "tcp_handler_streams"),
        }
    }
}

#[async_trait]
impl ProtocolHandler for TcpHandler {
    async fn handle_connection(&self, connection: &mut Connection) -> Result<()> {
        info!("Handling TCP connection: {}", connection.id);
        let tcp_stream = TcpStream::connect(connection.remote_addr).await?;
let config = ClientConfig::builder()
    .with_root_certificates(RootCertStore::empty())
    .with_no_client_auth();
let connector = TlsConnector::from(Arc::new(config));
let domain_str = connection.remote_addr.ip().to_string(); // Use IP as domain for simplicity, adjust if needed
let domain = rustls::pki_types::ServerName::try_from(domain_str)
    .map_err(|_| OpenAceError::invalid_argument("Invalid domain"))?
    .to_owned();
let stream = connector.connect(domain, tcp_stream).await?;

        self.streams.write().await.insert(connection.id.clone(), Arc::new(Mutex::new(stream)));
        connection.status = ConnectionStatus::Connected;
        Ok(())
    }
    
    async fn send_message(&self, connection: &Connection, message: &TransportMessage) -> Result<()> {
        debug!("Sending TCP message: {} bytes to {}", message.data.len(), connection.id);
        if let Some(stream) = self.streams.read().await.get(&connection.id) {
            let mut stream = stream.lock().await;
stream.write_all(&message.data).await?;
            Ok(())
        } else {
            Err(OpenAceError::not_found("TCP stream not found"))
        }
    }
    
    async fn receive_message(&self, connection: &Connection) -> Result<Option<TransportMessage>> {
        debug!("Receiving TCP message from: {}", connection.id);
        if let Some(stream) = self.streams.read().await.get(&connection.id) {
            let mut stream = stream.lock().await;
let mut buf = crate::utils::memory::SafeBuffer::new(1024)?;
let n = stream.read(buf.as_mut_slice()).await?;
            if n == 0 { return Ok(None); }
            Ok(Some(TransportMessage {
                id: Id::generate(),
                connection_id: connection.id.clone(),
                data: Bytes::copy_from_slice(&buf.as_slice()[0..n]),
                message_type: MessageType::Data,
                priority: Priority::Normal,
                timestamp: Instant::now(),
                retry_count: 0,
                max_retries: 3,
                timeout: Duration::from_secs(30),
            }))
        } else {
            Err(OpenAceError::not_found("TCP stream not found"))
        }
    }
    
    async fn close_connection(&self, connection: &mut Connection) -> Result<()> {
        info!("Closing TCP connection: {}", connection.id);
        if let Some(stream) = self.streams.write().await.remove(&connection.id) {
            let mut stream = stream.lock().await;
stream.shutdown().await?;
        }
        connection.status = ConnectionStatus::Disconnected;
        Ok(())
    }
}

/// UDP protocol handler
#[derive(Debug)]
pub struct UdpHandler {
    sockets: SafeRwLock<HashMap<Id, Arc<Mutex<UdpSocket>>>>,
}

impl Default for UdpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpHandler {
    pub fn new() -> Self {
        Self {
            sockets: SafeRwLock::new(HashMap::new(), "udp_handler_sockets"),
        }
    }
}

#[async_trait]
impl ProtocolHandler for UdpHandler {
    async fn handle_connection(&self, connection: &mut Connection) -> Result<()> {
        info!("Handling UDP connection: {}", connection.id);
        let socket = UdpSocket::bind(connection.local_addr).await?;
        socket.connect(connection.remote_addr).await?;
        self.sockets.write().await.insert(connection.id.clone(), Arc::new(Mutex::new(socket)));
        connection.status = ConnectionStatus::Connected;
        Ok(())
    }
    
    async fn send_message(&self, connection: &Connection, message: &TransportMessage) -> Result<()> {
        debug!("Sending UDP message: {} bytes to {}", message.data.len(), connection.id);
        if let Some(socket) = self.sockets.read().await.get(&connection.id) {
            let socket = socket.lock().await;
            socket.send(&message.data).await?;
            Ok(())
        } else {
            Err(OpenAceError::not_found("UDP socket not found"))
        }
    }
    
    async fn receive_message(&self, connection: &Connection) -> Result<Option<TransportMessage>> {
        debug!("Receiving UDP message from: {}", connection.id);
        if let Some(socket) = self.sockets.read().await.get(&connection.id) {
            let socket = socket.lock().await;
            let mut buf = crate::utils::memory::SafeBuffer::new(1024)?;
            let n = socket.recv(buf.as_mut_slice()).await?;
            if n == 0 { return Ok(None); }
            Ok(Some(TransportMessage {
                id: Id::generate(),
                connection_id: connection.id.clone(),
                data: Bytes::copy_from_slice(&buf.as_slice()[0..n]),
                message_type: MessageType::Data,
                priority: Priority::Normal,
                timestamp: Instant::now(),
                retry_count: 0,
                max_retries: 3,
                timeout: Duration::from_secs(30),
            }))
        } else {
            Err(OpenAceError::not_found("UDP socket not found"))
        }
    }
    
    async fn close_connection(&self, connection: &mut Connection) -> Result<()> {
        info!("Closing UDP connection: {}", connection.id);
        self.sockets.write().await.remove(&connection.id);
        connection.status = ConnectionStatus::Disconnected;
        Ok(())
    }
}

/// Transport engine implementation
#[derive(Debug)]
pub struct TransportEngine {
    config: SafeRwLock<TransportConfig>,
    state: SafeRwLock<TransportState>,
    stats: SafeRwLock<TransportStats>,
    connections: SafeRwLock<HashMap<Id, Connection>>,
    #[allow(dead_code)]
    message_queue: SafeMutex<Vec<TransportMessage>>,
    tcp_handler: Arc<TcpHandler>,
    udp_handler: Arc<UdpHandler>,
    message_sender: SafeMutex<Option<mpsc::UnboundedSender<TransportMessage>>>,
    shutdown_sender: SafeMutex<Option<oneshot::Sender<()>>>,
    start_time: Instant,
}

impl TransportEngine {
    /// Create a new transport engine
    pub fn new(config: &TransportConfig) -> Result<Self> {
        info!("Creating transport engine");
        
        Ok(Self {
            config: SafeRwLock::new(config.clone(), "transport_config"),
            state: SafeRwLock::new(TransportState::Uninitialized, "transport_state"),
            stats: SafeRwLock::new(TransportStats::default(), "transport_stats"),
            connections: SafeRwLock::new(HashMap::new(), "transport_connections"),
            message_queue: SafeMutex::new(Vec::new(), "transport_message_queue"),
            tcp_handler: Arc::new(TcpHandler::new()),
            udp_handler: Arc::new(UdpHandler::new()),
            message_sender: SafeMutex::new(None, "transport_message_sender"),
            shutdown_sender: SafeMutex::new(None, "transport_shutdown_sender"),
            start_time: Instant::now(),
        })
    }
    
    /// Initialize the transport engine
    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing transport engine");
        debug!("Starting transport engine initialization sequence");
        
        // Set state to initializing
        debug!("Setting transport state to Initializing");
        {
            let mut state = self.state.write().await;
            let old_state = state.clone();
            debug!("Current transport engine state: {:?}", *state);
            if *state != TransportState::Uninitialized {
                error!("Invalid state transition from {:?} to Initializing", *state);
                return Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Initializing"
                ));
            }
            *state = TransportState::Initializing;
            trace!("Transport state transition: {:?} -> {:?}", old_state, *state);
            debug!("Transport engine state changed to: {:?}", *state);
        }
        
        // Initialize message processing
        debug!("Setting up message processing channel");
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        debug!("Message processing channels created successfully");
        trace!("Message sender channel established");
        
        // Store senders
        debug!("Storing message and shutdown senders");
        *self.message_sender.lock().await = Some(message_tx);
        *self.shutdown_sender.lock().await = Some(shutdown_tx);
        debug!("Senders stored successfully");
        
        // Start message processing task
        debug!("Starting background message processing task");
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            debug!("Message processing task started");
            engine_ref.message_processing_loop(message_rx, shutdown_rx).await;
            debug!("Message processing task ended");
        });
        
        // Start connection monitoring task
        debug!("Starting connection monitoring task");
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            debug!("Connection monitoring task started");
            engine_ref.connection_monitoring_loop().await;
            debug!("Connection monitoring task ended");
        });
        
        // Set state to ready
        debug!("Setting transport state to Ready");
        {
            let mut state = self.state.write().await;
            let old_state = state.clone();
            *state = TransportState::Ready;
            trace!("Transport state transition: {:?} -> {:?}", old_state, *state);
            debug!("Transport engine state updated to: Ready");
        }
        
        info!("Transport engine initialized successfully");
        debug!("Transport engine initialization completed - ready for connections");
        Ok(())
    }
    
    /// Shutdown the transport engine
    pub async fn shutdown(&self) -> Result<()> {
        debug!("Starting transport engine shutdown");
        trace!("Shutting down transport engine");
        info!("Shutting down transport engine");
        
        // Update state
        debug!("Updating transport engine state to Shutdown");
        let previous_state = {
            let mut state = self.state.write().await;
            let prev = state.clone();
            *state = TransportState::Shutdown;
            prev
        };
        debug!("Transport engine state changed from {:?} to Shutdown", previous_state);
        
        // Send shutdown signal
        debug!("Sending shutdown signal to background tasks");
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            match sender.send(()) {
                Ok(_) => debug!("Shutdown signal sent successfully"),
                Err(_) => warn!("Failed to send shutdown signal - receiver may have been dropped"),
            }
        } else {
            debug!("No shutdown sender available - tasks may not have been started");
        }
        
        // Close all connections
        debug!("Closing all transport connections");
        self.close_all_connections().await?;
        debug!("All transport connections closed");
        
        info!("Transport engine shut down successfully");
        debug!("Transport engine shutdown completed");
        Ok(())
    }
    
    /// Pause the transport engine
    pub async fn pause(&self) -> Result<()> {
        debug!("Starting transport engine pause");
        trace!("Pausing transport engine");
        info!("Pausing transport engine");
        
        let mut state = self.state.write().await;
        debug!("Current transport engine state: {:?}", *state);
        match *state {
            TransportState::Ready | TransportState::Active => {
                let previous_state = state.clone();
                *state = TransportState::Paused;
                debug!("Transport engine state changed from {:?} to Paused", previous_state);
                info!("Transport engine paused successfully");
                Ok(())
            }
            _ => {
                error!("Invalid state transition from {:?} to Paused", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Paused"
                ))
            }
        }
    }
    
    /// Resume the transport engine
    pub async fn resume(&self) -> Result<()> {
        debug!("Starting transport engine resume");
        trace!("Resuming transport engine");
        info!("Resuming transport engine");
        
        let mut state = self.state.write().await;
        debug!("Current transport engine state: {:?}", *state);
        match *state {
            TransportState::Paused => {
                *state = TransportState::Ready;
                debug!("Transport engine state changed from Paused to Ready");
                info!("Transport engine resumed successfully");
                Ok(())
            }
            _ => {
                error!("Invalid state transition from {:?} to Ready", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Ready"
                ))
            }
        }
    }
    
    /// Create a new connection
    #[instrument(skip(self))]
    pub async fn create_connection(
        &self,
        remote_addr: SocketAddr,
        local_addr: SocketAddr,
        protocol: NetworkProtocol,
    ) -> Result<Id> {
        debug!("Creating connection from {} to {} using {:?}", local_addr, remote_addr, protocol);
        
        // Generate unique connection ID
        let connection_id = Id::new(format!("conn_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        
        // Create new connection
        let connection = Connection {
            id: connection_id.clone(),
            remote_addr,
            local_addr,
            protocol,
            status: ConnectionStatus::Connecting,
            created_at: Instant::now(),
            last_activity: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
            latency: None,
            bandwidth: None,
            error_count: 0,
        };
        
        // Store connection
        self.connections.write().await.insert(connection_id.clone(), connection);
        
        // Get mutable reference to stored connection
        let mut conn = self.connections.write().await.get_mut(&connection_id).unwrap().clone();
        
        // Use appropriate protocol handler
        let handler: Arc<dyn ProtocolHandler> = match protocol {
            NetworkProtocol::Tcp => self.tcp_handler.clone(),
            NetworkProtocol::Udp => self.udp_handler.clone(),
            _ => return Err(OpenAceError::unsupported_protocol(protocol)),
        };
        
        // Initialize connection with handler
        handler.handle_connection(&mut conn).await?;
        
        // Update connection status to connected
        conn.status = ConnectionStatus::Connected;
        
        // Update stored connection
        *self.connections.write().await.get_mut(&connection_id).unwrap() = conn;
        
        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_connections += 1;
        stats.active_connections += 1;
        
        debug!("Connection {} created successfully", connection_id);
        Ok(connection_id)
    }
    
    /// Close a connection
    pub async fn close_connection(&self, connection_id: &Id) -> Result<()> {
        debug!("Starting connection closure for {}", connection_id);
        trace!("Closing connection {}", connection_id);
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(connection_id) {
            debug!("Found connection {} with status {:?} and protocol {:?}", connection_id, connection.status, connection.protocol);
            // Use appropriate protocol handler
            let result = match connection.protocol {
                NetworkProtocol::Tcp => {
                    debug!("Using TCP handler to close connection {}", connection_id);
                    self.tcp_handler.close_connection(connection).await
                }
                NetworkProtocol::Udp => {
                    debug!("Using UDP handler to close connection {}", connection_id);
                    self.udp_handler.close_connection(connection).await
                }
                _ => {
                    debug!("Using default handler to close connection {}", connection_id);
                    let prev_status = connection.status.clone();
                    connection.status = ConnectionStatus::Disconnected;
                    debug!("Connection {} status changed from {:?} to {:?}", connection_id, prev_status, connection.status);
                    Ok(())
                }
            };
            
            if result.is_ok() {
                debug!("Connection {} closed successfully, updating statistics", connection_id);
                // Update stats
                let mut stats = self.stats.write().await;
                let prev_active = stats.active_connections;
                stats.active_connections = stats.active_connections.saturating_sub(1);
                debug!("Statistics updated: active_connections {} -> {}", prev_active, stats.active_connections);
                
                // Update engine state if no more active connections
                if stats.active_connections == 0 {
                    debug!("No more active connections, updating engine state to Ready");
                    drop(stats);
                    drop(connections);
                    *self.state.write().await = TransportState::Ready;
                    debug!("Transport engine state updated to Ready");
                }
            } else {
                error!("Failed to close connection {}: {:?}", connection_id, result);
            }
            
            info!("Connection closed: {}", connection_id);
            debug!("Connection closure completed for {}", connection_id);
            trace!("Connection {} removed from connections map", connection_id);
            result
        } else {
            warn!("Connection {} not found for closure", connection_id);
            debug!("Available connections: {:?}", connections.keys().collect::<Vec<_>>());
            trace!("Connection closure failed: connection {} does not exist", connection_id);
            Err(OpenAceError::not_found(format!("Connection not found: {}", connection_id)))
        }
    }
    
    /// Close all connections
    async fn close_all_connections(&self) -> Result<()> {
        debug!("Starting closure of all connections");
        trace!("Closing all connections");
        let mut connections = self.connections.write().await;
        let total_connections = connections.len();
        debug!("Found {} total connections to process", total_connections);
        let mut closed_count = 0;
        
        for (id, connection) in connections.iter_mut() {
            debug!("Processing connection {} with status {:?}", id, connection.status);
            if connection.status == ConnectionStatus::Connected {
                debug!("Closing connected connection {} using {:?} protocol", id, connection.protocol);
                let result = match connection.protocol {
                    NetworkProtocol::Tcp => {
                        debug!("Using TCP handler to close connection {}", id);
                        self.tcp_handler.close_connection(connection).await
                    }
                    NetworkProtocol::Udp => {
                        debug!("Using UDP handler to close connection {}", id);
                        self.udp_handler.close_connection(connection).await
                    }
                    _ => {
                        debug!("Using default handler to close connection {}", id);
                        let prev_status = connection.status.clone();
                        connection.status = ConnectionStatus::Disconnected;
                        debug!("Connection {} status changed from {:?} to {:?}", id, prev_status, connection.status);
                        Ok(())
                    }
                };
                
                if result.is_ok() {
                    closed_count += 1;
                    debug!("Successfully closed connection {}", id);
                } else {
                    error!("Failed to close connection {}: {:?}", id, result);
                }
            } else {
                debug!("Skipping connection {} with status {:?}", id, connection.status);
            }
        }
        
        // Update stats
        if closed_count > 0 {
            debug!("Updating transport statistics for {} closed connections", closed_count);
            let mut stats = self.stats.write().await;
            let prev_active = stats.active_connections;
            stats.active_connections = 0;
            debug!("Statistics updated: active_connections {} -> 0", prev_active);
        } else {
            debug!("No connections were closed, statistics unchanged");
        }
        
        info!("Closed {} connections", closed_count);
        debug!("All connections closure completed: {}/{} connections processed", closed_count, total_connections);
        Ok(())
    }
    
    /// Send a message
    #[instrument(skip(self, data))]
    pub async fn send_message(
        &self,
        connection_id: &Id,
        data: Bytes,
        message_type: MessageType,
        priority: Priority,
    ) -> Result<Id> {
        debug!("Starting message send to connection {}", connection_id);
        trace!("Sending message to connection {}", connection_id);
        debug!("Message details: type={:?}, priority={:?}, size={} bytes", message_type, priority, data.len());
        
        // Check if connection exists and is connected
        debug!("Validating connection {} for message sending", connection_id);
        {
            let connections = self.connections.read().await;
            if let Some(connection) = connections.get(connection_id) {
                debug!("Found connection {} with status {:?}", connection_id, connection.status);
                if connection.status != ConnectionStatus::Connected {
                    error!("Invalid connection status for message sending: {:?}", connection.status);
                    return Err(OpenAceError::invalid_state_transition(
                        format!("{:?}", connection.status),
                        "Connected"
                    ));
                }
            } else {
                error!("Connection {} not found for message sending", connection_id);
                return Err(OpenAceError::not_found(
                    format!("Connection not found: {}", connection_id)
                ));
            }
        }
        
        let message_id = Id::generate();
        debug!("Generated message ID: {}", message_id);
        let message = TransportMessage {
            id: message_id.clone(),
            connection_id: connection_id.clone(),
            data: data.clone(),
            message_type: message_type.clone(),
            priority,
            timestamp: Instant::now(),
            retry_count: 0,
            max_retries: 3,
            timeout: Duration::from_secs(30),
        };
        debug!("Created transport message: id={}, connection_id={}, type={:?}, priority={:?}", 
               message.id, message.connection_id, message.message_type, message.priority);
        
        // Queue message for processing
        debug!("Queueing message for processing");
        if let Some(sender) = self.message_sender.lock().await.as_ref() {
            match sender.send(message) {
                Ok(_) => debug!("Message {} queued successfully", message_id),
                Err(_) => {
                    error!("Failed to queue message {} for sending", message_id);
                    return Err(OpenAceError::engine_operation(
                        "transport",
                        "queue_message",
                        "Failed to queue message for sending",
                    ));
                }
            }
        } else {
            error!("Message sender not available - transport engine not initialized");
            return Err(OpenAceError::invalid_state_transition(
                "Uninitialized",
                "Initialized with message sender"
            ));
        }
        
        // Update statistics
        debug!("Updating transport statistics for sent message");
        {
            let mut stats = self.stats.write().await;
            let prev_sent = stats.total_messages_sent;
            let prev_bytes = stats.total_bytes_sent;
            stats.total_messages_sent += 1;
            stats.total_bytes_sent += data.len() as u64;
            debug!("Statistics updated: total_messages_sent {} -> {}, total_bytes_sent {} -> {}", 
                   prev_sent, stats.total_messages_sent, prev_bytes, stats.total_bytes_sent);
        }
        
        info!("Message sent successfully: id={}, connection={}, type={:?}", message_id, connection_id, message_type);
        debug!("Message send operation completed: message_id={}", message_id);
        trace!("Message send completed: id={}, size={} bytes, priority={:?}", message_id, data.len(), priority);
        Ok(message_id)
    }
    
    /// Get connection information
    pub async fn get_connection(&self, connection_id: &Id) -> Result<Connection> {
        let connections = self.connections.read().await;
        connections.get(connection_id)
            .cloned()
            .ok_or_else(|| OpenAceError::not_found(format!("Connection not found: {}", connection_id)))
    }
    
    /// Get all connections
    pub async fn get_all_connections(&self) -> Vec<Connection> {
        self.connections.read().await.values().cloned().collect()
    }
    
    /// Get active connections
    pub async fn get_active_connections(&self) -> Vec<Connection> {
        self.connections.read().await
            .values()
            .filter(|conn| conn.status == ConnectionStatus::Connected)
            .cloned()
            .collect()
    }
    
    /// Update configuration
    pub async fn update_config(&self, config: &TransportConfig) -> Result<()> {
        trace!("Updating transport configuration");
        if let Ok(mut current_config) = self.config.try_write() {
            *current_config = config.clone();
            Ok(())
        } else {
            Err(OpenAceError::thread_safety("Failed to acquire config lock"))
        }
    }
    

    
    /// Clone for task (simplified clone for async tasks)
    fn clone_for_task(&self) -> TransportEngineTask {
        TransportEngineTask {
            config: Arc::clone(&self.config.inner()),
            state: Arc::clone(&self.state.inner()),
            stats: Arc::clone(&self.stats.inner()),
            connections: Arc::clone(&self.connections.inner()),
            tcp_handler: Arc::clone(&self.tcp_handler),
            udp_handler: Arc::clone(&self.udp_handler),
            start_time: self.start_time,
        }
    }
}

/// Simplified engine reference for async tasks
#[derive(Debug, Clone)]
struct TransportEngineTask {
    #[allow(dead_code)]
    config: Arc<tokio::sync::RwLock<TransportConfig>>,
    #[allow(dead_code)]
    state: Arc<tokio::sync::RwLock<TransportState>>,
    #[allow(dead_code)]
    stats: Arc<tokio::sync::RwLock<TransportStats>>,
    connections: Arc<tokio::sync::RwLock<HashMap<Id, Connection>>>,
    #[allow(dead_code)]
    tcp_handler: Arc<TcpHandler>,
    #[allow(dead_code)]
    udp_handler: Arc<UdpHandler>,
    #[allow(dead_code)]
    start_time: Instant,
}

impl TransportEngineTask {
    async fn message_processing_loop(
        &self,
        mut message_rx: mpsc::UnboundedReceiver<TransportMessage>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                message = message_rx.recv() => {
                    if let Some(mut msg) = message {
                        debug!("Processing message: {:?}", msg.id);
                        let conn = { self.connections.read().await.get(&msg.connection_id).cloned() };
                        if let Some(connection) = conn {
                            let handler: Arc<dyn ProtocolHandler> = match connection.protocol {
                                NetworkProtocol::Tcp => self.tcp_handler.clone() as Arc<dyn ProtocolHandler>,
                                NetworkProtocol::Udp => self.udp_handler.clone() as Arc<dyn ProtocolHandler>,
                                _ => continue,
                            };
                            if handler.send_message(&connection, &msg).await.is_ok() {
                                // Update stats
                            } else if msg.retry_count < msg.max_retries {
                                msg.retry_count += 1;
                                // Requeue
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ = &mut shutdown_rx => { break; }
            }
        }
    }
    
    async fn connection_monitoring_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let now = Instant::now();
            let to_close: Vec<Id> = { self.connections.read().await.iter().filter_map(|(id, conn)| {
                if now - conn.last_activity > Duration::from_secs(300) { Some(id.clone()) } else { None }
            }).collect() };
            for id in to_close {
                // Call close_connection, but since it's task, perhaps log or something; ideally full engine needed.
                // For now, set status
                if let Some(mut conn) = { self.connections.write().await.get_mut(&id).cloned() } {
                    conn.status = ConnectionStatus::Disconnected;
                }
            }
        }
    }
}

#[async_trait]
impl Monitorable for TransportEngine {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        // Since this is synchronous, we can't use .await
        // We'll need to use try_read() or provide a default
        let state = match self.state.try_read() {
            Ok(state) => state.clone(),
            Err(_) => return EngineHealth::Unknown,
        };
        
        let stats = match self.stats.try_read() {
            Ok(stats) => stats.clone(),
            Err(_) => return EngineHealth::Unknown,
        };
        
        match state {
            TransportState::Ready | TransportState::Active => {
                if stats.error_count > 100 {
                    EngineHealth::Error(format!("High error count: {}", stats.error_count))
                } else if stats.error_count > 10 {
                    EngineHealth::Warning(format!("Elevated error count: {}", stats.error_count))
                } else {
                    EngineHealth::Healthy
                }
            }
            TransportState::Error(ref msg) => {
                EngineHealth::Error(msg.clone())
            }
            TransportState::Paused => {
                EngineHealth::Warning("Transport engine is paused".to_string())
            }
            TransportState::Uninitialized | TransportState::Initializing => {
                EngineHealth::Warning("Transport engine not ready".to_string())
            }
            TransportState::Shutdown => {
                EngineHealth::Error("Transport engine is shutdown".to_string())
            }
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        let state = self.state.read().await;
        let stats = self.stats.read().await;
        
        let health = match *state {
            TransportState::Ready | TransportState::Active => {
                if stats.error_count > 100 {
                    EngineHealth::Error(format!("High error count: {}", stats.error_count))
                } else if stats.error_count > 10 {
                    EngineHealth::Warning(format!("Elevated error count: {}", stats.error_count))
                } else {
                    EngineHealth::Healthy
                }
            }
            TransportState::Error(ref msg) => {
                EngineHealth::Error(msg.clone())
            }
            TransportState::Paused => {
                EngineHealth::Warning("Transport engine is paused".to_string())
            }
            TransportState::Uninitialized | TransportState::Initializing => {
                EngineHealth::Warning("Transport engine not ready".to_string())
            }
            TransportState::Shutdown => {
                EngineHealth::Error("Transport engine is shutdown".to_string())
            }
        };
        
        Ok(health)
    }
}

#[async_trait]
impl Configurable for TransportEngine {
    type Config = TransportConfig;
    
    async fn get_config(&self) -> Self::Config {
        self.config.try_read().map(|c| c.clone()).unwrap_or_default()
    }
    
    async fn update_config(&mut self, config: &Self::Config) -> Result<()> {
        let mut current_config = self.config.write().await;
        *current_config = config.clone();
        Ok(())
    }
    
    async fn validate_config(config: &Self::Config) -> Result<()> {
        if config.max_peers == 0 {
            return Err(OpenAceError::configuration(
                "max_peers must be greater than 0"
            ));
        }
        
        if config.listen_port == 0 {
            return Err(OpenAceError::configuration(
                "listen_port must be greater than 0"
            ));
        }
        
        if config.connection_timeout_seconds == 0 {
            return Err(OpenAceError::configuration(
                "connection_timeout_seconds must be greater than 0"
            ));
        }
        
        Ok(())
    }
}

#[async_trait]
impl StatisticsProvider for TransportEngine {
    type Stats = TransportStats;
    
    async fn get_statistics(&self) -> Self::Stats {
        // Use try_read for sync method
        match self.stats.try_read() {
            Ok(stats) => stats.clone(),
            Err(_) => TransportStats::default(),
        }
    }
    
    async fn reset_statistics(&mut self) -> Result<()> {
        if let Ok(mut stats) = self.stats.try_write() {
            *stats = TransportStats::default();
        }
        Ok(())
    }
}

#[async_trait]
impl Lifecycle for TransportEngine {
    async fn initialize(&mut self) -> Result<()> {
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        // Use try_read for non-blocking access to state
        if let Ok(state) = self.state.try_read() {
            !matches!(*state, TransportState::Uninitialized)
        } else {
            // If we can't read the state, assume not initialized
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TransportConfig;
    use std::net::{IpAddr, Ipv4Addr};
    
    #[tokio::test]
    async fn test_transport_engine_creation() {
        let config = TransportConfig::default();
        let engine = TransportEngine::new(&config);
        assert!(engine.is_ok());
    }
    
    #[tokio::test]
    async fn test_transport_engine_lifecycle() {
        let config = TransportConfig::default();
        let engine = TransportEngine::new(&config).unwrap();
        
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
    async fn test_connection_creation() {
        let config = TransportConfig::default();
        let engine = TransportEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
        
        // Test UDP connection instead of TCP to avoid "Connection refused" errors
        let connection_id = engine.create_connection(
            remote_addr,
            local_addr,
            NetworkProtocol::Udp,
        ).await;
        
        assert!(connection_id.is_ok());
        
        let connection_id = connection_id.unwrap();
        let connection = engine.get_connection(&connection_id).await;
        assert!(connection.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_message_sending() {
        let config = TransportConfig::default();
        let engine = TransportEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        
        // Test UDP connection instead of TCP to avoid "Connection refused" errors
        let connection_id = engine.create_connection(
            remote_addr,
            local_addr,
            NetworkProtocol::Udp,
        ).await.unwrap();
        
        // Manually set connection as connected for testing
        {
            let mut connections = engine.connections.write().await;
            if let Some(connection) = connections.get_mut(&connection_id) {
                connection.status = ConnectionStatus::Connected;
            }
        }
        
        let data = Bytes::from("test message");
        let message_id = engine.send_message(
            &connection_id,
            data,
            MessageType::Data,
            Priority::Normal,
        ).await;
        
        assert!(message_id.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_transport_statistics() {
        let config = TransportConfig::default();
        let mut engine = TransportEngine::new(&config).unwrap();
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.total_messages_sent, 0);
        
        engine.reset_statistics().await.unwrap();
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_connections, 0);
    }
}