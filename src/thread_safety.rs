//! Thread safety infrastructure for OpenAce Rust
//!
//! Provides atomic operations and mutex wrappers that mirror the C implementation
//! for compatibility with the original acestream engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::fmt::Debug;
use parking_lot::{Mutex as ParkingMutex, RwLock as ParkingRwLock};

/// Thread-safe mutex wrapper compatible with acestream_mutex_t
#[derive(Debug)]
pub struct SafeMutex<T> {
    inner: Arc<ParkingMutex<T>>,
}

impl<T> SafeMutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(ParkingMutex::new(value)),
        }
    }

    pub fn lock(&self) -> parking_lot::MutexGuard<T> {
        self.inner.lock()
    }

    pub fn try_lock(&self) -> Option<parking_lot::MutexGuard<T>> {
        self.inner.try_lock()
    }
}

impl<T> Clone for SafeMutex<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Thread-safe RwLock wrapper
#[derive(Debug)]
pub struct SafeRwLock<T> {
    inner: Arc<ParkingRwLock<T>>,
}

impl<T> SafeRwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(ParkingRwLock::new(value)),
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<T> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<T> {
        self.inner.write()
    }

    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<T>> {
        self.inner.try_read()
    }

    pub fn try_write(&self) -> Option<parking_lot::RwLockWriteGuard<T>> {
        self.inner.try_write()
    }
}

impl<T> Clone for SafeRwLock<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Atomic integer wrapper compatible with acestream_atomic_int_t
#[derive(Debug)]
pub struct SafeAtomicInt {
    value: AtomicI32,
}

impl SafeAtomicInt {
    pub fn new(initial_value: i32) -> Self {
        Self {
            value: AtomicI32::new(initial_value),
        }
    }

    pub fn get(&self) -> i32 {
        self.value.load(Ordering::Acquire)
    }

    pub fn set(&self, value: i32) {
        self.value.store(value, Ordering::Release);
    }

    pub fn compare_and_swap(&self, expected: i32, new_value: i32) -> i32 {
        self.value.compare_exchange(expected, new_value, Ordering::AcqRel, Ordering::Acquire)
            .unwrap_or_else(|x| x)
    }

    pub fn increment(&self) -> i32 {
        self.value.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn decrement(&self) -> i32 {
        self.value.fetch_sub(1, Ordering::AcqRel) - 1
    }
}

/// Atomic boolean wrapper compatible with acestream_atomic_bool_t
#[derive(Debug)]
pub struct SafeAtomicBool {
    value: AtomicBool,
}

impl SafeAtomicBool {
    pub fn new(initial_value: bool) -> Self {
        Self {
            value: AtomicBool::new(initial_value),
        }
    }

    pub fn get(&self) -> bool {
        self.value.load(Ordering::Acquire)
    }

    pub fn set(&self, value: bool) {
        self.value.store(value, Ordering::Release);
    }

    pub fn compare_and_swap(&self, expected: bool, new_value: bool) -> bool {
        self.value.compare_exchange(expected, new_value, Ordering::AcqRel, Ordering::Acquire)
            .unwrap_or_else(|x| x)
    }
}

/// Type alias for destructor function
type Destructor<T> = Box<dyn Fn(&T) + Send + Sync>;

/// Reference counting wrapper compatible with acestream_refcount_t
pub struct SafeRefCount<T> {
    count: AtomicUsize,
    data: Arc<T>,
    destructor: Option<Destructor<T>>,
}

impl<T> Debug for SafeRefCount<T> where T: Debug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeRefCount")
            .field("count", &self.count.load(Ordering::Relaxed))
            .field("data", &self.data)
            .field("destructor", &"<dyn Fn>")
            .finish()
    }
}

impl<T> SafeRefCount<T> {
    /// Create a new reference counter
    pub fn new(data: T) -> Self {
        Self {
            count: AtomicUsize::new(1),
            data: Arc::new(data),
            destructor: None,
        }
    }

    /// Create with custom destructor
    pub fn new_with_destructor<F>(data: T, destructor: F) -> Self 
    where 
        F: Fn(&T) + Send + Sync + 'static
    {
        Self {
            count: AtomicUsize::new(1),
            data: Arc::new(data),
            destructor: Some(Box::new(destructor)),
        }
    }

    /// Acquire a reference (increment count)
    pub fn acquire(&self) {
        self.count.fetch_add(1, Ordering::AcqRel);
    }

    /// Release a reference (decrement count)
    pub fn release(&self) -> bool {
        let old_count = self.count.fetch_sub(1, Ordering::AcqRel);
        if old_count == 1 {
            // Last reference, call destructor if available
            if let Some(ref destructor) = self.destructor {
                destructor(&self.data);
            }
            true
        } else {
            false
        }
    }

    /// Get current reference count
    pub fn get_count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// Get reference to the data
    pub fn data(&self) -> &Arc<T> {
        &self.data
    }
}

