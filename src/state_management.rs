//! State Management System for OpenAce Rust
//!
//! Provides comprehensive state synchronization, validation, and persistence
//! with support for distributed state management and conflict resolution.

use crate::error::{OpenAceError, ErrorContext, RecoveryStrategy, ErrorSeverity, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use tokio::sync::{broadcast, watch};
use tracing::{info, warn, debug};
use std::fmt::Debug;

use std::future::Future;
use std::pin::Pin;

// Type aliases to reduce complexity
type SaveFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type LoadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<Option<ManagedState<T>>>> + Send + 'a>>;
type DeleteFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type ListFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>>;

/// State change event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChangeType {
    /// State was created
    Created,
    /// State was updated
    Updated,
    /// State was deleted
    Deleted,
    /// State was synchronized from remote
    Synchronized,
    /// State validation failed
    ValidationFailed,
    /// State conflict detected
    ConflictDetected,
}

/// State change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChangeEvent<T> {
    /// Unique identifier for the state
    pub state_id: String,
    /// Type of change
    pub change_type: StateChangeType,
    /// Previous state value (if applicable)
    pub previous_value: Option<T>,
    /// New state value (if applicable)
    pub new_value: Option<T>,
    /// Timestamp of the change
    pub timestamp: u64,
    /// Component that initiated the change
    pub source_component: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// State validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationResult {
    /// State is valid
    Valid,
    /// State is invalid with reasons
    Invalid { reasons: Vec<String> },
    /// State validation warning
    Warning { warnings: Vec<String> },
}

/// State validator trait
pub trait StateValidator<T> {
    /// Validate a state value
    fn validate(&self, value: &T) -> ValidationResult;
    
    /// Get validator name for logging
    fn name(&self) -> &str;
}

/// State conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    /// Use the latest timestamp (last writer wins)
    LastWriterWins,
    /// Use the value from a specific source
    PreferSource { source: String },
    /// Merge values using a custom strategy
    CustomMerge { strategy: String },
    /// Reject the change and keep current value
    RejectChange,
    /// Manual resolution required
    ManualResolution,
}

/// State metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMetadata {
    /// When the state was created
    pub created_at: u64,
    /// When the state was last updated
    pub updated_at: u64,
    /// Version number for optimistic locking
    pub version: u64,
    /// Component that owns this state
    pub owner: String,
    /// Last component to modify this state
    pub last_modifier: String,
    /// State checksum for integrity verification
    pub checksum: String,
    /// Additional tags for categorization
    pub tags: Vec<String>,
}

/// Managed state wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedState<T> {
    /// The actual state value
    pub value: T,
    /// State metadata
    pub metadata: StateMetadata,
    /// Validation status
    pub validation_status: ValidationResult,
}

/// State persistence trait
pub trait StatePersistence<T>: Send + Sync {
    /// Save state to persistent storage
    fn save(&self, state_id: &str, state: &ManagedState<T>) -> SaveFuture<'_>;
    
    /// Load state from persistent storage
    fn load(&self, state_id: &str) -> LoadFuture<'_, T>;
    
    /// Delete state from persistent storage
    fn delete(&self, state_id: &str) -> DeleteFuture<'_>;
    
    /// List all state IDs
    fn list_states(&self) -> ListFuture<'_>;
}

/// In-memory state persistence implementation
#[derive(Debug, Default)]
pub struct MemoryStatePersistence<T> {
    storage: Arc<RwLock<HashMap<String, ManagedState<T>>>>,
}

