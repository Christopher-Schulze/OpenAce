//! Core types for OpenAce Rust
//!
//! This module defines common types, structures, and enumerations used
//! throughout the OpenAce system.

use crate::error::{OpenAceError, Result};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use std::fmt;
use serde::{Serialize, Deserialize};
use bytes::Bytes;

/// Unique identifier type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id(pub String);

impl Id {
    /// Create a new ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    /// Generate a random ID
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    
    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Id {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Version information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre_release: Option<String>,
    pub build_metadata: Option<String>,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            pre_release: None,
            build_metadata: None,
        }
    }
    
    /// Create a version with pre-release information
    pub fn with_pre_release(mut self, pre_release: impl Into<String>) -> Self {
        self.pre_release = Some(pre_release.into());
        self
    }
    
    /// Create a version with build metadata
    pub fn with_build_metadata(mut self, build_metadata: impl Into<String>) -> Self {
        self.build_metadata = Some(build_metadata.into());
        self
    }
    
    /// Check if this version is compatible with another
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major && self.minor >= other.minor
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        
        if let Some(pre) = &self.pre_release {
            write!(f, "-{}", pre)?;
        }
        
        if let Some(build) = &self.build_metadata {
            write!(f, "+{}", build)?;
        }
        
        Ok(())
    }
}

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Low => write!(f, "Low"),
            Priority::Normal => write!(f, "Normal"),
            Priority::High => write!(f, "High"),
            Priority::Critical => write!(f, "Critical"),
        }
    }
}

/// Status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl Default for Status {
    fn default() -> Self {
        Self::Pending
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Pending => write!(f, "Pending"),
            Status::InProgress => write!(f, "In Progress"),
            Status::Completed => write!(f, "Completed"),
            Status::Failed => write!(f, "Failed"),
            Status::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Media format enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaFormat {
    /// MPEG-4 Part 14
    Mp4,
    /// Audio Video Interleave
    Avi,
    /// Matroska Video
    Mkv,
    /// WebM
    WebM,
    /// MPEG Transport Stream
    Ts,
    /// HTTP Live Streaming
    Hls,
    /// Dynamic Adaptive Streaming over HTTP
    Dash,
    /// Unknown format
    Unknown,
}

impl Default for MediaFormat {
    fn default() -> Self {
        Self::Unknown
    }
}

impl fmt::Display for MediaFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaFormat::Mp4 => write!(f, "MP4"),
            MediaFormat::Avi => write!(f, "AVI"),
            MediaFormat::Mkv => write!(f, "MKV"),
            MediaFormat::WebM => write!(f, "WebM"),
            MediaFormat::Ts => write!(f, "TS"),
            MediaFormat::Hls => write!(f, "HLS"),
            MediaFormat::Dash => write!(f, "DASH"),
            MediaFormat::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Media quality enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MediaQuality {
    Low = 1,
    Medium = 2,
    High = 3,
    UltraHigh = 4,
}

impl Default for MediaQuality {
    fn default() -> Self {
        Self::Medium
    }
}

impl fmt::Display for MediaQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaQuality::Low => write!(f, "Low"),
            MediaQuality::Medium => write!(f, "Medium"),
            MediaQuality::High => write!(f, "High"),
            MediaQuality::UltraHigh => write!(f, "Ultra High"),
        }
    }
}

/// Network protocol enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkProtocol {
    Tcp,
    Udp,
    Http,
    Https,
    WebSocket,
    WebRTC,
    P2P,
}

impl fmt::Display for NetworkProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkProtocol::Tcp => write!(f, "TCP"),
            NetworkProtocol::Udp => write!(f, "UDP"),
            NetworkProtocol::Http => write!(f, "HTTP"),
            NetworkProtocol::Https => write!(f, "HTTPS"),
            NetworkProtocol::WebSocket => write!(f, "WebSocket"),
            NetworkProtocol::WebRTC => write!(f, "WebRTC"),
            NetworkProtocol::P2P => write!(f, "P2P"),
        }
    }
}

/// Data buffer wrapper
#[derive(Debug, Clone)]
pub struct DataBuffer {
    data: Bytes,
    metadata: HashMap<String, String>,
}

impl DataBuffer {
    /// Create a new data buffer
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self {
            data: data.into(),
            metadata: HashMap::new(),
        }
    }
    
    /// Create a data buffer with metadata
    pub fn with_metadata(data: impl Into<Bytes>, metadata: HashMap<String, String>) -> Self {
        Self {
            data: data.into(),
            metadata,
        }
    }
    
    /// Get the data
    pub fn data(&self) -> &Bytes {
        &self.data
    }
    
    /// Get the metadata
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    
    /// Get mutable metadata
    pub fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.metadata
    }
    
    /// Get data length
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Add metadata
    pub fn add_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }
    
    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub size: Option<u64>,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub duration: Option<Duration>,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub metadata: HashMap<String, String>,
}

impl ResourceInfo {
    /// Create new resource info
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            name: name.into(),
            description: None,
            size: None,
            format: MediaFormat::default(),
            quality: MediaQuality::default(),
            duration: None,
            created_at: now,
            modified_at: now,
            metadata: HashMap::new(),
        }
    }
    
    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
    
    /// Set size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
    
    /// Set format
    pub fn with_format(mut self, format: MediaFormat) -> Self {
        self.format = format;
        self
    }
    
    /// Set quality
    pub fn with_quality(mut self, quality: MediaQuality) -> Self {
        self.quality = quality;
        self
    }
    
    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
    
    /// Update modified time
    pub fn touch(&mut self) {
        self.modified_at = SystemTime::now();
    }
    
    /// Add metadata
    pub fn add_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
        self.touch();
    }
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: Id,
    pub protocol: NetworkProtocol,
    pub local_address: String,
    pub remote_address: String,
    pub established_at: SystemTime,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub status: Status,
    pub metadata: HashMap<String, String>,
}