/// Engine state enumeration compatible with acestream engine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum EngineState {
    Uninitialized = 0,
    Initializing = 1,
    Initialized = 2,
    Running = 3,
    Paused = 4,
    Stopping = 5,
    Error = -1,
}

impl From<i32> for EngineState {
    fn from(value: i32) -> Self {
        match value {
            0 => EngineState::Uninitialized,
            1 => EngineState::Initializing,
            2 => EngineState::Initialized,
            3 => EngineState::Running,
            4 => EngineState::Paused,
            5 => EngineState::Stopping,
            _ => EngineState::Error,
        }
    }
}

impl From<EngineState> for i32 {
    fn from(state: EngineState) -> Self {
        state as i32
    }
}

/// Thread-safe engine state manager
#[derive(Debug)]
pub struct EngineStateManager {
    state: SafeAtomicInt,
    mutex: SafeMutex<()>,
}

impl EngineStateManager {
    /// Create a new engine state manager
    pub fn new() -> Self {
        Self {
            state: SafeAtomicInt::new(EngineState::Uninitialized as i32),
            mutex: SafeMutex::new(()),
        }
    }

    /// Get current state
    pub fn get_state(&self) -> EngineState {
        EngineState::from(self.state.get())
    }

    /// Set new state with thread safety
    pub fn set_state(&self, new_state: EngineState) {
        let _guard = self.mutex.lock();
        self.state.set(new_state as i32);
    }

    /// Compare and swap state atomically
    pub fn compare_and_swap_state(&self, expected: EngineState, new_state: EngineState) -> bool {
        let result = self.state.compare_and_swap(expected as i32, new_state as i32);
        result == expected as i32
    }

    /// Check if engine is in a running state
    pub fn is_running(&self) -> bool {
        matches!(self.get_state(), EngineState::Running)
    }

    /// Check if engine is initialized
    pub fn is_initialized(&self) -> bool {
        !matches!(self.get_state(), EngineState::Uninitialized | EngineState::Error)
    }
}

impl Default for EngineStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;


    #[test]
    fn test_safe_mutex() {
        let mutex = SafeMutex::new(42);
        let mut guard = mutex.lock();
        *guard = 100;
        drop(guard);
        let guard = mutex.lock();
        assert_eq!(*guard, 100);
        // Entferne Aufrufe zu destroy und is_initialized
    }

    #[test]
    fn test_safe_rwlock() {
        let rwlock = SafeRwLock::new(42);
        let read = rwlock.read();
        assert_eq!(*read, 42);
        drop(read);
        let mut write = rwlock.write();
        *write = 100;
        drop(write);
        let read = rwlock.read();
        assert_eq!(*read, 100);
        // Entferne Aufrufe zu destroy und is_initialized
    }

    #[test]
    fn test_safe_atomic_int() {
        let atomic = SafeAtomicInt::new(0);
        assert_eq!(atomic.get(), 0);
        atomic.set(5);
        assert_eq!(atomic.get(), 5);
        assert_eq!(atomic.increment(), 6);
        assert_eq!(atomic.decrement(), 5);
        assert_eq!(atomic.compare_and_swap(5, 10), 5);
        assert_eq!(atomic.get(), 10);
        // Entferne Aufrufe zu destroy und is_initialized
    }

    // Ähnlich für andere Tests: Passe an, entferne veraltete Methodenaufrufe

    #[test]
    fn test_safe_atomic_bool() {
        let atomic = SafeAtomicBool::new(false);
        assert!(!atomic.get());
        
        atomic.set(true);
        assert!(atomic.get());
        
        let old = atomic.compare_and_swap(true, false);
        assert!(old);
        assert!(!atomic.get());
    }

    #[test]
    fn test_engine_state_manager() {
        let manager = EngineStateManager::new();
        assert_eq!(manager.get_state(), EngineState::Uninitialized);
        assert!(!manager.is_initialized());
        assert!(!manager.is_running());
        
        manager.set_state(EngineState::Initialized);
        assert_eq!(manager.get_state(), EngineState::Initialized);
        assert!(manager.is_initialized());
        assert!(!manager.is_running());
        
        manager.set_state(EngineState::Running);
        assert!(manager.is_running());
        
        let swapped = manager.compare_and_swap_state(EngineState::Running, EngineState::Paused);
        assert!(swapped);
        assert_eq!(manager.get_state(), EngineState::Paused);
    }

    #[test]
    fn test_safe_ref_count() {
        let refcount = SafeRefCount::new("test data".to_string());
        assert_eq!(refcount.get_count(), 1);
        
        refcount.acquire();
        assert_eq!(refcount.get_count(), 2);
        
        let released = refcount.release();
        assert!(!released);
        assert_eq!(refcount.get_count(), 1);
        
        let released = refcount.release();
        assert!(released);
        assert_eq!(refcount.get_count(), 0);
    }

    #[test]
    fn test_concurrent_access() {
        let mutex = Arc::new(SafeMutex::new(0));
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let mutex = Arc::clone(&mutex);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let mut guard = mutex.lock();
                        *guard += 1;
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*mutex.lock(), 1000);
    }
}