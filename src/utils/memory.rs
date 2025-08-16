//! Memory management utilities for OpenAce Rust
//!
//! Provides memory-safe alternatives to the C memory management functions
//! with automatic cleanup, leak detection, and performance monitoring.

use crate::error::{OpenAceError, Result};
use crate::core::traits::Lifecycle;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::ptr::NonNull;
use tracing::{debug, warn};

/// Memory allocation statistics
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MemoryStats {
    /// Total bytes allocated
    pub total_allocated: usize,
    /// Total bytes deallocated
    pub total_deallocated: usize,
    /// Current bytes in use
    pub current_usage: usize,
    /// Peak memory usage
    pub peak_usage: usize,
    /// Number of allocations
    pub allocation_count: usize,
    /// Number of deallocations
    pub deallocation_count: usize,
}

impl MemoryStats {
    /// Get current memory usage in MB
    pub fn current_usage_mb(&self) -> f64 {
        self.current_usage as f64 / (1024.0 * 1024.0)
    }
    
    /// Get peak memory usage in MB
    pub fn peak_usage_mb(&self) -> f64 {
        self.peak_usage as f64 / (1024.0 * 1024.0)
    }
    
    /// Check if there are potential memory leaks
    pub fn has_potential_leaks(&self) -> bool {
        self.allocation_count > self.deallocation_count
    }
}

/// Non-serializable atomic counters for global memory stats
#[derive(Debug, Default)]
struct MemoryStatsAtomic {
    total_allocated: AtomicUsize,
    total_deallocated: AtomicUsize,
    current_usage: AtomicUsize,
    peak_usage: AtomicUsize,
    allocation_count: AtomicUsize,
    deallocation_count: AtomicUsize,
}

/// Global memory statistics
static MEMORY_STATS: MemoryStatsAtomic = MemoryStatsAtomic {
    total_allocated: AtomicUsize::new(0),
    total_deallocated: AtomicUsize::new(0),
    current_usage: AtomicUsize::new(0),
    peak_usage: AtomicUsize::new(0),
    allocation_count: AtomicUsize::new(0),
    deallocation_count: AtomicUsize::new(0),
};