impl ConnectionInfo {
    /// Create new connection info
    pub fn new(
        id: Id,
        protocol: NetworkProtocol,
        local_address: impl Into<String>,
        remote_address: impl Into<String>,
    ) -> Self {
        Self {
            id,
            protocol,
            local_address: local_address.into(),
            remote_address: remote_address.into(),
            established_at: SystemTime::now(),
            bytes_sent: 0,
            bytes_received: 0,
            status: Status::Pending,
            metadata: HashMap::new(),
        }
    }
    
    /// Update bytes sent
    pub fn add_bytes_sent(&mut self, bytes: u64) {
        self.bytes_sent += bytes;
    }
    
    /// Update bytes received
    pub fn add_bytes_received(&mut self, bytes: u64) {
        self.bytes_received += bytes;
    }
    
    /// Set status
    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }
    
    /// Get total bytes transferred
    pub fn total_bytes(&self) -> u64 {
        self.bytes_sent + self.bytes_received
    }
    
    /// Get connection duration
    pub fn duration(&self) -> Duration {
        self.established_at.elapsed().unwrap_or(Duration::ZERO)
    }
}

/// Task information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub priority: Priority,
    pub status: Status,
    pub progress: f32, // 0.0 to 1.0
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl TaskInfo {
    /// Create new task info
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            description: None,
            priority: Priority::default(),
            status: Status::default(),
            progress: 0.0,
            created_at: SystemTime::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
    
    /// Start the task
    pub fn start(&mut self) {
        self.status = Status::InProgress;
        self.started_at = Some(SystemTime::now());
    }
    
    /// Complete the task
    pub fn complete(&mut self) {
        self.status = Status::Completed;
        self.progress = 1.0;
        self.completed_at = Some(SystemTime::now());
    }
    
    /// Fail the task
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = Status::Failed;
        self.error_message = Some(error.into());
        self.completed_at = Some(SystemTime::now());
    }
    
    /// Cancel the task
    pub fn cancel(&mut self) {
        self.status = Status::Cancelled;
        self.completed_at = Some(SystemTime::now());
    }
    
    /// Update progress
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }
    
    /// Get task duration
    pub fn duration(&self) -> Option<Duration> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => end.duration_since(start).ok(),
            (Some(start), None) => start.elapsed().ok(),
            _ => None,
        }
    }
    
    /// Check if task is finished
    pub fn is_finished(&self) -> bool {
        matches!(self.status, Status::Completed | Status::Failed | Status::Cancelled)
    }
}

/// Result wrapper for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub duration: Duration,
    pub timestamp: SystemTime,
}

impl<T> OperationResult<T> {
    /// Create a successful result
    pub fn success(data: T, duration: Duration) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            duration,
            timestamp: SystemTime::now(),
        }
    }
    
    /// Create a failed result
    pub fn failure(error: impl Into<String>, duration: Duration) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
            duration,
            timestamp: SystemTime::now(),
        }
    }
    
    /// Convert to Result
    pub fn into_result(self) -> Result<T> {
        if self.success {
            if let Some(data) = self.data {
                Ok(data)
            } else {
                Err(OpenAceError::internal("Success result without data"))
            }
        } else {
            Err(OpenAceError::internal(
                self.error.unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_id_generation() {
        let id1 = Id::generate();
        let id2 = Id::generate();
        assert_ne!(id1, id2);
        
        let id3 = Id::new("test-id");
        assert_eq!(id3.as_str(), "test-id");
    }
    
    #[test]
    fn test_version_compatibility() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 3, 0);
        let v3 = Version::new(2, 0, 0);
        
        assert!(v2.is_compatible_with(&v1));
        assert!(!v1.is_compatible_with(&v2));
        assert!(!v3.is_compatible_with(&v1));
    }
    
    #[test]
    fn test_version_display() {
        let v1 = Version::new(1, 2, 3);
        assert_eq!(v1.to_string(), "1.2.3");
        
        let v2 = Version::new(1, 2, 3)
            .with_pre_release("alpha")
            .with_build_metadata("build123");
        assert_eq!(v2.to_string(), "1.2.3-alpha+build123");
    }
    
    #[test]
    fn test_data_buffer() {
        let mut buffer = DataBuffer::new(&b"test data"[..]);
        assert_eq!(buffer.len(), 9);
        assert!(!buffer.is_empty());
        
        buffer.add_metadata("type", "test");
        assert_eq!(buffer.get_metadata("type"), Some(&"test".to_string()));
    }
    
    #[test]
    fn test_task_lifecycle() {
        let mut task = TaskInfo::new(Id::generate(), "test task")
            .with_priority(Priority::High);
        
        assert_eq!(task.status, Status::Pending);
        assert_eq!(task.progress, 0.0);
        
        task.start();
        assert_eq!(task.status, Status::InProgress);
        assert!(task.started_at.is_some());
        
        task.set_progress(0.5);
        assert_eq!(task.progress, 0.5);
        
        task.complete();
        assert_eq!(task.status, Status::Completed);
        assert_eq!(task.progress, 1.0);
        assert!(task.is_finished());
    }
    
    #[test]
    fn test_operation_result() {
        let success_result = OperationResult::success("data", Duration::from_millis(100));
        assert!(success_result.success);
        assert_eq!(success_result.data, Some("data"));
        
        let failure_result: OperationResult<String> = OperationResult::failure(
            "error message", 
            Duration::from_millis(50)
        );
        assert!(!failure_result.success);
        assert_eq!(failure_result.error, Some("error message".to_string()));
    }
}