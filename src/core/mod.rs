//! Core module for OpenAce Rust
//!
//! This module contains the core infrastructure and base types used throughout
//! the OpenAce system.

pub mod engine;
pub mod types;
pub mod traits;
pub mod string;
pub mod logging;

// Re-export specific items from submodules to avoid ambiguous glob re-exports
pub use engine::{EngineState, EngineStats, CoreEngine, Engine, CoreEngineFactory};
pub use types::{
    Id, Version, Priority, Status, MediaFormat, MediaQuality, NetworkProtocol,
    DataBuffer, ResourceInfo, ConnectionInfo, TaskInfo, OperationResult
};
pub use traits::{
    InitializationStatus, HealthStatus, Lifecycle, Pausable, StatisticsProvider,
    Configurable, Monitorable, EventHandler, DataProcessor, DataStreamer,
    ResourceManager, Cache, RateLimiter, RetryPolicy, ConnectionManager,
    TaskScheduler, Plugin, Observer, Observable, Command, StateMachine, Middleware
};
pub use string::StringUtils;
pub use logging::{LogLevel, LogEntry, LogStatistics, LogHealthStatus, LogHealthLevel, LogAggregator};

use crate::error::Result;
use crate::config::CoreConfig;
use tracing::{info, debug};

/// Core module initialization
pub async fn initialize_core(config: CoreConfig) -> Result<()> {
    info!("Initializing core module with config: {:?}", config);
    
    // Initialize core components
    debug!("Core module initialization complete");
    Ok(())
}

/// Core module shutdown
pub async fn shutdown_core() -> Result<()> {
    info!("Shutting down core module");
    
    // Cleanup core components
    debug!("Core module shutdown complete");
    Ok(())
}