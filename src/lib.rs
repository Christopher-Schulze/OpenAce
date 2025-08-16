//! OpenAce Rust - A modern, memory-safe streaming engine
//!
//! This is a complete rewrite of the OpenAce streaming engine in Rust,
//! providing memory safety, better concurrency, and modern architecture.

pub mod core;
pub mod engines;
pub mod utils;
pub mod error;
pub mod error_recovery;
pub mod state_management;
pub mod resource_management;
pub mod config;
pub mod thread_safety;

// Entferne #[cfg(feature = "python")] pub mod python;
// Entferne #[cfg(feature = "c_api")] pub mod c_api;

use error::Result;
use std::sync::Arc;
use crate::config::Config;

// Use EngineManager as the central orchestrator
use engines::manager::EngineManager;

/// Global OpenAce instance - singleton pattern
static GLOBAL_MANAGER: once_cell::sync::OnceCell<Arc<EngineManager>> = once_cell::sync::OnceCell::new();

/// Get the global EngineManager
pub fn get_global_manager() -> Option<Arc<EngineManager>> {
    GLOBAL_MANAGER.get().cloned()
}

/// Initialize the global EngineManager
pub async fn initialize_global_manager(config: Config) -> Result<Arc<EngineManager>> {
    let manager = Arc::new(EngineManager::with_config(config)?);
    manager.initialize().await?;
    
    GLOBAL_MANAGER.set(manager.clone())
        .map_err(|_| error::OpenAceError::AlreadyInitialized {
            context: Box::new(error::ErrorContext::new("global_manager", "initialization")),
            recovery_strategy: Box::new(error::RecoveryStrategy::Fallback {
                alternative: "Use get_global_manager() to access existing instance".to_string(),
            }),
        })?;
    
    Ok(manager)
}

/// Shutdown the global EngineManager
pub async fn shutdown_global_manager() -> Result<()> {
    if let Some(manager) = GLOBAL_MANAGER.get() {
        manager.shutdown().await?;
    }
    Ok(())
}