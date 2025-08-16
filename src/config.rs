//! Configuration management for OpenAce Rust
//!
//! Provides centralized configuration handling with environment variable support,
//! file-based configuration, and runtime updates.

use crate::error::{OpenAceError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration structure for OpenAce
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Path to configuration file for hot-reload
    pub config_path: Option<PathBuf>,
    /// Core engine configuration
    pub core: CoreConfig,
    /// Main engine configuration
    pub main: MainConfig,
    /// Segmenter configuration
    pub segmenter: SegmenterConfig,
    /// Streamer configuration
    pub streamer: StreamerConfig,
    /// Transport/P2P configuration
    pub transport: TransportConfig,
    /// Live streaming configuration
    pub live: LiveConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Performance tuning
    pub performance: PerformanceConfig,
}

/// Core engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    /// Maximum number of concurrent operations
    pub max_concurrent_operations: usize,
    /// Default timeout for operations
    pub default_timeout_ms: u64,
    /// Enable debug mode
    pub debug_mode: bool,
    /// Memory pool size in MB
    pub memory_pool_size_mb: usize,
}

/// Main engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    /// Whether the main engine is enabled
    pub enabled: bool,
    /// Maximum number of concurrent content streams
    pub max_concurrent_streams: usize,
    /// Content cache size in MB
    pub content_cache_size_mb: usize,
    /// Content discovery timeout in seconds
    pub discovery_timeout_seconds: u64,
    /// Enable content verification
    pub enable_verification: bool,
    /// Maximum content size in MB
    pub max_content_size_mb: usize,
}

/// Segmenter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmenterConfig {
    /// Enable segmenter engine
    pub enabled: bool,
    /// Segment duration in seconds
    pub segment_duration_seconds: f64,
    /// Maximum segment size in bytes
    pub max_segment_size_bytes: usize,
    /// Output directory for segments
    pub output_directory: PathBuf,
    /// Video codec settings
    pub video_codec: VideoCodecConfig,
    /// Audio codec settings
    pub audio_codec: AudioCodecConfig,
    /// Enable hardware acceleration
    pub hardware_acceleration: bool,
}

/// Video codec configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoCodecConfig {
    /// Codec name (e.g., "h264", "h265")
    pub codec: String,
    /// Bitrate in kbps
    pub bitrate_kbps: u32,
    /// Frame rate
    pub framerate: f64,
    /// Resolution width
    pub width: u32,
    /// Resolution height
    pub height: u32,
    /// Quality preset
    pub preset: String,
}

/// Audio codec configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCodecConfig {
    /// Codec name (e.g., "aac", "mp3")
    pub codec: String,
    /// Bitrate in kbps
    pub bitrate_kbps: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u32,
}

/// Streamer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamerConfig {
    /// Enable streamer engine
    pub enabled: bool,
    /// Server port
    pub port: u16,
    /// Maximum concurrent clients
    pub max_clients: usize,
    /// Maximum concurrent streams
    pub max_concurrent_streams: usize,
    /// Buffer size for streaming
    pub buffer_size_bytes: usize,
    /// Chunk size for transmission
    pub chunk_size_bytes: usize,
    /// Enable adaptive bitrate
    pub adaptive_bitrate: bool,
    /// Enable hardware acceleration
    pub hardware_acceleration: bool,
    /// Quality levels for adaptive streaming
    pub quality_levels: Vec<QualityLevel>,
}

/// Quality level for adaptive streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityLevel {
    /// Quality name (e.g., "720p", "1080p")
    pub name: String,
    /// Video bitrate in kbps
    pub video_bitrate_kbps: u32,
    /// Audio bitrate in kbps
    pub audio_bitrate_kbps: u32,
    /// Resolution width
    pub width: u32,
    /// Resolution height
    pub height: u32,
}

/// Transport/P2P configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Enable transport engine
    pub enabled: bool,
    /// Listen port for P2P connections
    pub listen_port: u16,
    /// Maximum number of peers
    pub max_peers: usize,
    /// Connection timeout in seconds
    pub connection_timeout_seconds: u64,
    /// Enable DHT (Distributed Hash Table)
    pub enable_dht: bool,
    /// Bootstrap nodes for P2P network
    pub bootstrap_nodes: Vec<String>,
    /// Upload rate limit in bytes/second (0 = unlimited)
    pub upload_rate_limit_bps: u64,
    /// Download rate limit in bytes/second (0 = unlimited)
    pub download_rate_limit_bps: u64,
}

/// Live streaming configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveConfig {
    /// Enable live engine
    pub enabled: bool,
    /// Buffer duration in seconds
    pub buffer_duration_seconds: f64,
    /// Maximum latency in milliseconds
    pub max_latency_ms: u64,
    /// Enable low-latency mode
    pub low_latency_mode: bool,
    /// Retry attempts for failed streams
    pub retry_attempts: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    /// Log output file (None = stdout)
    pub output_file: Option<PathBuf>,
    /// Enable JSON formatting
    pub json_format: bool,
    /// Maximum log file size in MB
    pub max_file_size_mb: usize,
    /// Number of log files to keep
    pub max_files: usize,
    /// Enable performance tracing
    pub enable_tracing: bool,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Number of worker threads (0 = auto)
    pub worker_threads: usize,
    /// Enable thread pinning
    pub thread_pinning: bool,
    /// Memory allocation strategy
    pub memory_strategy: MemoryStrategy,
    /// Enable SIMD optimizations
    pub enable_simd: bool,
    /// Cache size in MB
    pub cache_size_mb: usize,
}

