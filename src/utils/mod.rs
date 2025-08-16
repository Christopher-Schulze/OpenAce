//! Utility modules for OpenAce Rust
//!
//! Provides memory-safe alternatives to the C utility functions,
//! with modern Rust patterns and zero-cost abstractions.

pub mod memory;
pub mod logging;
pub mod context;
pub mod string;
pub mod strings;
pub mod threading;
pub mod time;

use crate::error::Result;

/// Re-export commonly used utilities
pub use memory::*;
pub use logging::*;
pub use context::*;
pub use threading::*;

/// Initialize all utility subsystems
pub async fn initialize_utils() -> Result<()> {
    tracing::info!("Initializing OpenAce utilities");
    
    // Initialize logging first
    logging::initialize_logging().await?;
    
    // Initialize memory management
    memory::initialize_memory_management().await?;
    
    // Initialize threading utilities
    threading::initialize_threading().await?;
    
    tracing::info!("OpenAce utilities initialized successfully");
    Ok(())
}

/// Shutdown all utility subsystems
pub async fn shutdown_utils() -> Result<()> {
    tracing::info!("Shutting down OpenAce utilities");
    
    // Shutdown in reverse order
    threading::shutdown_threading().await?;
    memory::shutdown_memory_management().await?;
    logging::shutdown_logging().await?;
    
    tracing::info!("OpenAce utilities shut down successfully");
    Ok(())
}