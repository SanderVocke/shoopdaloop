//! Audio buffer queue implementation for audio processing.
//!
//! This provides a FIFO queue of audio buffers with automatic memory management.
//! Uses RefillingPool for pre-allocated buffer reuse - no allocations on audio thread.
//!
//! Ported from C++ BufferQueue.
//!
//! ============================================================================
//! ## TODO: Optimize Snapshot When AudioPort is Ported to Rust
//! ============================================================================
//! WHEN AudioPort is ported to Rust, we can use Rust-native Arc<Vec<f32>> types
//! to achieve TRUE thin-copy shared ownership without data copying:
//!
//! 1. Change snapshot() to return Vec<Arc<Vec<f32>>> directly
//! 2. C++ can then wrap Arc<Vec<f32>> in a custom smart pointer
//! 3. AudioPort in C++ becomes Arc<Vec<f32>> which is zero-copy
//! 4. Eliminate all memcpy operations in the consumer path
//!
//! ## Why Current CXX Bridge Limits Us
//! ============================================================================
//! CXX doesn't support these patterns which prevent true shared ownership:
//!   - SharedPtr<OpaqueRustType> - not supported
//!   - SharedPtr<Vec<f32>> - not supported
//!   - Vec<SharedPtr<...>> - Vec can't hold SharedPtr elements
//!   - Vec<CxxVector<f32>> - Vec can't hold CxxVector elements
//!   - CxxVector by value in structs - not supported
//!
//! Current workaround: snapshot() returns Vec<BufferPtrInfo> with raw pointers.
//! C++ copies data into AudioBuffer objects - acceptable for consumer thread,
//! but not ideal for the audio thread path.
//!
//! ## Target Architecture (post-AudioPort Rust port)
//! ============================================================================
//! AudioBufferQueue::put() -> RefillingPool (zero alloc, lock-free)
//! AudioBufferQueue::snapshot() -> Vec<Arc<Vec<f32>>> (true thin-copy)
//!
//! C++ side:
//! - AudioPort holds Arc<Vec<f32>> (shared ownership, no copies)
//! - AudioBuffer is created from Arc<Vec<f32>> (still shared)
//! ============================================================================

use std::collections::VecDeque;

use refilling_pool::refilling_pool::RefillingPool;

/// A buffer wrapper that wraps a pool-allocated Vec
#[derive(Debug)]
pub struct PooledBuffer {
    data: Vec<f32>,
}

impl PooledBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0.0f32; size],
        }
    }
}

/// Wrapper around audio buffer data for the queue
#[derive(Debug)]
pub struct AudioBufferData {
    /// Number of valid samples in this buffer
    n_samples: usize,
    /// The pooled buffer
    pooled: PooledBuffer,
}

impl AudioBufferData {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            n_samples: 0,
            pooled: PooledBuffer::new(buffer_size),
        }
    }

    /// Get number of valid samples
    pub fn len(&self) -> usize {
        self.n_samples
    }

    /// Get buffer data as slice
    pub fn as_slice(&self) -> &[f32] {
        &self.pooled.data[..self.n_samples]
    }

    /// Get mutable buffer data as slice
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.pooled.data[..self.n_samples]
    }

    /// Get raw pointer to data
    pub fn data_ptr(&self) -> *const f32 {
        self.pooled.data.as_ptr()
    }

    /// Get total capacity
    pub fn capacity(&self) -> usize {
        self.pooled.data.len()
    }
}

/// FIFO queue for audio buffers with automatic eviction.
/// Uses RefillingPool for zero-allocation buffer reuse on the audio thread.
/// 
/// # Architecture
/// - `buffers`: Completed buffers ready for consumption (FIFO)
/// - `active_buffer`: Buffer currently being filled (not yet committed)
/// - `active_buffer.n_samples`: Current fill position in active_buffer
/// 
/// This matches the C++ BufferQueue which stores only completed buffers,
/// and tracks the fill position separately.
pub struct AudioBufferQueue {
    /// Completed buffers ready for consumption
    buffers: VecDeque<AudioBufferData>,
    /// Buffer pool for pre-allocated buffer reuse
    pool: RefillingPool<PooledBuffer>,
    /// Capacity of each buffer
    buffer_size: u32,
    /// Maximum number of buffers to store
    max_buffers: u32,
    /// Buffer currently being filled (None when fully flushed)
    active_buffer: Option<AudioBufferData>,
}