/// Memory pool for efficient allocation of small objects
#[derive(Debug)]
pub struct MemoryPool {
    /// Pool name for debugging
    name: String,
    /// Block size for this pool
    block_size: usize,
    /// Available blocks
    available_blocks: RwLock<Vec<NonNull<u8>>>,
    /// Total blocks allocated
    total_blocks: AtomicUsize,
    /// Blocks in use
    blocks_in_use: AtomicUsize,
}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new(name: String, block_size: usize, initial_blocks: usize) -> Result<Self> {
        let mut available_blocks = Vec::with_capacity(initial_blocks);
        
        // Pre-allocate initial blocks
        for _ in 0..initial_blocks {
            let layout = Layout::from_size_align(block_size, std::mem::align_of::<u8>())
                .map_err(|e| OpenAceError::memory_allocation(format!("Invalid layout: {}", e)))?;
                
            let ptr = unsafe { System.alloc(layout) };
            if ptr.is_null() {
                return Err(OpenAceError::memory_allocation(
                    "Failed to allocate memory pool block"
                ));
            }
            
            available_blocks.push(NonNull::new(ptr).unwrap());
        }
        
        Ok(Self {
            name,
            block_size,
            available_blocks: RwLock::new(available_blocks),
            total_blocks: AtomicUsize::new(initial_blocks),
            blocks_in_use: AtomicUsize::new(0),
        })
    }
    
    /// Allocate a block from the pool
    pub fn allocate(&self) -> Result<NonNull<u8>> {
        let mut blocks = self.available_blocks.write();
        
        if let Some(block) = blocks.pop() {
            self.blocks_in_use.fetch_add(1, Ordering::Relaxed);
            debug!("Allocated block from pool '{}'", self.name);
            Ok(block)
        } else {
            // Pool is empty, allocate a new block
            let layout = Layout::from_size_align(self.block_size, std::mem::align_of::<u8>())
                .map_err(|e| OpenAceError::memory_allocation(format!("Invalid layout: {}", e)))?;
                
            let ptr = unsafe { System.alloc(layout) };
            if ptr.is_null() {
                return Err(OpenAceError::memory_allocation(
                    "Failed to allocate new memory pool block"
                ));
            }
            
            self.total_blocks.fetch_add(1, Ordering::Relaxed);
            self.blocks_in_use.fetch_add(1, Ordering::Relaxed);
            
            debug!("Allocated new block for pool '{}'", self.name);
            Ok(NonNull::new(ptr).unwrap())
        }
    }
    
    /// Deallocate a block back to the pool
    pub fn deallocate(&self, block: NonNull<u8>) {
        let mut blocks = self.available_blocks.write();
        blocks.push(block);
        self.blocks_in_use.fetch_sub(1, Ordering::Relaxed);
        debug!("Deallocated block to pool '{}'", self.name);
        drop(blocks);
        self.trim();
    }
    
    /// Trim unused blocks to free memory
    pub fn trim(&self) {
        let mut blocks = self.available_blocks.write();
        let available = blocks.len();
        let total = self.total_blocks.load(Ordering::Relaxed);
        
        if available > total / 2 && available > 10 {
            let to_free = available / 2;
            let layout = Layout::from_size_align(self.block_size, std::mem::align_of::<u8>()).unwrap();
            
            for _ in 0..to_free {
                if let Some(block) = blocks.pop() {
                    unsafe {
                        System.dealloc(block.as_ptr(), layout);
                    }
                    self.total_blocks.fetch_sub(1, Ordering::Relaxed);
                }
            }
            debug!("Trimmed {} blocks from pool '{}'", to_free, self.name);
        }
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> (usize, usize, usize) {
        let available = self.available_blocks.read().len();
        let total = self.total_blocks.load(Ordering::Relaxed);
        let in_use = self.blocks_in_use.load(Ordering::Relaxed);
        (total, in_use, available)
    }
}

impl Drop for MemoryPool {
    fn drop(&mut self) {
        let blocks = self.available_blocks.get_mut();
        let layout = Layout::from_size_align(self.block_size, std::mem::align_of::<u8>()).unwrap();
        
        for block in blocks.drain(..) {
            unsafe {
                System.dealloc(block.as_ptr(), layout);
            }
        }
        
        debug!("Memory pool '{}' destroyed", self.name);
    }
}

/// Global memory pool manager
static MEMORY_POOLS: once_cell::sync::Lazy<RwLock<HashMap<String, Arc<MemoryPool>>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

/// Memory-safe buffer that automatically manages its lifetime
#[derive(Debug)]
pub struct SafeBuffer {
    /// Raw data pointer
    data: NonNull<u8>,
    /// Buffer size
    size: usize,
    /// Buffer capacity
    capacity: usize,
    /// Memory pool (if allocated from pool)
    pool: Option<Arc<MemoryPool>>,
}

