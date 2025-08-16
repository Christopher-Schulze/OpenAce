//! Threading utilities for OpenAce Rust
//!
//! Provides thread-safe primitives, atomic operations, and async-aware
//! synchronization mechanisms as modern alternatives to C threading.

use crate::error::{OpenAceError, Result};
use std::sync::Arc;
use parking_lot::Condvar;
use tokio::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicI64, Ordering};
use tokio::sync::{Semaphore, Notify};
use tokio::sync::Mutex as TokioMutex;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Thread-safe reference counter
#[derive(Debug)]
pub struct AtomicRefCount {
    count: AtomicUsize,
}

impl AtomicRefCount {
    /// Create a new reference counter with initial value
    pub fn new(initial: usize) -> Self {
        Self {
            count: AtomicUsize::new(initial),
        }
    }
    
    /// Increment reference count
    pub fn increment(&self) -> usize {
        self.count.fetch_add(1, Ordering::Relaxed) + 1
    }
    
    /// Decrement reference count
    pub fn decrement(&self) -> usize {
        let prev = self.count.fetch_sub(1, Ordering::Relaxed);
        if prev == 0 {
            panic!("Reference count underflow");
        }
        prev - 1
    }
    
    /// Get current reference count
    pub fn get(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
    
    /// Check if reference count is zero
    pub fn is_zero(&self) -> bool {
        self.get() == 0
    }
}

/// Thread-safe boolean flag
#[derive(Debug)]
pub struct AtomicFlag {
    flag: AtomicBool,
}

impl AtomicFlag {
    /// Create a new atomic flag
    pub fn new(initial: bool) -> Self {
        Self {
            flag: AtomicBool::new(initial),
        }
    }
    
    /// Set the flag to true
    pub fn set(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
    
    /// Set the flag to false
    pub fn clear(&self) {
        self.flag.store(false, Ordering::Relaxed);
    }
    
    /// Get the current flag value
    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
    
    /// Test and set the flag atomically
    pub fn test_and_set(&self) -> bool {
        self.flag.swap(true, Ordering::Relaxed)
    }
    
    /// Compare and swap the flag value
    pub fn compare_and_swap(&self, current: bool, new: bool) -> bool {
        self.flag.compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }
}

/// Thread-safe integer counter
#[derive(Debug)]
pub struct AtomicCounter {
    value: AtomicI64,
}

impl AtomicCounter {
    /// Create a new atomic counter
    pub fn new(initial: i64) -> Self {
        Self {
            value: AtomicI64::new(initial),
        }
    }
    
    /// Increment the counter
    pub fn increment(&self) -> i64 {
        self.value.fetch_add(1, Ordering::Relaxed) + 1
    }
    
    /// Decrement the counter
    pub fn decrement(&self) -> i64 {
        self.value.fetch_sub(1, Ordering::Relaxed) - 1
    }
    
    /// Add a value to the counter
    pub fn add(&self, value: i64) -> i64 {
        self.value.fetch_add(value, Ordering::Relaxed) + value
    }
    
    /// Subtract a value from the counter
    pub fn sub(&self, value: i64) -> i64 {
        self.value.fetch_sub(value, Ordering::Relaxed) - value
    }
    
    /// Get the current counter value
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
    
    /// Set the counter value
    pub fn set(&self, value: i64) {
        self.value.store(value, Ordering::Relaxed);
    }
    
    /// Compare and swap the counter value
    pub fn compare_and_swap(&self, current: i64, new: i64) -> Result<i64> {
        self.value
            .compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed)
            .map_err(|actual| OpenAceError::thread_safety(format!(
                "CAS failed: expected {}, found {}",
                current, actual
            )))
    }
}

/// Thread-safe mutex wrapper
#[derive(Debug)]
pub struct SafeMutex<T> {
    inner: Arc<TokioMutex<T>>,
    name: String,
}

impl<T> SafeMutex<T> {
    /// Create a new safe mutex
    pub fn new(data: T, name: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(TokioMutex::new(data)),
            name: name.into(),
        }
    }
    
    /// Lock the mutex
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
        debug!("Acquiring mutex lock: {}", self.name);
        let start = Instant::now();
        let guard = self.inner.lock().await;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            warn!("Slow mutex acquisition: {} took {}ms", self.name, duration.as_millis());
        }
        guard
    }
    
    /// Try to lock the mutex without blocking
    pub fn try_lock(&self) -> Result<tokio::sync::MutexGuard<'_, T>> {
        self.inner
            .try_lock()
            .map_err(|e| OpenAceError::thread_safety(format!("Mutex try_lock failed: {}", e)))
    }
    
    /// Get the mutex name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the inner Arc<TokioMutex<T>> to allow cloning for async tasks
    pub fn inner(&self) -> Arc<TokioMutex<T>> {
        Arc::clone(&self.inner)
    }
}

/// Thread-safe read-write lock wrapper
#[derive(Debug)]
pub struct SafeRwLock<T> {
    inner: Arc<RwLock<T>>,
    name: String,
}