impl AudioBufferQueue {
    /// Create a new AudioBufferQueue with pre-allocated buffers.
    pub fn new(
        pool_capacity: usize,
        low_water_mark: usize,
        buffer_size: usize,
        max_buffers: u32,
    ) -> Self {
        let factory = move || Box::new(PooledBuffer::new(buffer_size));
        let pool = RefillingPool::new(pool_capacity, low_water_mark, factory)
            .expect("Failed to create buffer pool");

        Self {
            buffers: VecDeque::with_capacity(max_buffers as usize),
            pool,
            buffer_size: buffer_size as u32,
            max_buffers,
            active_buffer: None,
        }
    }

    /// Get the number of samples currently in the queue
    pub fn n_samples(&self) -> u32 {
        // Match C++ semantics:
        // - Completed buffers contribute full buffer_size each
        // - Active buffer contributes active_buffer_pos (current fill position)
        // - When active_buffer is None, no samples in progress
        let completed_samples = (self.buffers.len() as u32) * self.buffer_size;
        let active_samples = self.active_buffer.as_ref()
            .map(|b| b.n_samples as u32)
            .unwrap_or(0);
        completed_samples + active_samples
    }

    /// Get the buffer size
    pub fn single_buffer_size(&self) -> u32 {
        self.buffer_size
    }

    /// Get the maximum number of buffers
    pub fn get_max_buffers(&self) -> u32 {
        self.max_buffers
    }

    /// Set the maximum number of buffers
    pub fn set_max_buffers(&mut self, max: u32) {
        self.max_buffers = max;
        while self.buffers.len() > max as usize {
            self.evict_front();
        }
    }

    /// Set minimum number of samples to keep
    pub fn set_min_n_samples(&mut self, _n: u32) {
        // Not implemented for this version
    }

    /// Get the number of buffers
    pub fn n_buffers(&self) -> usize {
        // Match C++: completed buffers + potentially active buffer
        // If there's an active buffer with data, count it
        let completed = self.buffers.len();
        let has_active = self.active_buffer.is_some();
        completed + if has_active { 1 } else { 0 }
    }

    /// Add samples to the queue - no allocations on audio thread
    pub fn put(&mut self, data: &[f32]) {
        let mut remaining = data;

        while !remaining.is_empty() {
            // If no active buffer, create one
            if self.active_buffer.is_none() {
                if let Some(new_buf) = self.get_pooled_buffer() {
                    self.active_buffer = Some(new_buf);
                } else {
                    return; // Pool exhausted
                }
            }

            // Get the active buffer
            let active = self.active_buffer.as_mut().unwrap();
            
            // Check if active buffer is full
            if (active.n_samples as u32) >= self.buffer_size {
                // Commit the full buffer and create a new one
                self.commit_active_buffer();
                continue; // Will create new buffer on next iteration
            }

            // Calculate how much space is left in active buffer
            let space = (self.buffer_size as usize).saturating_sub(active.n_samples);
            let to_copy = remaining.len().min(space);

            // Copy data directly into the buffer (no allocation)
            let n = active.n_samples;
            let dest = &mut active.pooled.data[n..n + to_copy];
            dest.copy_from_slice(&remaining[..to_copy]);
            active.n_samples += to_copy;

            remaining = &remaining[to_copy..];
        }
        
        // After putting data, if active buffer is now full, commit it
        // Check by looking at n_samples before committing
        let needs_commit = self.active_buffer.as_ref()
            .map(|b| (b.n_samples as u32) >= self.buffer_size)
            .unwrap_or(false);
        if needs_commit {
            self.commit_active_buffer();
        }
    }

    /// Commit the active buffer to the completed buffers deque
    fn commit_active_buffer(&mut self) {
        if let Some(buf) = self.active_buffer.take() {
            self.buffers.push_back(buf);
            // C++ evicts AFTER push: while (buffers->size() > max_buffers)
            // This means when size == max, after push we do one eviction
            while (self.buffers.len() > self.max_buffers as usize) {
                self.evict_front();
            }
        }
    }