impl SafeBuffer {
    /// Create a new buffer with specified capacity
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(OpenAceError::memory_allocation("Cannot allocate zero-sized buffer"));
        }
        
        let layout = Layout::from_size_align(capacity, std::mem::align_of::<u8>())
            .map_err(|e| OpenAceError::memory_allocation(format!("Invalid layout: {}", e)))?;
            
        let ptr = unsafe { System.alloc(layout) };
        if ptr.is_null() {
            return Err(OpenAceError::memory_allocation(
                format!("Failed to allocate {} bytes", capacity)
            ));
        }
        
        // Update global statistics
        MEMORY_STATS.total_allocated.fetch_add(capacity, Ordering::Relaxed);
        MEMORY_STATS.current_usage.fetch_add(capacity, Ordering::Relaxed);
        MEMORY_STATS.allocation_count.fetch_add(1, Ordering::Relaxed);
        
        let current = MEMORY_STATS.current_usage.load(Ordering::Relaxed);
        let mut peak = MEMORY_STATS.peak_usage.load(Ordering::Relaxed);
        while current > peak {
            match MEMORY_STATS.peak_usage.compare_exchange_weak(
                peak, current, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(x) => peak = x,
            }
        }
        
        Ok(Self {
            data: NonNull::new(ptr).unwrap(),
            size: 0,
            capacity,
            pool: None,
        })
    }
    
    /// Create a buffer from a memory pool
    pub fn from_pool(pool_name: &str) -> Result<Self> {
        let pools = MEMORY_POOLS.read();
        let pool = pools.get(pool_name)
            .ok_or_else(|| OpenAceError::memory_allocation(
                format!("Memory pool '{}' not found", pool_name)
            ))?
            .clone();
        drop(pools);
        
        let block = pool.allocate()?;
        
        Ok(Self {
            data: block,
            size: 0,
            capacity: pool.block_size,
            pool: Some(pool),
        })
    }
    
    /// Get buffer data as slice
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data.as_ptr(), self.size) }
    }
    
    /// Get buffer data as mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.data.as_ptr(), self.size) }
    }
    
    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    
    /// Get current buffer size
    pub fn len(&self) -> usize {
        self.size
    }
    
    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    
    /// Resize buffer (truncate or extend with zeros)
    pub fn resize(&mut self, new_size: usize) -> Result<()> {
        if new_size > self.capacity {
            return Err(OpenAceError::memory_allocation(
                format!("Cannot resize buffer to {} bytes (capacity: {})", new_size, self.capacity)
            ));
        }
        
        if new_size > self.size {
            // Zero out new bytes
            let new_bytes = new_size - self.size;
            unsafe {
                std::ptr::write_bytes(
                    self.data.as_ptr().add(self.size),
                    0,
                    new_bytes
                );
            }
        }
        
        self.size = new_size;
        Ok(())
    }
    
    /// Copy data into buffer
    pub fn copy_from_slice(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.capacity {
            return Err(OpenAceError::memory_allocation(
                format!("Data size {} exceeds buffer capacity {}", data.len(), self.capacity)
            ));
        }
        
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.data.as_ptr(),
                data.len()
            );
        }
        
        self.size = data.len();
        Ok(())
    }
}

