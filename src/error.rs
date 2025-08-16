//! Error handling for OpenAce Rust
//!
//! Provides comprehensive error types and handling for all OpenAce operations.
//! Includes enhanced error context, recovery mechanisms, and detailed error tracking.

use thiserror::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};


/// Main result type for OpenAce operations
pub type Result<T> = std::result::Result<T, OpenAceError>;

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Low severity - informational or minor issues
    Low,
    /// Medium severity - warnings that should be addressed
    Medium,
    /// High severity - errors that affect functionality
    High,
    /// Critical severity - system-threatening errors
    Critical,
}

/// Error context providing additional information about error conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Timestamp when the error occurred
    pub timestamp: u64,
    /// Component or module where the error occurred
    pub component: String,
    /// Operation being performed when error occurred
    pub operation: String,
    /// Error severity level
    pub severity: ErrorSeverity,
    /// Additional context data
    pub metadata: HashMap<String, String>,
    /// Stack trace if available
    pub stack_trace: Option<String>,
    /// Related error IDs for error correlation
    pub related_errors: Vec<String>,
    /// Suggested recovery actions
    pub recovery_suggestions: Vec<String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(component: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            component: component.into(),
            operation: operation.into(),
            severity: ErrorSeverity::Medium,
            metadata: HashMap::new(),
            stack_trace: None,
            related_errors: Vec::new(),
            recovery_suggestions: Vec::new(),
        }
    }
    
    /// Set error severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
    
    /// Add stack trace
    pub fn with_stack_trace(mut self, trace: impl Into<String>) -> Self {
        self.stack_trace = Some(trace.into());
        self
    }
    
    /// Add related error
    pub fn with_related_error(mut self, error_id: impl Into<String>) -> Self {
        self.related_errors.push(error_id.into());
        self
    }
    
    /// Add recovery suggestion
    pub fn with_recovery_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.recovery_suggestions.push(suggestion.into());
        self
    }
}

/// Recovery strategy for handling errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// No recovery possible - fail fast
    None,
    /// Retry the operation with exponential backoff
    Retry { max_attempts: u32, base_delay_ms: u64 },
    /// Fallback to alternative implementation
    Fallback { alternative: String },
    /// Reset component to known good state
    Reset { component: String },
    /// Graceful degradation with reduced functionality
    Degrade { reduced_features: Vec<String> },
    /// Circuit breaker pattern - temporarily disable
    CircuitBreaker { timeout_ms: u64 },
    /// Custom recovery procedure
    Custom { procedure: String, parameters: HashMap<String, String> },
}

/// Error recovery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryResult {
    /// Recovery was successful
    Success { strategy_used: RecoveryStrategy, duration_ms: u64 },
    /// Recovery failed
    Failed { strategy_attempted: RecoveryStrategy, reason: String },
    /// Recovery is in progress
    InProgress { strategy: RecoveryStrategy, elapsed_ms: u64 },
    /// Recovery was skipped
    Skipped { reason: String },
}