impl<T> SafeRwLock<T> {
    /// Create a new safe read-write lock
    pub fn new(data: T, name: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(data)),
            name: name.into(),
        }
    }
    
    /// Acquire a read lock
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<T> {
        debug!("Acquiring read lock: {}", self.name);
        self.inner.read().await
    }
    
    /// Acquire a write lock
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<T> {
        debug!("Acquiring write lock: {}", self.name);
        let start = Instant::now();
        let guard = self.inner.write().await;
        let duration = start.elapsed();
        
        if duration > Duration::from_millis(100) {
            warn!("Slow write lock acquisition: {} took {}ms", self.name, duration.as_millis());
        }
        
        guard
    }
    
    /// Try to acquire a read lock without blocking
    pub fn try_read(&self) -> Result<tokio::sync::RwLockReadGuard<T>> {
        self.inner
            .try_read()
            .map_err(|e| OpenAceError::thread_safety(format!("RwLock try_read failed: {}", e)))
    }
    
    /// Try to acquire a write lock without blocking
    pub fn try_write(&self) -> Result<tokio::sync::RwLockWriteGuard<T>> {
        self.inner
            .try_write()
            .map_err(|e| OpenAceError::thread_safety(format!("RwLock try_write failed: {}", e)))
    }
    
    /// Get the lock name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get the inner Arc<RwLock<T>>
    pub fn inner(&self) -> Arc<RwLock<T>> {
        Arc::clone(&self.inner)
    }
}

/// Condition variable for thread synchronization
#[derive(Debug)]
pub struct SafeCondvar {
    condvar: Condvar,
    name: String,
}

impl SafeCondvar {
    /// Create a new condition variable
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            condvar: Condvar::new(),
            name: name.into(),
        }
    }
    
    /// Wait on the condition variable
    pub fn wait<'a, T>(&self, mut guard: parking_lot::MutexGuard<'a, T>) -> parking_lot::MutexGuard<'a, T> {
        debug!("Waiting on condition variable: {}", self.name);
        self.condvar.wait(&mut guard);
        guard
    }
    
    /// Wait with timeout
    pub fn wait_timeout<'a, T>(
        &self,
        mut guard: parking_lot::MutexGuard<'a, T>,
        timeout: Duration,
    ) -> (parking_lot::MutexGuard<'a, T>, bool) {
        debug!("Waiting on condition variable with timeout: {} ({}ms)", self.name, timeout.as_millis());
        let result = self.condvar.wait_for(&mut guard, timeout);
        (guard, !result.timed_out())
    }
    
    /// Notify one waiting thread
    pub fn notify_one(&self) {
        debug!("Notifying one thread on condition variable: {}", self.name);
        self.condvar.notify_one();
    }
    
    /// Notify all waiting threads
    pub fn notify_all(&self) {
        debug!("Notifying all threads on condition variable: {}", self.name);
        self.condvar.notify_all();
    }
}

/// Async-aware semaphore
#[derive(Debug, Clone)]
pub struct AsyncSemaphore {
    semaphore: Arc<Semaphore>,
    name: String,
}

impl AsyncSemaphore {
    /// Create a new async semaphore
    pub fn new(permits: usize, name: impl Into<String>) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            name: name.into(),
        }
    }
    
    /// Acquire a permit
    pub async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit> {
        debug!("Acquiring semaphore permit: {}", self.name);
        self.semaphore.acquire().await
            .map_err(|e| OpenAceError::thread_safety(format!("Semaphore acquire failed: {}", e)))
    }
    
    /// Try to acquire a permit without waiting
    pub fn try_acquire(&self) -> Result<tokio::sync::SemaphorePermit> {
        self.semaphore.try_acquire()
            .map_err(|e| OpenAceError::thread_safety(format!("Semaphore try_acquire failed: {}", e)))
    }
    
    /// Get available permits
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
    
    /// Add permits to the semaphore
    pub fn add_permits(&self, n: usize) {
        self.semaphore.add_permits(n);
        debug!("Added {} permits to semaphore: {}", n, self.name);
    }
}

/// Async notification primitive
#[derive(Debug, Clone)]
pub struct AsyncNotify {
    notify: Arc<Notify>,
    name: String,
}

impl AsyncNotify {
    /// Create a new async notify
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            name: name.into(),
        }
    }
    
    /// Wait for notification
    pub async fn wait(&self) {
        debug!("Waiting for notification: {}", self.name);
        self.notify.notified().await;
    }
    
    /// Notify one waiting task
    pub fn notify_one(&self) {
        debug!("Sending notification: {}", self.name);
        self.notify.notify_one();
    }
    
    /// Notify all waiting tasks
    pub fn notify_waiters(&self) {
        debug!("Notifying all waiters: {}", self.name);
        self.notify.notify_waiters();
    }
}

/// Thread pool for CPU-intensive tasks
#[derive(Debug, Clone)]
pub struct ThreadPool {
    #[allow(dead_code)]
    pool: Arc<tokio::runtime::Handle>,
    name: String,
}