impl Drop for SafeBuffer {
    fn drop(&mut self) {
        if let Some(pool) = &self.pool {
            // Return to pool
            pool.deallocate(self.data);
        } else {
            // Free directly
            let layout = Layout::from_size_align(self.capacity, std::mem::align_of::<u8>()).unwrap();
            unsafe {
                System.dealloc(self.data.as_ptr(), layout);
            }
            
            // Update global statistics
            MEMORY_STATS.total_deallocated.fetch_add(self.capacity, Ordering::Relaxed);
            MEMORY_STATS.current_usage.fetch_sub(self.capacity, Ordering::Relaxed);
            MEMORY_STATS.deallocation_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// Safety: SafeBuffer owns its data and ensures exclusive access
unsafe impl Send for SafeBuffer {}
unsafe impl Sync for SafeBuffer {}

// MemoryPool is safe to send between threads and share references
// because all mutable state is protected by RwLock and AtomicUsize
unsafe impl Send for MemoryPool {}
unsafe impl Sync for MemoryPool {}

#[async_trait::async_trait]
impl Lifecycle for MemoryPool {
    async fn initialize(&mut self) -> Result<()> {
        debug!("Initializing memory pool: {}", self.name);
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down memory pool: {}", self.name);
        Ok(())
    }

    fn is_initialized(&self) -> bool {
        // MemoryPool is considered initialized after construction
        true
    }
}

// Note: Arc<MemoryPool> cannot implement Lifecycle trait because it requires &mut self
// but Arc provides only shared references. The underlying MemoryPool implements Lifecycle.

/// Initialize memory management subsystem
pub async fn initialize_memory_management() -> Result<()> {
    tracing::info!("Initializing memory management");
    
    // Create default memory pools
    create_memory_pool("small".to_string(), 1024, 100).await?;      // 1KB blocks
    create_memory_pool("medium".to_string(), 64 * 1024, 50).await?;  // 64KB blocks
    create_memory_pool("large".to_string(), 1024 * 1024, 10).await?; // 1MB blocks
    
    tracing::info!("Memory management initialized with default pools");
    Ok(())
}

/// Shutdown memory management subsystem
pub async fn shutdown_memory_management() -> Result<()> {
    tracing::info!("Shutting down memory management");
    
    // Check for memory leaks
    let stats = get_memory_stats();
    if stats.has_potential_leaks() {
        warn!(
            "Potential memory leaks detected: {} allocations, {} deallocations",
            stats.allocation_count, stats.deallocation_count
        );
    }
    
    // Clear all memory pools
    MEMORY_POOLS.write().clear();
    
    tracing::info!(
        "Memory management shutdown complete. Final stats: {:.2}MB allocated, {:.2}MB peak",
        stats.total_allocated as f64 / (1024.0 * 1024.0),
        stats.peak_usage_mb()
    );
    
    Ok(())
}

/// Create a new memory pool
pub async fn create_memory_pool(
    name: String,
    block_size: usize,
    initial_blocks: usize,
) -> Result<()> {
    let pool = Arc::new(MemoryPool::new(name.clone(), block_size, initial_blocks)?);
    MEMORY_POOLS.write().insert(name.clone(), pool);
    
    debug!("Created memory pool '{}' with {} blocks of {} bytes", name, initial_blocks, block_size);
    Ok(())
}

/// Get current memory statistics
pub fn get_memory_stats() -> MemoryStats {
    MemoryStats {
        total_allocated: MEMORY_STATS.total_allocated.load(Ordering::Relaxed),
        total_deallocated: MEMORY_STATS.total_deallocated.load(Ordering::Relaxed),
        current_usage: MEMORY_STATS.current_usage.load(Ordering::Relaxed),
        peak_usage: MEMORY_STATS.peak_usage.load(Ordering::Relaxed),
        allocation_count: MEMORY_STATS.allocation_count.load(Ordering::Relaxed),
        deallocation_count: MEMORY_STATS.deallocation_count.load(Ordering::Relaxed),
    }
}

/// Get memory pool statistics
pub fn get_pool_stats() -> HashMap<String, (usize, usize, usize)> {
    let pools = MEMORY_POOLS.read();
    pools.iter()
        .map(|(name, pool)| (name.clone(), pool.stats()))
        .collect()
}

/// Memory-safe alternative to C's malloc
pub fn safe_malloc(size: usize) -> Result<SafeBuffer> {
    SafeBuffer::new(size)
}

/// Memory-safe alternative to C's calloc (zero-initialized)
pub fn safe_calloc(count: usize, size: usize) -> Result<SafeBuffer> {
    let total_size = count.checked_mul(size)
        .ok_or_else(|| OpenAceError::memory_allocation("Integer overflow in calloc"))?;
        
    let mut buffer = SafeBuffer::new(total_size)?;
    buffer.resize(total_size)?; // This zeros the memory
    Ok(buffer)
}

/// Memory-safe copy operation
pub fn safe_memcpy(dest: &mut [u8], src: &[u8]) -> Result<()> {
    if dest.len() < src.len() {
        return Err(OpenAceError::memory_allocation(
            "Destination buffer too small for memcpy"
        ));
    }
    
    dest[..src.len()].copy_from_slice(src);
    Ok(())
}

/// Memory-safe set operation
pub fn safe_memset(buffer: &mut [u8], value: u8) {
    buffer.fill(value);
}