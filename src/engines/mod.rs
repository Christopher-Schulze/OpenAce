//! Engine modules for OpenAce Rust
//!
//! This module contains all the engine implementations that handle
//! different aspects of the OpenAce system.

use crate::error::Result;
use crate::config::OpenAceConfig;
use crate::core::traits::Monitorable;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use serde::{Serialize, Deserialize};

// Engine modules
pub mod core;
pub mod segmenter;
pub mod streamer;
pub mod transport;
pub mod live;
pub mod main_engine;
pub mod manager;

// Re-export engine types
pub use core::CoreEngine;
pub use segmenter::SegmenterEngine;
pub use streamer::StreamerEngine;
pub use transport::TransportEngine;
pub use live::LiveEngine;
pub use main_engine::MainEngine;

// Re-export the main EngineManager


/// Engine manager that coordinates all engines
#[derive(Debug)]
pub struct EngineManager {
    segmenter: Arc<RwLock<SegmenterEngine>>,
    streamer: Arc<RwLock<StreamerEngine>>,
    transport: Arc<RwLock<TransportEngine>>,
    live: Arc<RwLock<LiveEngine>>,
    main: Arc<RwLock<MainEngine>>,
    initialized: Arc<RwLock<bool>>,
}

impl EngineManager {
    /// Create a new engine manager
    pub fn new(config: &OpenAceConfig) -> Result<Self> {
        info!("Creating engine manager");
        
        let segmenter = Arc::new(RwLock::new(SegmenterEngine::new(&config.segmenter)?));
        let streamer = Arc::new(RwLock::new(StreamerEngine::new(&config.streamer)?));
        let transport = Arc::new(RwLock::new(TransportEngine::new(&config.transport)?));
        let live = Arc::new(RwLock::new(LiveEngine::new(&config.live)?));
        let main = Arc::new(RwLock::new(MainEngine::new(&config.main)?));
        
        Ok(Self {
            segmenter,
            streamer,
            transport,
            live,
            main,
            initialized: Arc::new(RwLock::new(false)),
        })
    }
    
    /// Initialize all engines
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing all engines");
        
        // Initialize engines in dependency order
        self.transport.write().await.initialize().await?;
        self.segmenter.write().await.initialize().await?;
        self.streamer.write().await.initialize().await?;
        self.live.write().await.initialize().await?;
        self.main.write().await.initialize().await?;
        
        *self.initialized.write().await = true;
        info!("All engines initialized successfully");
        Ok(())
    }
    
    /// Shutdown all engines
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down all engines");
        
        // Shutdown engines in reverse dependency order
        if let Err(e) = self.main.write().await.shutdown().await {
            error!("Failed to shutdown main engine: {}", e);
        }
        
        if let Err(e) = self.live.write().await.shutdown().await {
            error!("Failed to shutdown live engine: {}", e);
        }
        
        if let Err(e) = self.streamer.write().await.shutdown().await {
            error!("Failed to shutdown streamer engine: {}", e);
        }
        
        if let Err(e) = self.segmenter.write().await.shutdown().await {
            error!("Failed to shutdown segmenter engine: {}", e);
        }
        
        if let Err(e) = self.transport.write().await.shutdown().await {
            error!("Failed to shutdown transport engine: {}", e);
        }
        
        *self.initialized.write().await = false;
        info!("All engines shut down");
        Ok(())
    }
    
    /// Check if all engines are initialized
    pub async fn is_initialized(&self) -> bool {
        *self.initialized.read().await
    }
    
    /// Get segmenter engine reference
    pub fn segmenter(&self) -> Arc<RwLock<SegmenterEngine>> {
        Arc::clone(&self.segmenter)
    }
    
    /// Get streamer engine reference
    pub fn streamer(&self) -> Arc<RwLock<StreamerEngine>> {
        Arc::clone(&self.streamer)
    }
    
    /// Get transport engine reference
    pub fn transport(&self) -> Arc<RwLock<TransportEngine>> {
        Arc::clone(&self.transport)
    }
    
    /// Get live engine reference
    pub fn live(&self) -> Arc<RwLock<LiveEngine>> {
        Arc::clone(&self.live)
    }
    
    /// Get main engine reference
    pub fn main(&self) -> Arc<RwLock<MainEngine>> {
        Arc::clone(&self.main)
    }
    
    /// Pause all engines
    pub async fn pause_all(&self) -> Result<()> {
        info!("Pausing all engines");
        
        self.main.write().await.pause().await?;
        self.live.write().await.pause().await?;
        self.streamer.write().await.pause().await?;
        self.segmenter.write().await.pause().await?;
        
        info!("All engines paused");
        Ok(())
    }
    
    /// Resume all engines
    pub async fn resume_all(&self) -> Result<()> {
        info!("Resuming all engines");
        
        self.segmenter.write().await.resume().await?;
        self.streamer.write().await.resume().await?;
        self.live.write().await.resume().await?;
        self.main.write().await.resume().await?;
        
        info!("All engines resumed");
        Ok(())
    }
    
    /// Get health status of all engines
    pub async fn get_health_status(&self) -> EngineHealthStatus {
        let segmenter_health = self.segmenter.read().await.get_health().await;
        let streamer_health = self.streamer.read().await.get_health().await;
        let transport_health = self.transport.read().await.get_health().await;
        let live_health = self.live.read().await.get_health().await;
        let main_health = self.main.read().await.get_health().await;
        
        EngineHealthStatus {
            segmenter: segmenter_health,
            streamer: streamer_health,
            transport: transport_health,
            live: live_health,
            main: main_health,
        }
    }
    
    /// Update configuration for all engines
    pub async fn update_config(&self, config: &OpenAceConfig) -> Result<()> {
        info!("Updating configuration for all engines");
        
        self.segmenter.write().await.update_config(&config.segmenter).await?;
        self.streamer.write().await.update_config(&config.streamer).await?;
        self.transport.write().await.update_config(&config.transport).await?;
        self.live.write().await.update_config(&config.live).await?;
        self.main.write().await.update_config(&config.main).await?;
        
        info!("Configuration updated for all engines");
        Ok(())
    }
}

