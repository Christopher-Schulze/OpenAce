//! OpenAce Engine Binary Entry Point
//!
//! This is the main entry point for the OpenAce Engine binary.
//! The binary name will be "OpenAce-Engine" and will include OS and platform information
//! when built for different targets.

use openace_rust::{
    config::Config,
    engines::manager::EngineManager,
    error::OpenAceError,
};
use std::env;
use std::process;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), OpenAceError> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Print binary information
    print_binary_info();
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let config_path = args.get(1).map(|s| s.as_str()).unwrap_or("config.toml");
    
    info!("Starting OpenAce Engine with config: {}", config_path);
    
    // Load configuration
    let config = match Config::from_file(config_path) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            process::exit(1);
        }
    };

    // Initialize engine manager
    let engine_manager = match EngineManager::with_config(config) {
        Ok(manager) => manager,
        Err(e) => {
            error!("Failed to create engine manager: {}", e);
            process::exit(1);
        }
    };

    // Initialize all engines
    if let Err(e) = engine_manager.initialize().await {
        error!("Failed to initialize engines: {}", e);
        process::exit(1);
    }

    // Start all engines
    if let Err(e) = engine_manager.start().await {
        error!("Failed to start engines: {}", e);
        process::exit(1);
    }
    
    info!("OpenAce Engine started successfully");
    
    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal, stopping engines...");
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }
    
    // Shutdown all engines
    if let Err(e) = engine_manager.shutdown().await {
        error!("Error during shutdown: {}", e);
        process::exit(1);
    }
    
    info!("OpenAce Engine shutdown complete");
    Ok(())
}

// Include build-time information
include!(concat!(env!("OUT_DIR"), "/build_info.rs"));

fn print_binary_info() {
    let version = env!("CARGO_PKG_VERSION");
    
    println!("========================================");
    println!("OpenAce Engine v{}", version);
    println!("Binary: {}", build_info::BINARY_NAME);
    println!("Platform: {}", build_info::PLATFORM_SUFFIX);
    println!("Target: {}", build_info::TARGET);
    println!("OS: {}", build_info::TARGET_OS);
    println!("Architecture: {}", build_info::TARGET_ARCH);
    println!("Environment: {}", build_info::TARGET_ENV);
    println!("Build Time: {}", build_info::BUILD_TIMESTAMP);
    println!("========================================");
}