impl<T> MemoryStatePersistence<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl<T> StatePersistence<T> for MemoryStatePersistence<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    fn save(&self, state_id: &str, state: &ManagedState<T>) -> SaveFuture<'_> {
        let state_id = state_id.to_string();
        let state = state.clone();
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let mut storage = storage.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire write lock: {}", e),
                    ErrorContext::new("state_management", "persistence_save")
                        .with_severity(ErrorSeverity::High),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            storage.insert(state_id.clone(), state);
            debug!(state_id = %state_id, "State saved to memory persistence");
            Ok(())
        })
    }

    fn load(&self, state_id: &str) -> LoadFuture<'_, T> {
        let state_id = state_id.to_string();
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let storage = storage.read().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire read lock: {}", e),
                    ErrorContext::new("state_management", "persistence_load")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            let result = storage.get(&state_id).cloned();
            debug!(state_id = %state_id, found = result.is_some(), "State loaded from memory persistence");
            Ok(result)
        })
    }

    fn delete(&self, state_id: &str) -> DeleteFuture<'_> {
        let state_id = state_id.to_string();
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let mut storage = storage.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire write lock: {}", e),
                    ErrorContext::new("state_management", "persistence_delete")
                        .with_severity(ErrorSeverity::High),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            let existed = storage.remove(&state_id).is_some();
            debug!(state_id = %state_id, existed = existed, "State deleted from memory persistence");
            Ok(())
        })
    }

    fn list_states(&self) -> ListFuture<'_> {
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let storage = storage.read().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire read lock: {}", e),
                    ErrorContext::new("state_management", "persistence_list")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            let states: Vec<String> = storage.keys().cloned().collect();
            debug!(count = states.len(), "Listed states from memory persistence");
            Ok(states)
        })
    }
}

/// State manager for handling state lifecycle and synchronization
pub struct StateManager<T> {
    /// In-memory state cache
    states: Arc<RwLock<HashMap<String, ManagedState<T>>>>,
    /// State validators
    validators: Arc<RwLock<Vec<Box<dyn StateValidator<T> + Send + Sync>>>>,
    /// State persistence layer
    persistence: Arc<dyn StatePersistence<T> + Send + Sync>,
    /// State change event broadcaster
    event_sender: broadcast::Sender<StateChangeEvent<T>>,
    /// State watchers for specific states
    watchers: Arc<Mutex<HashMap<String, watch::Sender<Option<T>>>>>,
    /// Conflict resolution strategy
    conflict_resolution: ConflictResolution,
    /// Component identifier
    component_id: String,
}

impl<T> std::fmt::Debug for StateManager<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + Debug + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .field("states", &self.states)
            .field("validators", &"<validators>")
            .field("persistence", &"<persistence>")
            .field("watchers", &self.watchers)
            .field("conflict_resolution", &self.conflict_resolution)
            .field("component_id", &self.component_id)
            .finish()
    }
}