/// Health status for all engines
#[derive(Debug, Clone)]
pub struct EngineHealthStatus {
    pub segmenter: EngineHealth,
    pub streamer: EngineHealth,
    pub transport: EngineHealth,
    pub live: EngineHealth,
    pub main: EngineHealth,
}

impl EngineHealthStatus {
    /// Check if all engines are healthy
    pub fn is_healthy(&self) -> bool {
        self.segmenter.is_healthy() &&
        self.streamer.is_healthy() &&
        self.transport.is_healthy() &&
        self.live.is_healthy() &&
        self.main.is_healthy()
    }
    
    /// Get unhealthy engines
    pub fn get_unhealthy_engines(&self) -> Vec<String> {
        let mut unhealthy = Vec::new();
        
        if !self.segmenter.is_healthy() {
            unhealthy.push("segmenter".to_string());
        }
        if !self.streamer.is_healthy() {
            unhealthy.push("streamer".to_string());
        }
        if !self.transport.is_healthy() {
            unhealthy.push("transport".to_string());
        }
        if !self.live.is_healthy() {
            unhealthy.push("live".to_string());
        }
        if !self.main.is_healthy() {
            unhealthy.push("main".to_string());
        }
        
        unhealthy
    }
}

/// Engine health status
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EngineHealth {
    Healthy,
    Warning(String),
    Error(String),
    Critical(String),
    Failed(String),
    Unknown,
}

impl EngineHealth {
    /// Check if engine is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, EngineHealth::Healthy)
    }
    
    /// Get health message
    pub fn message(&self) -> Option<&str> {
        match self {
            EngineHealth::Warning(msg) | EngineHealth::Error(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Initialize all engines
pub async fn initialize_engines(config: &OpenAceConfig) -> Result<EngineManager> {
    info!("Initializing engine subsystem");
    
    let manager = EngineManager::new(config)?;
    manager.initialize().await?;
    
    info!("Engine subsystem initialized successfully");
    Ok(manager)
}

/// Shutdown all engines
pub async fn shutdown_engines(manager: EngineManager) -> Result<()> {
    info!("Shutting down engine subsystem");
    
    manager.shutdown().await?;
    
    info!("Engine subsystem shut down successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenAceConfig;
    
    #[tokio::test]
    async fn test_engine_manager_creation() {
        let config = OpenAceConfig::default();
        let manager = EngineManager::new(&config);
        assert!(manager.is_ok());
    }
    
    #[tokio::test]
    async fn test_engine_manager_lifecycle() {
        let config = OpenAceConfig::default();
        let manager = EngineManager::new(&config).unwrap();
        
        assert!(!manager.is_initialized().await);
        
        // Initialize
        let result = manager.initialize().await;
        assert!(result.is_ok());
        assert!(manager.is_initialized().await);
        
        // Shutdown
        let result = manager.shutdown().await;
        assert!(result.is_ok());
        assert!(!manager.is_initialized().await);
    }
    
    #[tokio::test]
    async fn test_engine_health_status() {
        let healthy_status = EngineHealthStatus {
            segmenter: EngineHealth::Healthy,
            streamer: EngineHealth::Healthy,
            transport: EngineHealth::Healthy,
            live: EngineHealth::Healthy,
            main: EngineHealth::Healthy,
        };
        
        assert!(healthy_status.is_healthy());
        assert!(healthy_status.get_unhealthy_engines().is_empty());
        
        let unhealthy_status = EngineHealthStatus {
            segmenter: EngineHealth::Error("Test error".to_string()),
            streamer: EngineHealth::Healthy,
            transport: EngineHealth::Warning("Test warning".to_string()),
            live: EngineHealth::Healthy,
            main: EngineHealth::Healthy,
        };
        
        assert!(!unhealthy_status.is_healthy());
        let unhealthy = unhealthy_status.get_unhealthy_engines();
        assert_eq!(unhealthy.len(), 2);
        assert!(unhealthy.contains(&"segmenter".to_string()));
        assert!(unhealthy.contains(&"transport".to_string()));
    }
    
    #[test]
    fn test_engine_health() {
        let healthy = EngineHealth::Healthy;
        assert!(healthy.is_healthy());
        assert!(healthy.message().is_none());
        
        let warning = EngineHealth::Warning("Test warning".to_string());
        assert!(!warning.is_healthy());
        assert_eq!(warning.message(), Some("Test warning"));
        
        let error = EngineHealth::Error("Test error".to_string());
        assert!(!error.is_healthy());
        assert_eq!(error.message(), Some("Test error"));
        
        let unknown = EngineHealth::Unknown;
        assert!(!unknown.is_healthy());
        assert!(unknown.message().is_none());
    }
}