/// Memory allocation strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryStrategy {
    /// Standard system allocator
    System,
    /// Memory pool allocation
    Pool,
    /// NUMA-aware allocation
    Numa,
}



impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            max_concurrent_operations: 100,
            default_timeout_ms: 30000,
            debug_mode: false,
            memory_pool_size_mb: 256,
        }
    }
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concurrent_streams: 10,
            content_cache_size_mb: 512,
            discovery_timeout_seconds: 30,
            enable_verification: true,
            max_content_size_mb: 4096,
        }
    }
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            segment_duration_seconds: 6.0,
            max_segment_size_bytes: 10 * 1024 * 1024, // 10MB
            output_directory: PathBuf::from("./segments"),
            video_codec: VideoCodecConfig::default(),
            audio_codec: AudioCodecConfig::default(),
            hardware_acceleration: true,
        }
    }
}

impl Default for VideoCodecConfig {
    fn default() -> Self {
        Self {
            codec: "h264".to_string(),
            bitrate_kbps: 2500,
            framerate: 30.0,
            width: 1920,
            height: 1080,
            preset: "medium".to_string(),
        }
    }
}

impl Default for AudioCodecConfig {
    fn default() -> Self {
        Self {
            codec: "aac".to_string(),
            bitrate_kbps: 128,
            sample_rate: 48000,
            channels: 2,
        }
    }
}

impl Default for StreamerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8080,
            max_clients: 1000,
            max_concurrent_streams: 50,
            buffer_size_bytes: 1024 * 1024, // 1MB
            chunk_size_bytes: 64 * 1024,    // 64KB
            adaptive_bitrate: true,
            hardware_acceleration: false,
            quality_levels: vec![
                QualityLevel {
                    name: "480p".to_string(),
                    video_bitrate_kbps: 1000,
                    audio_bitrate_kbps: 96,
                    width: 854,
                    height: 480,
                },
                QualityLevel {
                    name: "720p".to_string(),
                    video_bitrate_kbps: 2500,
                    audio_bitrate_kbps: 128,
                    width: 1280,
                    height: 720,
                },
                QualityLevel {
                    name: "1080p".to_string(),
                    video_bitrate_kbps: 5000,
                    audio_bitrate_kbps: 192,
                    width: 1920,
                    height: 1080,
                },
            ],
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_port: 6881,
            max_peers: 100,
            connection_timeout_seconds: 30,
            enable_dht: true,
            bootstrap_nodes: vec![
                "/ip4/127.0.0.1/tcp/8081".to_string(),
            ],
            upload_rate_limit_bps: 0,   // unlimited
            download_rate_limit_bps: 0, // unlimited
        }
    }
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_duration_seconds: 30.0,
            max_latency_ms: 3000,
            low_latency_mode: false,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            output_file: None,
            json_format: false,
            max_file_size_mb: 100,
            max_files: 5,
            enable_tracing: false,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: 0, // auto-detect
            thread_pinning: false,
            memory_strategy: MemoryStrategy::System,
            enable_simd: true,
            cache_size_mb: 128,
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let settings = config::Config::builder()
            .add_source(config::File::with_name(path.to_str().unwrap()))
            .add_source(config::Environment::with_prefix("OPENACE"))
            .build()
            .map_err(OpenAceError::from)?;
            
        let mut config: Self = settings.try_deserialize().map_err(OpenAceError::from)?;
        config.config_path = Some(path);
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(OpenAceError::from)?;
            
        std::fs::write(path, content)
            .map_err(OpenAceError::from)
    }
    
    /// Get timeout as Duration
    pub fn default_timeout(&self) -> Duration {
        Duration::from_millis(self.core.default_timeout_ms)
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate core config
        if self.core.max_concurrent_operations == 0 {
            return Err(OpenAceError::configuration(
                "max_concurrent_operations must be greater than 0"
            ));
        }
        
        // Validate segmenter config
        if self.segmenter.segment_duration_seconds <= 0.0 {
            return Err(OpenAceError::configuration(
                "segment_duration_seconds must be greater than 0"
            ));
        }
        
        // Validate transport config
        if self.transport.listen_port == 0 {
            return Err(OpenAceError::configuration(
                "listen_port must be greater than 0"
            ));
        }
        
        // Validate logging level
        match self.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {},
            _ => return Err(OpenAceError::configuration(
                "Invalid log level. Must be one of: trace, debug, info, warn, error"
            )),
        }
        
        Ok(())
    }
}

/// Type alias for backward compatibility
pub type OpenAceConfig = Config;