impl ThreadPool {
    /// Create a new thread pool
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            pool: Arc::new(tokio::runtime::Handle::current()),
            name: name.into(),
        }
    }
    
    /// Spawn a CPU-intensive task
    pub fn spawn_blocking<F, R>(&self, f: F) -> tokio::task::JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        debug!("Spawning blocking task on thread pool: {}", self.name);
        tokio::task::spawn_blocking(f)
    }
    
    /// Spawn an async task
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        debug!("Spawning async task on thread pool: {}", self.name);
        tokio::task::spawn(future)
    }
}

/// Global threading state
static THREADING_STATE: once_cell::sync::OnceCell<Arc<RwLock<ThreadingState>>> = 
    once_cell::sync::OnceCell::new();

#[derive(Debug)]
struct ThreadingState {
    initialized: bool,
    thread_pools: std::collections::HashMap<String, ThreadPool>,
    semaphores: std::collections::HashMap<String, AsyncSemaphore>,
    notifications: std::collections::HashMap<String, AsyncNotify>,
}

/// Initialize threading subsystem
pub async fn initialize_threading() -> Result<()> {
    tracing::info!("Initializing threading subsystem");
    
    let state = ThreadingState {
        initialized: true,
        thread_pools: std::collections::HashMap::new(),
        semaphores: std::collections::HashMap::new(),
        notifications: std::collections::HashMap::new(),
    };
    
    THREADING_STATE.set(Arc::new(RwLock::new(state)))
        .map_err(|_| OpenAceError::internal("Threading already initialized"))?;
    
    // Create default thread pool
    create_thread_pool("default".to_string()).await?;
    
    // Create default semaphores
    create_semaphore("io".to_string(), 100).await?;
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    create_semaphore("cpu".to_string(), cpus).await?;
    
    tracing::info!("Threading subsystem initialized");
    Ok(())
}

/// Shutdown threading subsystem
pub async fn shutdown_threading() -> Result<()> {
    if let Some(state_arc) = THREADING_STATE.get() {
        let mut state = state_arc.write().await;
        if state.initialized {
            tracing::info!("Shutting down threading subsystem");
            
            // Clear all resources
            state.thread_pools.clear();
            state.semaphores.clear();
            state.notifications.clear();
            state.initialized = false;
        }
    }
    
    Ok(())
}

/// Create a new thread pool
pub async fn create_thread_pool(name: String) -> Result<()> {
    if let Some(state_arc) = THREADING_STATE.get() {
        let mut state = state_arc.write().await;
        let pool = ThreadPool::new(name.clone());
        state.thread_pools.insert(name.clone(), pool);
        debug!("Created thread pool: {}", name);
    }
    Ok(())
}

/// Create a new semaphore
pub async fn create_semaphore(name: String, permits: usize) -> Result<()> {
    if let Some(state_arc) = THREADING_STATE.get() {
        let mut state = state_arc.write().await;
        let semaphore = AsyncSemaphore::new(permits, name.clone());
        state.semaphores.insert(name.clone(), semaphore);
        debug!("Created semaphore '{}' with {} permits", name, permits);
    }
    Ok(())
}

/// Create a new notification
pub async fn create_notification(name: String) -> Result<()> {
    if let Some(state_arc) = THREADING_STATE.get() {
        let mut state = state_arc.write().await;
        let notification = AsyncNotify::new(name.clone());
        state.notifications.insert(name.clone(), notification);
        debug!("Created notification: {}", name);
    }
    Ok(())
}

/// Get a thread pool by name
pub fn get_thread_pool(name: &str) -> Option<ThreadPool> {
    THREADING_STATE
        .get()
        .and_then(|state| state.try_read().ok().and_then(|g| g.thread_pools.get(name).cloned()))
}

/// Get a semaphore by name
pub fn get_semaphore(name: &str) -> Option<AsyncSemaphore> {
    THREADING_STATE
        .get()
        .and_then(|state| state.try_read().ok().and_then(|g| g.semaphores.get(name).cloned()))
}

/// Get a notification by name
pub fn get_notification(name: &str) -> Option<AsyncNotify> {
    THREADING_STATE
        .get()
        .and_then(|state| state.try_read().ok().and_then(|g| g.notifications.get(name).cloned()))
}

/// Spawn a task on the default thread pool
pub fn spawn_task<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::task::spawn(future)
}

/// Spawn a blocking task on the default thread pool
pub fn spawn_blocking_task<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}

/// Sleep for a specified duration (async)
pub async fn async_sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Timeout wrapper for async operations
pub async fn with_timeout<F, T>(future: F, timeout: Duration) -> Result<T>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(timeout, future).await
        .map_err(|_| OpenAceError::Timeout { 
            operation: "Operation".to_string(), 
            timeout_ms: timeout.as_millis() as u64,
            context: Box::new(crate::error::ErrorContext::new("threading", "with_timeout")),
            recovery_strategy: Box::new(crate::error::RecoveryStrategy::Retry {
                max_attempts: 3,
                base_delay_ms: 100
            }),
        })
}