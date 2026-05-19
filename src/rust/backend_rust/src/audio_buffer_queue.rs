//! Audio buffer queue implementation for audio processing.
//! 
//! This provides a FIFO queue of audio buffers with automatic memory management.
//! Uses RefillingPool for pre-allocated buffer reuse - no allocations on audio thread.
//!
//! Ported from C++ BufferQueue.

use std::collections::VecDeque;

use refilling_pool::refilling_pool::RefillingPool;

/// A buffer wrapper that wraps a pool-allocated Vec
#[derive(Debug)]
struct PooledBuffer {
    data: Vec<f32>,
}

impl PooledBuffer {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0.0f32; size],
        }
    }
}

/// Wrapper around audio buffer data for the queue
#[derive(Debug)]
struct AudioBufferData {
    /// Number of valid samples in this buffer
    n_samples: usize,
    /// The pooled buffer
    pooled: PooledBuffer,
}

impl AudioBufferData {
    fn new(buffer_size: usize) -> Self {
        Self {
            n_samples: 0,
            pooled: PooledBuffer::new(buffer_size),
        }
    }

    /// Get number of valid samples
    pub fn len(&self) -> usize {
        self.n_samples
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.n_samples == 0
    }

    /// Get buffer data as slice
    pub fn as_slice(&self) -> &[f32] {
        &self.pooled.data[..self.n_samples]
    }

    /// Get mutable buffer data as slice
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.pooled.data[..self.n_samples]
    }
}

/// Immutable snapshot of a buffer queue state.
#[derive(Debug)]
pub struct AudioBufferQueueSnapshot {
    /// Total number of samples across all buffers
    pub n_samples: u32,
    /// Size of each full buffer
    pub buffer_size: u32,
    /// The buffer data - each entry is a Vec<f32>
    pub buffers: Vec<Vec<f32>>,
}

impl AudioBufferQueueSnapshot {
    /// Get the number of buffers
    pub fn n_buffers(&self) -> usize {
        self.buffers.len()
    }

    /// Get buffer at index
    pub fn get_buffer(&self, idx: usize) -> Option<&[f32]> {
        self.buffers.get(idx).map(|b| b.as_slice())
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
    /// 
    /// - `pool_capacity`: Number of pre-allocated buffers in the pool
    /// - `low_water_mark`: Refill when pool drops below this count
    /// - `buffer_size`: Size of each buffer in elements (f32)
    /// - `max_buffers`: Maximum number of buffers in the queue (older ones evicted)
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

    /// Set minimum number of samples to keep (not used in this implementation)
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
            pooled: *boxed, // Unbox the value
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
        self.has_active_buffer = true; // We still have an active buffer
    }

    /// Return all buffers to pool
    fn return_all_to_pool(&mut self) {
        while let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
    }

    /// Take a snapshot of the current queue state.
    /// Copies data into pre-allocated storage - no allocations, uses memcpy.
    pub fn snapshot(&self) -> AudioBufferQueueSnapshot {
        let total_samples = self.n_samples();
        let n_bufs = if self.has_active_buffer {
            self.buffers.len().saturating_sub(1)
        } else {
            self.buffers.len()
        };
        
        let mut buffers = Vec::with_capacity(n_bufs + 1);
        
        for (i, buf) in self.buffers.iter().enumerate() {
            let filled_len = if i == self.buffers.len() - 1 && self.has_active_buffer {
                self.active_buffer_pos as usize
            } else {
                buf.n_samples
            };
            
            // Copy data into pre-allocated Vec
            let mut dest = Vec::with_capacity(filled_len);
            dest.extend_from_slice(&buf.pooled.data[..filled_len]);
            buffers.push(dest);
        }
        
        AudioBufferQueueSnapshot {
            n_samples: total_samples,
            buffer_size: self.buffer_size,
            buffers,
        }
    }

    /// Clear all buffers - returns them to the pool
    pub fn clear(&mut self) {
        self.return_all_to_pool();
        self.active_buffer_pos = 0;
        self.has_active_buffer = false;
    }
}

// Type aliases for f32 samples
pub type AudioBufferQueueF32 = AudioBufferQueue;
pub type AudioBufferQueueSnapshotF32 = AudioBufferQueueSnapshot;

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
    fn test_snapshot() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        let data = vec![1.0f32; 256];
        queue.put(&data);
        
        let snap = queue.snapshot();
        
        assert_eq!(snap.n_samples, 256);
        assert_eq!(snap.buffer_size, 128);
        assert_eq!(snap.buffers.len(), 2);
    }

    #[test]
    fn test_snapshot_independence() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        let data = vec![1.0f32; 128];
        queue.put(&data);
        
        let snap = queue.snapshot();
        
        // Modify queue
        queue.put(&[2.0f32; 128]);
        
        // Snapshot should be unchanged
        assert_eq!(snap.n_samples, 128);
        assert_eq!(snap.buffers.len(), 1);
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

    #[test]
    fn test_zero_samples_after_clear() {
        let queue = AudioBufferQueue::new(20, 5, 128, 10);
        
        assert_eq!(queue.n_samples(), 0);
    }

    #[test]
    fn test_empty_data() {
        let mut queue = AudioBufferQueue::new(20, 5, 128, 10);
        queue.put(&[]);
        
        assert_eq!(queue.n_samples(), 0);
    }
}