/// Comprehensive error types for OpenAce with enhanced context and recovery
#[derive(Error, Debug)]
pub enum OpenAceError {
    /// Memory allocation errors with context
    #[error("Memory allocation failed: {message}")]
    MemoryAllocation {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Thread safety violations with context
    #[error("Thread safety violation: {message}")]
    ThreadSafety {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Context initialization errors with context
    #[error("Context initialization failed: {message}")]
    ContextInitialization {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Engine initialization errors with context
    #[error("Engine initialization failed: {engine} - {message}")]
    EngineInitialization {
        engine: String,
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Engine operation errors with context
    #[error("Engine operation failed: {engine} - {operation} - {message}")]
    EngineOperation {
        engine: String,
        operation: String,
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Configuration errors with context
    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Media processing errors with context
    #[error("Media processing error: {message}")]
    MediaProcessing {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Network/P2P errors with context
    #[error("Network error: {message}")]
    Network {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// File I/O errors with context
    #[error("File I/O error: {message}")]
    FileIO {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Serialization/Deserialization errors with context
    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Context already initialized
    #[error("OpenAce context is already initialized")]
    AlreadyInitialized {
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Context not initialized
    #[error("OpenAce context is not initialized")]
    NotInitialized {
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Invalid state transitions with context
    #[error("Invalid state transition: from {from} to {to}")]
    InvalidStateTransition {
        from: String,
        to: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Resource exhaustion with context
    #[error("Resource exhausted: {resource} - {message}")]
    ResourceExhausted {
        resource: String,
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Timeout errors with context
    #[error("Operation timed out: {operation} after {timeout_ms}ms")]
    Timeout {
        operation: String,
        timeout_ms: u64,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Python integration errors (when feature enabled) with context
    #[cfg(feature = "python")]
    #[error("Python integration error: {message}")]
    Python {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// FFmpeg errors with context
    #[error("FFmpeg error: {message}")]
    FFmpeg {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// libp2p errors with context
    #[error("P2P error: {message}")]
    P2P {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Generic internal errors with context
    #[error("Internal error: {message}")]
    Internal {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
    
    /// Operation cancelled errors with context
    #[error("Operation cancelled: {message}")]
    OperationCancelled {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },

    /// Invalid argument errors with context
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Unsupported protocol errors with context
    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    /// Hardware acceleration errors
    #[error("Hardware acceleration error: {message}")]
    HardwareAcceleration {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },

    /// Streaming protocol errors
    #[error("Streaming protocol error: {message}")]
    StreamingProtocol {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },

    /// Chat system errors
    #[error("Chat system error: {message}")]
    ChatSystem {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },

    /// Metrics collection errors
    #[error("Metrics collection error: {message}")]
    MetricsCollection {
        message: String,
        context: Box<ErrorContext>,
        recovery_strategy: Box<RecoveryStrategy>,
    },
}

impl OpenAceError {
    /// Create a memory allocation error
    pub fn memory_allocation(message: impl Into<String>) -> Self {
        Self::MemoryAllocation {
            message: message.into(),
            context: Box::new(ErrorContext::new("memory", "allocation")),
            recovery_strategy: Box::new(RecoveryStrategy::Retry { max_attempts: 3, base_delay_ms: 100 }),
        }
    }
    
    /// Create a thread safety error
    pub fn thread_safety(message: impl Into<String>) -> Self {
        Self::ThreadSafety {
            message: message.into(),
            context: Box::new(ErrorContext::new("thread", "safety_check")),
            recovery_strategy: Box::new(RecoveryStrategy::Reset { component: "thread_pool".to_string() }),
        }
    }
    
    /// Create a context initialization error
    pub fn context_initialization(message: impl Into<String>) -> Self {
        Self::ContextInitialization {
            message: message.into(),
            context: Box::new(ErrorContext::new("context", "initialization")),
            recovery_strategy: Box::new(RecoveryStrategy::Retry { max_attempts: 2, base_delay_ms: 500 }),
        }
    }
    
    /// Create an engine initialization error
    pub fn engine_initialization(engine: impl Into<String>, message: impl Into<String>) -> Self {
        Self::EngineInitialization {
            engine: engine.into(),
            message: message.into(),
            context: Box::new(ErrorContext::new("engine", "initialization")),
            recovery_strategy: Box::new(RecoveryStrategy::Retry { max_attempts: 3, base_delay_ms: 1000 }),
        }
    }
    
    /// Create an engine operation error
    pub fn engine_operation(
        engine: impl Into<String>,
        operation: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::EngineOperation {
            engine: engine.into(),
            operation: operation.into(),
            message: message.into(),
            context: Box::new(ErrorContext::new("engine", "operation")),
            recovery_strategy: Box::new(RecoveryStrategy::Fallback { alternative: "safe_mode".to_string() }),
        }
    }
    
    /// Create a configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
            context: Box::new(ErrorContext::new("config", "validation")),
            recovery_strategy: Box::new(RecoveryStrategy::Fallback { alternative: "default_config".to_string() }),
        }
    }
    
    /// Create a media processing error
    pub fn media_processing(message: impl Into<String>) -> Self {
        Self::MediaProcessing {
            message: message.into(),
            context: Box::new(ErrorContext::new("media", "processing")),
            recovery_strategy: Box::new(RecoveryStrategy::Degrade { reduced_features: vec!["high_quality".to_string()] }),
        }
    }
    
    /// Create a network error
    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
            context: Box::new(ErrorContext::new("network", "communication")),
            recovery_strategy: Box::new(RecoveryStrategy::Retry { max_attempts: 5, base_delay_ms: 2000 }),
        }
    }
    
    /// Create a file I/O error
    pub fn file_io(message: impl Into<String>) -> Self {
        Self::FileIO {
            message: message.into(),
            context: Box::new(ErrorContext::new("file", "io_operation")),
            recovery_strategy: Box::new(RecoveryStrategy::Retry { max_attempts: 3, base_delay_ms: 500 }),
        }
    }
    
    /// Create a serialization error
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
            context: Box::new(ErrorContext::new("serialization", "data_conversion")),
            recovery_strategy: Box::new(RecoveryStrategy::Fallback { alternative: "json_fallback".to_string() }),
        }
    }
    
    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            context: Box::new(ErrorContext::new("internal", "system_error")),
            recovery_strategy: Box::new(RecoveryStrategy::Reset { component: "system".to_string() }),
        }
    }
    
    /// Create an operation cancelled error
    pub fn operation_cancelled(message: impl Into<String>) -> Self {
        Self::OperationCancelled {
            message: message.into(),
            context: Box::new(ErrorContext::new("operation", "cancellation")),
            recovery_strategy: Box::new(RecoveryStrategy::None),
        }
    }
    
    /// Create an invalid state transition error
    pub fn invalid_state_transition(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::InvalidStateTransition {
            from: from.into(),
            to: to.into(),
            context: Box::new(ErrorContext::new("state", "transition")),
            recovery_strategy: Box::new(RecoveryStrategy::Reset { component: "state_machine".to_string() }),
        }
    }

    /// Create a not found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Internal {
            message: format!("Not found: {}", message.into()),
            context: Box::new(ErrorContext::new("core", "not_found")),
            recovery_strategy: Box::new(RecoveryStrategy::None),
        }
    }

    /// Create an invalid argument error
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }

    /// Create an unsupported protocol error
    pub fn unsupported_protocol(protocol: crate::core::types::NetworkProtocol) -> Self {
        Self::UnsupportedProtocol(format!("{:?}", protocol))
    }

    /// Create a hardware acceleration error
    pub fn hardware_acceleration(message: impl Into<String>) -> Self {
        Self::HardwareAcceleration {
            message: message.into(),
            context: Box::new(ErrorContext::new("hardware", "acceleration")),
            recovery_strategy: Box::new(RecoveryStrategy::Fallback { alternative: "software_fallback".to_string() }),
        }
    }
}

impl OpenAceError {
    /// Create a new error with context and recovery strategy
    pub fn new_with_context(
        error_type: impl Into<String>,
        message: impl Into<String>,
        context: ErrorContext,
        recovery_strategy: RecoveryStrategy,
    ) -> Self {
        match error_type.into().as_str() {
            "memory" => Self::MemoryAllocation {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "thread" => Self::ThreadSafety {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "engine" => Self::EngineInitialization {
                engine: "unknown".to_string(),
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "network" => Self::Network {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "file" => Self::FileIO {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "config" => Self::Configuration {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "resource" => Self::ResourceExhausted {
                resource: "unknown".to_string(),
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "timeout" => Self::Timeout {
                operation: "unknown".to_string(),
                timeout_ms: 0,
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "validation" => Self::Internal {
                message: format!("Validation: {}", message.into()),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "state" => Self::InvalidStateTransition {
                from: "unknown".to_string(),
                to: "unknown".to_string(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "hardware" => Self::HardwareAcceleration {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "streaming" => Self::StreamingProtocol {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "chat" => Self::ChatSystem {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            "metrics" => Self::MetricsCollection {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
            _ => Self::Internal {
                message: message.into(),
                context: Box::new(context),
                recovery_strategy: Box::new(recovery_strategy),
            },
        }
    }

    /// Get the error context
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::MemoryAllocation { context, .. } => context.as_ref(),
            Self::ThreadSafety { context, .. } => context.as_ref(),
            Self::EngineInitialization { context, .. } => context.as_ref(),
            Self::ContextInitialization { context, .. } => context.as_ref(),
            Self::EngineOperation { context, .. } => context.as_ref(),
            Self::Configuration { context, .. } => context.as_ref(),
            Self::MediaProcessing { context, .. } => context.as_ref(),
            Self::Network { context, .. } => context.as_ref(),
            Self::FileIO { context, .. } => context.as_ref(),
            Self::Serialization { context, .. } => context.as_ref(),
            Self::AlreadyInitialized { context, .. } => context.as_ref(),
            Self::NotInitialized { context, .. } => context.as_ref(),
            Self::InvalidStateTransition { context, .. } => context.as_ref(),
            Self::ResourceExhausted { context, .. } => context.as_ref(),
            Self::Timeout { context, .. } => context.as_ref(),
            #[cfg(feature = "python")]
            Self::Python { context, .. } => context.as_ref(),
            Self::FFmpeg { context, .. } => context.as_ref(),
            Self::P2P { context, .. } => context.as_ref(),
            Self::Internal { context, .. } => context.as_ref(),
            Self::OperationCancelled { context, .. } => context.as_ref(),
            Self::HardwareAcceleration { context, .. } => context.as_ref(),
            Self::StreamingProtocol { context, .. } => context.as_ref(),
            Self::ChatSystem { context, .. } => context.as_ref(),
            Self::MetricsCollection { context, .. } => context.as_ref(),
            // For legacy errors without context, create a default one
            _ => {
                static DEFAULT_CONTEXT: std::sync::OnceLock<ErrorContext> = std::sync::OnceLock::new();
                DEFAULT_CONTEXT.get_or_init(|| {
                    ErrorContext::new("unknown", "legacy_error")
                        .with_severity(ErrorSeverity::Medium)
                })
            }
        }
    }

    /// Get the recovery strategy
    pub fn recovery_strategy(&self) -> &RecoveryStrategy {
        match self {
            Self::MemoryAllocation { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::ThreadSafety { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::EngineInitialization { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::ContextInitialization { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::EngineOperation { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::Configuration { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::MediaProcessing { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::Network { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::FileIO { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::Serialization { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::AlreadyInitialized { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::NotInitialized { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::InvalidStateTransition { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::ResourceExhausted { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::Timeout { recovery_strategy, .. } => recovery_strategy.as_ref(),
            #[cfg(feature = "python")]
            Self::Python { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::FFmpeg { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::P2P { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::Internal { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::OperationCancelled { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::HardwareAcceleration { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::StreamingProtocol { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::ChatSystem { recovery_strategy, .. } => recovery_strategy.as_ref(),
            Self::MetricsCollection { recovery_strategy, .. } => recovery_strategy.as_ref(),
            // For legacy errors without recovery strategy, return None
            _ => {
                static DEFAULT_RECOVERY: std::sync::OnceLock<RecoveryStrategy> = std::sync::OnceLock::new();
                DEFAULT_RECOVERY.get_or_init(|| RecoveryStrategy::None)
            },
        }
    }

    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        !matches!(self.recovery_strategy(), RecoveryStrategy::None)
    }

    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        self.context().severity
    }

    /// Check if error is critical
    pub fn is_critical(&self) -> bool {
        matches!(self.severity(), ErrorSeverity::Critical)
    }
}

/// Convert from standard I/O errors
impl From<std::io::Error> for OpenAceError {
    fn from(err: std::io::Error) -> Self {
        let context = ErrorContext::new("io", "file_operation")
            .with_severity(ErrorSeverity::High)
            .with_metadata("error_kind", format!("{:?}", err.kind()))
            .with_recovery_suggestion("Check file permissions and disk space");
        
        let recovery_strategy = match err.kind() {
            std::io::ErrorKind::NotFound => RecoveryStrategy::Fallback {
                alternative: "create_missing_file".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => RecoveryStrategy::Custom {
                procedure: "request_elevated_permissions".to_string(),
                parameters: HashMap::new(),
            },
            std::io::ErrorKind::TimedOut => RecoveryStrategy::Retry {
                max_attempts: 3,
                base_delay_ms: 1000,
            },
            _ => RecoveryStrategy::None,
        };
        
        Self::FileIO {
            message: err.to_string(),
            context: Box::new(context),
            recovery_strategy: Box::new(recovery_strategy),
        }
    }
}

/// Convert from serde JSON errors
impl From<serde_json::Error> for OpenAceError {
    fn from(err: serde_json::Error) -> Self {
        let context = ErrorContext::new("serialization", "json_processing")
            .with_severity(ErrorSeverity::Medium)
            .with_metadata("error_category", format!("{:?}", err.classify()))
            .with_recovery_suggestion("Validate JSON structure and data types");
        
        let recovery_strategy = if err.is_data() {
            RecoveryStrategy::Fallback {
                alternative: "use_default_values".to_string(),
            }
        } else {
            RecoveryStrategy::None
        };
        
        Self::Serialization {
            message: err.to_string(),
            context: Box::new(context),
            recovery_strategy: Box::new(recovery_strategy),
        }
    }
}

/// Convert from config errors
impl From<config::ConfigError> for OpenAceError {
    fn from(err: config::ConfigError) -> Self {
        let context = ErrorContext::new("configuration", "config_loading")
            .with_severity(ErrorSeverity::High)
            .with_recovery_suggestion("Check configuration file syntax and required fields");
        
        let recovery_strategy = RecoveryStrategy::Fallback {
            alternative: "use_default_configuration".to_string(),
        };
        
        Self::Configuration {
            message: err.to_string(),
            context: Box::new(context),
            recovery_strategy: Box::new(recovery_strategy),
        }
    }
}

// FFmpeg conversion disabled due to missing system dependencies
// impl From<ffmpeg_next::Error> for OpenAceError {
//     fn from(err: ffmpeg_next::Error) -> Self {
//         Self::FFmpeg {
//             message: err.to_string(),
//         }
//     }
// }

#[cfg(feature = "python")]
impl From<pyo3::PyErr> for OpenAceError {
    fn from(err: pyo3::PyErr) -> Self {
        Self::Python {
            message: err.to_string(),
        }
    }
}