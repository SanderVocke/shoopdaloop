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
pub struct AudioBufferQueue {
    /// Internal storage for buffer wrappers
    buffers: VecDeque<AudioBufferData>,
    /// Buffer pool for pre-allocated buffer reuse
    pool: RefillingPool<PooledBuffer>,
    /// Capacity of each buffer
    buffer_size: u32,
    /// Maximum number of buffers to store
    max_buffers: u32,
    /// Current position within the active buffer
    active_buffer_pos: u32,
    /// Whether we have an active buffer started
    has_active_buffer: bool,
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
            active_buffer_pos: 0,
            has_active_buffer: false,
        }
    }

    /// Get the number of samples currently in the queue
    pub fn n_samples(&self) -> u32 {
        if self.buffers.is_empty() {
            return 0;
        }
        let completed_buffers = if self.has_active_buffer {
            self.buffers.len() - 1
        } else {
            self.buffers.len()
        };
        (completed_buffers as u32) * self.buffer_size + self.active_buffer_pos
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
        if self.has_active_buffer {
            self.buffers.len() + 1
        } else {
            self.buffers.len()
        }
    }

    /// Add samples to the queue - no allocations on audio thread
    pub fn put(&mut self, data: &[f32]) {
        let mut remaining = data;

        while !remaining.is_empty() {
            // Get or create a buffer to fill
            let buf = match self.buffers.back_mut() {
                Some(buf) if (buf.n_samples as u32) < self.buffer_size => buf,
                _ => {
                    // Current buffer is full or no buffers exist
                    if self.buffers.len() >= self.max_buffers as usize {
                        self.evict_front();
                    }
                    if let Some(new_buf) = self.get_pooled_buffer() {
                        self.buffers.push_back(new_buf);
                    } else {
                        return; // Pool exhausted
                    }
                    self.buffers.back_mut().unwrap()
                }
            };

            // Calculate how much space is left
            let space = (self.buffer_size as usize).saturating_sub(buf.n_samples);
            let to_copy = remaining.len().min(space);

            // Copy data directly into the buffer (no allocation)
            let n = buf.n_samples;
            let dest = &mut buf.pooled.data[n..n + to_copy];
            dest.copy_from_slice(&remaining[..to_copy]);
            buf.n_samples += to_copy;

            remaining = &remaining[to_copy..];
            self.active_buffer_pos += to_copy as u32;
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
        self.has_active_buffer = true;
    }

    /// Return all buffers to pool
    fn return_all_to_pool(&mut self) {
        while let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
    }

    /// Clear all buffers - returns them to the pool
    pub fn clear(&mut self) {
        self.return_all_to_pool();
        self.active_buffer_pos = 0;
        self.has_active_buffer = false;
    }

    /// Get iterator over buffers for snapshot creation
    pub fn iter_buffers(&self) -> impl Iterator<Item = &AudioBufferData> {
        self.buffers.iter()
    }

    /// Get active buffer position (filled samples in current buffer)
    pub fn active_buffer_pos(&self) -> u32 {
        self.active_buffer_pos
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
        assert_eq!(queue.n_buffers(), 1);
    }

    #[test]
    fn test_multiple_buffers() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);

        let data = vec![1.0f32; 300];
        queue.put(&data);

        assert_eq!(queue.n_samples(), 300);
        assert_eq!(queue.n_buffers(), 3);
    }

    #[test]
    fn test_eviction() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 3);

        let data = vec![1.0f32; 512];
        queue.put(&data);

        assert!(queue.n_buffers() <= 4);
        assert_eq!(queue.n_samples(), 512);
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
        let data = vec![1.0f32; 512];
        queue.put(&data);

        assert!(queue.n_buffers() <= 6);

        queue.set_max_buffers(2);

        assert!(queue.n_buffers() <= 3);
    }
}
