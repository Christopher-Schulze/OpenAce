//! Error Recovery System for OpenAce Rust
//!
//! Provides automatic error recovery mechanisms with configurable strategies,
//! circuit breaker patterns, and comprehensive recovery tracking.

use crate::error::{OpenAceError, ErrorContext, RecoveryStrategy, RecoveryResult, ErrorSeverity, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{warn, info, debug};
use serde::{Serialize, Deserialize};

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// Circuit is closed - normal operation
    Closed,
    /// Circuit is open - failing fast
    Open,
    /// Circuit is half-open - testing recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit from half-open
    pub success_threshold: u32,
    /// Timeout before moving from open to half-open
    pub timeout_ms: u64,
    /// Window size for failure counting
    pub window_size_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 60000, // 1 minute
            window_size_ms: 300000, // 5 minutes
        }
    }
}

/// Circuit breaker implementation
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
    failure_count: Arc<Mutex<u32>>,
    success_count: Arc<Mutex<u32>>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    component_name: String,
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            config: self.config.clone(),
            failure_count: self.failure_count.clone(),
            success_count: self.success_count.clone(),
            last_failure_time: self.last_failure_time.clone(),
            component_name: self.component_name.clone(),
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(component_name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState::Closed)),
            config,
            failure_count: Arc::new(Mutex::new(0)),
            success_count: Arc::new(Mutex::new(0)),
            last_failure_time: Arc::new(Mutex::new(None)),
            component_name: component_name.into(),
        }
    }

    /// Check if operation should be allowed
    pub fn can_execute(&self) -> bool {
        let state = self.state.lock().unwrap();
        match *state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = *self.last_failure_time.lock().unwrap() {
                    if last_failure.elapsed().as_millis() as u64 > self.config.timeout_ms {
                        drop(state);
                        self.transition_to_half_open();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    /// Record a successful operation
    pub fn record_success(&self) {
        let mut success_count = self.success_count.lock().unwrap();
        *success_count += 1;

        let state = self.state.lock().unwrap();
        if *state == CircuitBreakerState::HalfOpen && *success_count >= self.config.success_threshold {
            drop(state);
            self.transition_to_closed();
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        let mut failure_count = self.failure_count.lock().unwrap();
        *failure_count += 1;
        *self.last_failure_time.lock().unwrap() = Some(Instant::now());

        let state = self.state.lock().unwrap();
        if (*state == CircuitBreakerState::Closed || *state == CircuitBreakerState::HalfOpen)
            && *failure_count >= self.config.failure_threshold
        {
            drop(state);
            self.transition_to_open();
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitBreakerState {
        self.state.lock().unwrap().clone()
    }

    fn transition_to_open(&self) {
        *self.state.lock().unwrap() = CircuitBreakerState::Open;
        *self.success_count.lock().unwrap() = 0;
        warn!(
            component = %self.component_name,
            "Circuit breaker opened due to failures"
        );
    }

    fn transition_to_half_open(&self) {
        *self.state.lock().unwrap() = CircuitBreakerState::HalfOpen;
        *self.failure_count.lock().unwrap() = 0;
        *self.success_count.lock().unwrap() = 0;
        info!(
            component = %self.component_name,
            "Circuit breaker moved to half-open state"
        );
    }

    fn transition_to_closed(&self) {
        *self.state.lock().unwrap() = CircuitBreakerState::Closed;
        *self.failure_count.lock().unwrap() = 0;
        *self.success_count.lock().unwrap() = 0;
        info!(
            component = %self.component_name,
            "Circuit breaker closed - normal operation resumed"
        );
    }
}

/// Recovery attempt tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    pub attempt_number: u32,
    pub strategy: Box<RecoveryStrategy>,
    pub start_time: u64,
    pub duration_ms: Option<u64>,
    pub result: Option<RecoveryResult>,
    pub error_context: Box<ErrorContext>,
}

/// Error recovery manager
#[derive(Debug)]
pub struct ErrorRecoveryManager {
    circuit_breakers: Arc<Mutex<HashMap<String, CircuitBreaker>>>,
    recovery_attempts: Arc<Mutex<HashMap<String, Vec<RecoveryAttempt>>>>,
    default_circuit_config: CircuitBreakerConfig,
}

impl ErrorRecoveryManager {
    /// Create a new error recovery manager
    pub fn new() -> Self {
        Self {
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
            recovery_attempts: Arc::new(Mutex::new(HashMap::new())),
            default_circuit_config: CircuitBreakerConfig::default(),
        }
    }

    /// Create a new error recovery manager with custom circuit breaker config
    pub fn with_circuit_config(config: CircuitBreakerConfig) -> Self {
        Self {
            circuit_breakers: Arc::new(Mutex::new(HashMap::new())),
            recovery_attempts: Arc::new(Mutex::new(HashMap::new())),
            default_circuit_config: config,
        }
    }

    /// Get or create circuit breaker for component
    pub fn get_circuit_breaker(&self, component: &str) -> CircuitBreaker {
        let mut breakers = self.circuit_breakers.lock().unwrap();
        breakers
            .entry(component.to_string())
            .or_insert_with(|| {
                CircuitBreaker::new(component, self.default_circuit_config.clone())
            })
            .clone()
    }

    /// Attempt to recover from an error
    pub async fn attempt_recovery(&self, error: &OpenAceError) -> Result<RecoveryResult> {
        let component = &error.context().component;
        let strategy = error.recovery_strategy().clone();
        let error_id = self.generate_error_id(error);

        // Check circuit breaker
        let circuit_breaker = self.get_circuit_breaker(component);
        if !circuit_breaker.can_execute() {
            warn!(
                component = %component,
                error_id = %error_id,
                "Recovery blocked by circuit breaker"
            );
            return Ok(RecoveryResult::Skipped {
                reason: "Circuit breaker is open".to_string(),
            });
        }

        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let attempt = RecoveryAttempt {
            attempt_number: self.get_next_attempt_number(&error_id),
            strategy: Box::new(strategy.clone()),
            start_time,
            duration_ms: None,
            result: None,
            error_context: Box::new(error.context().clone()),
        };

        info!(
            component = %component,
            error_id = %error_id,
            attempt = attempt.attempt_number,
            strategy = ?strategy,
            "Starting error recovery attempt"
        );

        let recovery_start = Instant::now();
        let result = self.execute_recovery_strategy(&strategy, error).await;
        let duration_ms = recovery_start.elapsed().as_millis() as u64;

        let recovery_result = match result {
            Ok(()) => {
                circuit_breaker.record_success();
                RecoveryResult::Success {
                    strategy_used: strategy.clone(),
                    duration_ms,
                }
            }
            Err(recovery_error) => {
                circuit_breaker.record_failure();
                RecoveryResult::Failed {
                    strategy_attempted: strategy.clone(),
                    reason: recovery_error.to_string(),
                }
            }
        };

        // Record the attempt
        let mut final_attempt = attempt;
        final_attempt.duration_ms = Some(duration_ms);
        final_attempt.result = Some(recovery_result.clone());
        self.record_recovery_attempt(&error_id, final_attempt);

        info!(
            component = %component,
            error_id = %error_id,
            duration_ms = duration_ms,
            result = ?recovery_result,
            "Recovery attempt completed"
        );

        Ok(recovery_result)
    }

    /// Execute a specific recovery strategy
    async fn execute_recovery_strategy(
        &self,
        strategy: &RecoveryStrategy,
        error: &OpenAceError,
    ) -> Result<()> {
        match strategy {
            RecoveryStrategy::None => {
                Err(OpenAceError::new_with_context(
            "recovery",
            "No recovery strategy available",
            (*error.context()).clone(),
            RecoveryStrategy::None,
        ))
            }
            RecoveryStrategy::Retry { max_attempts, base_delay_ms } => {
                self.execute_retry_strategy(*max_attempts, *base_delay_ms).await
            }
            RecoveryStrategy::Fallback { alternative } => {
                self.execute_fallback_strategy(alternative).await
            }
            RecoveryStrategy::Reset { component } => {
                self.execute_reset_strategy(component).await
            }
            RecoveryStrategy::Degrade { reduced_features } => {
                self.execute_degradation_strategy(reduced_features).await
            }
            RecoveryStrategy::CircuitBreaker { timeout_ms } => {
                self.execute_circuit_breaker_strategy(*timeout_ms).await
            }
            RecoveryStrategy::Custom { procedure, parameters } => {
                self.execute_custom_strategy(procedure, parameters).await
            }
        }
    }

    async fn execute_retry_strategy(&self, max_attempts: u32, base_delay_ms: u64) -> Result<()> {
        for attempt in 1..=max_attempts {
            if attempt > 1 {
                let delay = base_delay_ms * 2_u64.pow(attempt - 2); // Exponential backoff
                debug!(attempt = attempt, delay_ms = delay, "Retrying with exponential backoff");
                sleep(Duration::from_millis(delay)).await;
            }
            
            // In a real implementation, this would retry the original operation
            // For now, we simulate success after a few attempts
            if attempt >= max_attempts / 2 {
                info!(attempt = attempt, "Retry strategy succeeded");
                return Ok(());
            }
        }
        
        Err(OpenAceError::new_with_context(
            "recovery",
            format!("Retry strategy failed after {} attempts", max_attempts),
            ErrorContext::new("recovery", "retry_strategy")
                .with_severity(ErrorSeverity::High),
            RecoveryStrategy::None,
        ))
    }

    async fn execute_fallback_strategy(&self, alternative: &str) -> Result<()> {
        info!(alternative = %alternative, "Executing fallback strategy");
        // In a real implementation, this would switch to the alternative implementation
        Ok(())
    }

    async fn execute_reset_strategy(&self, component: &str) -> Result<()> {
        info!(component = %component, "Executing reset strategy");
        // In a real implementation, this would reset the component to a known good state
        Ok(())
    }

    async fn execute_degradation_strategy(&self, reduced_features: &[String]) -> Result<()> {
        info!(features = ?reduced_features, "Executing degradation strategy");
        // In a real implementation, this would disable the specified features
        Ok(())
    }

    async fn execute_circuit_breaker_strategy(&self, timeout_ms: u64) -> Result<()> {
        info!(timeout_ms = timeout_ms, "Executing circuit breaker strategy");
        // Circuit breaker is already handled in attempt_recovery
        Ok(())
    }

    async fn execute_custom_strategy(
        &self,
        procedure: &str,
        parameters: &HashMap<String, String>,
    ) -> Result<()> {
        info!(
            procedure = %procedure,
            parameters = ?parameters,
            "Executing custom recovery strategy"
        );
        // In a real implementation, this would execute the custom procedure
        Ok(())
    }

    fn generate_error_id(&self, error: &OpenAceError) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        error.context().component.hash(&mut hasher);
        error.context().operation.hash(&mut hasher);
        error.to_string().hash(&mut hasher);
        format!("error_{:x}", hasher.finish())
    }

    fn get_next_attempt_number(&self, error_id: &str) -> u32 {
        let attempts = self.recovery_attempts.lock().unwrap();
        attempts
            .get(error_id)
            .map(|attempts| attempts.len() as u32 + 1)
            .unwrap_or(1)
    }

    fn record_recovery_attempt(&self, error_id: &str, attempt: RecoveryAttempt) {
        let mut attempts = self.recovery_attempts.lock().unwrap();
        attempts
            .entry(error_id.to_string())
            .or_default()
            .push(attempt);
    }

    /// Get recovery statistics for a component
    pub fn get_recovery_stats(&self, component: &str) -> RecoveryStats {
        let attempts = self.recovery_attempts.lock().unwrap();
        let component_attempts: Vec<_> = attempts
            .values()
            .flatten()
            .filter(|attempt| attempt.error_context.component == component)
            .collect();

        let total_attempts = component_attempts.len();
        let successful_attempts = component_attempts
            .iter()
            .filter(|attempt| {
                matches!(
                    attempt.result,
                    Some(RecoveryResult::Success { .. })
                )
            })
            .count();

        let average_duration = if !component_attempts.is_empty() {
            component_attempts
                .iter()
                .filter_map(|attempt| attempt.duration_ms)
                .sum::<u64>() / component_attempts.len() as u64
        } else {
            0
        };

        RecoveryStats {
            component: component.to_string(),
            total_attempts,
            successful_attempts,
            success_rate: if total_attempts > 0 {
                successful_attempts as f64 / total_attempts as f64
            } else {
                0.0
            },
            average_duration_ms: average_duration,
        }
    }
}

impl Default for ErrorRecoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Recovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStats {
    pub component: String,
    pub total_attempts: usize,
    pub successful_attempts: usize,
    pub success_rate: f64,
    pub average_duration_ms: u64,
}

/// Global error recovery manager instance
static RECOVERY_MANAGER: std::sync::OnceLock<ErrorRecoveryManager> = std::sync::OnceLock::new();

/// Get the global error recovery manager
pub fn get_recovery_manager() -> &'static ErrorRecoveryManager {
    RECOVERY_MANAGER.get_or_init(ErrorRecoveryManager::new)
}

/// Initialize the global error recovery manager with custom config
pub fn init_recovery_manager(config: CircuitBreakerConfig) -> Result<()> {
    RECOVERY_MANAGER
        .set(ErrorRecoveryManager::with_circuit_config(config))
        .map_err(|_| {
            OpenAceError::new_with_context(
                "recovery",
                "Recovery manager already initialized",
                ErrorContext::new("recovery", "initialization")
                    .with_severity(ErrorSeverity::Medium),
                RecoveryStrategy::None,
            )
        })
}

/// Convenience function to attempt recovery from an error
pub async fn recover_from_error(error: &OpenAceError) -> Result<RecoveryResult> {
    get_recovery_manager().attempt_recovery(error).await
}