impl<T> StateManager<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + Debug + 'static,
{
    /// Create a new state manager
    pub fn new(
        component_id: impl Into<String>,
        persistence: Arc<dyn StatePersistence<T> + Send + Sync>,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            validators: Arc::new(RwLock::new(Vec::new())),
            persistence,
            event_sender,
            watchers: Arc::new(Mutex::new(HashMap::new())),
            conflict_resolution: ConflictResolution::LastWriterWins,
            component_id: component_id.into(),
        }
    }

    /// Create a new state manager with memory persistence
    pub fn with_memory_persistence(component_id: impl Into<String>) -> Self {
        let persistence = Arc::new(MemoryStatePersistence::new());
        Self::new(component_id, persistence)
    }

    /// Set conflict resolution strategy
    pub fn with_conflict_resolution(mut self, strategy: ConflictResolution) -> Self {
        self.conflict_resolution = strategy;
        self
    }

    /// Add a state validator
    pub fn add_validator(&self, validator: Box<dyn StateValidator<T> + Send + Sync>) -> Result<()> {
        let mut validators = self.validators.write().map_err(|e| {
            OpenAceError::new_with_context(
                "state",
                format!("Failed to acquire validator lock: {}", e),
                ErrorContext::new("state_management", "add_validator")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;
        
        validators.push(validator);
        Ok(())
    }

    /// Subscribe to state change events
    pub fn subscribe_to_changes(&self) -> broadcast::Receiver<StateChangeEvent<T>> {
        self.event_sender.subscribe()
    }

    /// Watch a specific state for changes
    pub fn watch_state(&self, state_id: &str) -> watch::Receiver<Option<T>> {
        let mut watchers = self.watchers.lock().unwrap();
        
        if let Some(sender) = watchers.get(state_id) {
            sender.subscribe()
        } else {
            let (sender, receiver) = watch::channel(None);
            watchers.insert(state_id.to_string(), sender);
            receiver
        }
    }

    /// Set a state value
    pub async fn set_state(&self, state_id: &str, value: T) -> Result<()> {
        self.set_state_with_metadata(state_id, value, HashMap::new()).await
    }

    /// Set a state value with metadata
    pub async fn set_state_with_metadata(
        &self,
        state_id: &str,
        value: T,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Validate the new value
        let validation_result = self.validate_state(&value).await?;
        if matches!(validation_result, ValidationResult::Invalid { .. }) {
            return Err(OpenAceError::new_with_context(
                "state",
                "State validation failed",
                ErrorContext::new("state_management", "set_state")
                    .with_severity(ErrorSeverity::High)
                    .with_metadata("state_id", state_id),
                RecoveryStrategy::None,
            ));
        }

        // Get current state for conflict detection
        let previous_state = self.get_managed_state(state_id).await?;
        let (previous_value, version, created_at) = if let Some(current) = &previous_state {
            (
                Some(current.value.clone()),
                current.metadata.version + 1,
                current.metadata.created_at,
            )
        } else {
            (None, 1, timestamp)
        };

        // Create checksum for integrity
        let checksum = self.calculate_checksum(&value)?;

        // Create new managed state
        let managed_state = ManagedState {
            value: value.clone(),
            metadata: StateMetadata {
                created_at,
                updated_at: timestamp,
                version,
                owner: self.component_id.clone(),
                last_modifier: self.component_id.clone(),
                checksum,
                tags: Vec::new(),
            },
            validation_status: validation_result,
        };

        // Handle potential conflicts
        if let Some(current) = previous_state {
            if let Err(conflict_error) = self.resolve_conflict(&current, &managed_state).await {
                warn!(
                    state_id = %state_id,
                    error = %conflict_error,
                    "State conflict detected"
                );
                
                // Emit conflict event
                let conflict_event = StateChangeEvent {
                    state_id: state_id.to_string(),
                    change_type: StateChangeType::ConflictDetected,
                    previous_value: Some(current.value),
                    new_value: Some(value.clone()),
                    timestamp,
                    source_component: self.component_id.clone(),
                    metadata,
                };
                
                let _ = self.event_sender.send(conflict_event);
                return Err(conflict_error);
            }
        }

        // Update in-memory cache
        {
            let mut states = self.states.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire state lock: {}", e),
                    ErrorContext::new("state_management", "set_state")
                        .with_severity(ErrorSeverity::High),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    }
                )
            })?;
            
            states.insert(state_id.to_string(), managed_state.clone());
        }

        // Persist to storage
        self.persistence.save(state_id, &managed_state).await?;

        // Notify watchers
        if let Ok(mut watchers) = self.watchers.lock() {
            if let Some(sender) = watchers.get_mut(state_id) {
                let _ = sender.send(Some(value.clone()));
            }
        }

        // Emit change event
        let change_type = if previous_value.is_some() {
            StateChangeType::Updated
        } else {
            StateChangeType::Created
        };
        
        let event = StateChangeEvent {
            state_id: state_id.to_string(),
            change_type,
            previous_value,
            new_value: Some(value),
            timestamp,
            source_component: self.component_id.clone(),
            metadata,
        };
        
        let _ = self.event_sender.send(event);

        info!(
            state_id = %state_id,
            version = version,
            "State updated successfully"
        );

        Ok(())
    }

    /// Get a state value
    pub async fn get_state(&self, state_id: &str) -> Result<Option<T>> {
        let managed_state = self.get_managed_state(state_id).await?;
        Ok(managed_state.map(|state| state.value))
    }

    /// Get managed state with metadata
    pub async fn get_managed_state(&self, state_id: &str) -> Result<Option<ManagedState<T>>> {
        // Try in-memory cache first
        {
            let states = self.states.read().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire state lock: {}", e),
                    ErrorContext::new("state_management", "get_state")
                         .with_severity(ErrorSeverity::Medium),
                     RecoveryStrategy::Retry {
                         max_attempts: 2,
                         base_delay_ms: 50,
                     },
                )
            })?;
            
            if let Some(state) = states.get(state_id) {
                return Ok(Some(state.clone()));
            }
        }

        // Load from persistence if not in cache
        let persisted_state = self.persistence.load(state_id).await?;
        
        if let Some(state) = &persisted_state {
            // Update cache
            let mut states = self.states.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire state lock: {}", e),
                    ErrorContext::new("state_management", "get_state")
                        .with_severity(ErrorSeverity::Medium),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            states.insert(state_id.to_string(), state.clone());
        }

        Ok(persisted_state)
    }

    /// Delete a state
    pub async fn delete_state(&self, state_id: &str) -> Result<()> {
        let previous_state = self.get_managed_state(state_id).await?;
        
        // Remove from cache
        {
            let mut states = self.states.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire state lock: {}", e),
                    ErrorContext::new("state_management", "delete_state")
                        .with_severity(ErrorSeverity::High),
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay_ms: 100,
                    },
                )
            })?;
            
            states.remove(state_id);
        }

        // Remove from persistence
        self.persistence.delete(state_id).await?;

        // Notify watchers
        if let Ok(mut watchers) = self.watchers.lock() {
            if let Some(sender) = watchers.get_mut(state_id) {
                let _ = sender.send(None);
            }
        }

        // Emit delete event
        if let Some(previous) = previous_state {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            
            let event = StateChangeEvent {
                state_id: state_id.to_string(),
                change_type: StateChangeType::Deleted,
                previous_value: Some(previous.value),
                new_value: None,
                timestamp,
                source_component: self.component_id.clone(),
                metadata: HashMap::new(),
            };
            
            let _ = self.event_sender.send(event);
        }

        info!(state_id = %state_id, "State deleted successfully");
        Ok(())
    }

    /// List all state IDs
    pub async fn list_states(&self) -> Result<Vec<String>> {
        self.persistence.list_states().await
    }

    /// Synchronize state from external source
    pub async fn sync_state(&self, state_id: &str, external_state: ManagedState<T>) -> Result<()> {
        let current_state = self.get_managed_state(state_id).await?;
        
        // Handle conflict resolution
        if let Some(current) = current_state {
            if self.resolve_conflict(&current, &external_state).await.is_err() {
                // Apply conflict resolution strategy
                match &self.conflict_resolution {
                    ConflictResolution::LastWriterWins => {
                        if external_state.metadata.updated_at <= current.metadata.updated_at {
                            debug!(
                                state_id = %state_id,
                                "Ignoring older external state"
                            );
                            return Ok(());
                        }
                    }
                    ConflictResolution::RejectChange => {
                        debug!(
                            state_id = %state_id,
                            "Rejecting external state change due to conflict"
                        );
                        return Ok(());
                    }
                    _ => {
                        warn!(
                            state_id = %state_id,
                            "Unhandled conflict resolution strategy"
                        );
                    }
                }
            }
        }

        // Update state
        {
            let mut states = self.states.write().map_err(|e| {
                OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire state lock: {}", e),
                ErrorContext::new("state_management", "sync_state")
                     .with_severity(ErrorSeverity::High),
                 RecoveryStrategy::Retry {
                     max_attempts: 3,
                     base_delay_ms: 100,
                 },
            )
        })?;
            
            states.insert(state_id.to_string(), external_state.clone());
        }

        // Persist
        self.persistence.save(state_id, &external_state).await?;

        // Notify watchers
        if let Ok(mut watchers) = self.watchers.lock() {
            if let Some(sender) = watchers.get_mut(state_id) {
                let _ = sender.send(Some(external_state.value.clone()));
            }
        }

        // Emit sync event
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        let event = StateChangeEvent {
            state_id: state_id.to_string(),
            change_type: StateChangeType::Synchronized,
            previous_value: None,
            new_value: Some(external_state.value),
            timestamp,
            source_component: "external".to_string(),
            metadata: HashMap::new(),
        };
        
        let _ = self.event_sender.send(event);

        info!(state_id = %state_id, "State synchronized successfully");
        Ok(())
    }

    /// Validate state using registered validators
    async fn validate_state(&self, value: &T) -> Result<ValidationResult> {
        let validators = self.validators.read().map_err(|e| {
            OpenAceError::new_with_context(
                    "state",
                    format!("Failed to acquire validator lock: {}", e),
                ErrorContext::new("state_management", "validate_state")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay_ms: 100,
                },
            )
        })?;

        let mut all_errors = Vec::new();
        let mut all_warnings = Vec::new();

        for validator in validators.iter() {
            match validator.validate(value) {
                ValidationResult::Valid => {}
                ValidationResult::Invalid { mut reasons } => {
                    all_errors.append(&mut reasons);
                }
                ValidationResult::Warning { mut warnings } => {
                    all_warnings.append(&mut warnings);
                }
            }
        }

        if !all_errors.is_empty() {
            Ok(ValidationResult::Invalid { reasons: all_errors })
        } else if !all_warnings.is_empty() {
            Ok(ValidationResult::Warning { warnings: all_warnings })
        } else {
            Ok(ValidationResult::Valid)
        }
    }

    /// Resolve conflicts between states
    async fn resolve_conflict(
        &self,
        current: &ManagedState<T>,
        new: &ManagedState<T>,
    ) -> Result<()> {
        // Check version conflicts
        if new.metadata.version <= current.metadata.version {
            return Err(OpenAceError::new_with_context(
                "state",
                "Version conflict detected",
                ErrorContext::new("state_management", "resolve_conflict")
                    .with_severity(ErrorSeverity::Medium)
                    .with_metadata("current_version", current.metadata.version.to_string())
                    .with_metadata("new_version", new.metadata.version.to_string()),
                RecoveryStrategy::None,
            ));
        }

        // Check timestamp conflicts
        if new.metadata.updated_at < current.metadata.updated_at {
            return Err(OpenAceError::new_with_context(
                "state",
                "Timestamp conflict detected",
                ErrorContext::new("state_management", "resolve_conflict")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::None,
            ));
        }

        Ok(())
    }

    /// Calculate checksum for state integrity
    fn calculate_checksum(&self, value: &T) -> Result<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let serialized = serde_json::to_string(value).map_err(|e| {
            OpenAceError::new_with_context(
                "state",
                format!("Failed to serialize state for checksum: {}", e),
                ErrorContext::new("state_management", "calculate_checksum")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::None,
            )
        })?;

        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        Ok(format!("{:x}", hasher.finish()))
    }
}

/// Global state managers registry
static STATE_MANAGERS: std::sync::OnceLock<Mutex<HashMap<String, Box<dyn std::any::Any + Send + Sync>>>> = std::sync::OnceLock::new();

/// Get or create a state manager for a specific type and component
pub fn get_state_manager<T>(
    component_id: &str,
    type_name: &str,
) -> Arc<StateManager<T>>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + Debug + 'static,
{
    let managers = STATE_MANAGERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut managers_map = managers.lock().unwrap();
    
    let key = format!("{}:{}", component_id, type_name);
    
    if let Some(manager_any) = managers_map.get(&key) {
        if let Some(manager) = manager_any.downcast_ref::<Arc<StateManager<T>>>() {
            return manager.clone();
        }
    }
    
    let manager = Arc::new(StateManager::with_memory_persistence(component_id));
    managers_map.insert(key, Box::new(manager.clone()));
    manager
}