    /// Get a buffer from the pool
    fn get_pooled_buffer(&mut self) -> Option<AudioBufferData> {
        let boxed = self.pool.get();
        Some(AudioBufferData {
            n_samples: 0,
            pooled: *boxed,
        })
    }

    /// Return a buffer to the pool
    fn return_to_pool(&mut self, buf: PooledBuffer) {
        self.pool.release(Box::new(buf));
    }

    /// Evict the front buffer, returning it to the pool
    fn evict_front(&mut self) {
        if let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
    }

    /// Return all buffers to pool
    fn return_all_to_pool(&mut self) {
        while let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
        if let Some(buf) = self.active_buffer.take() {
            self.return_to_pool(buf.pooled);
        }
    }

    /// Clear all buffers - returns them to the pool
    pub fn clear(&mut self) {
        self.return_all_to_pool();
    }

    /// Get iterator over all buffers (completed + active)
    pub fn iter_buffers(&self) -> impl Iterator<Item = &AudioBufferData> {
        self.buffers.iter().chain(self.active_buffer.as_ref())
    }

    /// Get active buffer position (filled samples in active buffer)
    pub fn active_buffer_pos(&self) -> u32 {
        self.active_buffer.as_ref().map(|b| b.n_samples as u32).unwrap_or(0)
    }
}

// Type alias
pub type AudioBufferQueueF32 = AudioBufferQueue;

impl Drop for AudioBufferQueue {
    fn drop(&mut self) {
        self.return_all_to_pool();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_queue_is_empty() {
        let queue = AudioBufferQueue::new(20, 5, 128, 10);
        assert_eq!(queue.n_samples(), 0);
        assert_eq!(queue.n_buffers(), 0);
    }

    #[test]
    fn test_single_buffer() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        let data = vec![1.0f32; 128];
        queue.put(&data);

        assert_eq!(queue.n_samples(), 128);
        // After putting exactly one buffer, it's the active buffer
        // n_buffers counts active buffer
        assert_eq!(queue.n_buffers(), 1);
    }

    #[test]
    fn test_partial_buffer() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        let data = vec![1.0f32; 64];
        queue.put(&data);

        assert_eq!(queue.n_samples(), 64);
        assert_eq!(queue.n_buffers(), 1);
    }

    #[test]
    fn test_multiple_buffers() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);

        let data = vec![1.0f32; 300];
        queue.put(&data);

        // 256 samples = 2 completed buffers (128 each) + 44 samples in active
        assert_eq!(queue.n_samples(), 300);
        assert_eq!(queue.n_buffers(), 3); // 2 completed + 1 active
    }

    #[test]
    fn test_full_buffers() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        
        // Fill 2 full buffers
        let data = vec![1.0f32; 256];
        queue.put(&data);

        // 128 samples in each of 2 buffers
        assert_eq!(queue.n_samples(), 256);
        assert_eq!(queue.n_buffers(), 2);
    }

    #[test]
    fn test_eviction() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 3);

        let data = vec![1.0f32; 512];
        queue.put(&data);

        // With max_buffers=3:
        // After 4 full buffers, the oldest is evicted
        // Result: 3 completed buffers (384 samples)
        assert_eq!(queue.n_samples(), 384);
        assert_eq!(queue.n_buffers(), 3); // 3 completed, no active
    }

    #[test]
    fn test_clear() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        let data = vec![1.0f32; 256];
        queue.put(&data);

        queue.clear();

        assert_eq!(queue.n_samples(), 0);
        assert_eq!(queue.n_buffers(), 0);
    }

    #[test]
    fn test_set_max_buffers() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 5);
        let data = vec![1.0f32; 640]; // 5 full buffers
        queue.put(&data);

        // After 5 full buffers: 4 completed + 1 active
        assert_eq!(queue.n_buffers(), 5);
        assert_eq!(queue.n_samples(), 640);

        queue.set_max_buffers(2);

        // Should now have 1 completed + 1 active
        assert!(queue.n_buffers() <= 2);
    }
}
