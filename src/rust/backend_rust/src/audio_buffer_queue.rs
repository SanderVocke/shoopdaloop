//! Audio buffer queue implementation for audio processing.
//!
//! Ported from C++ BufferQueue.
//! See: src/backend/internal/BufferQueue.cpp (master branch) for exact semantics.
//!
//! Key behaviors from C++:
//! - buffers is a deque of shared buffers
//! - active buffer position tracked separately
//! - n_samples = (buffers.size() - 1) * buffer_size + active_pos
//! - PROC_get returns a snapshot (vector copy) of all buffers

use std::collections::VecDeque;

use crate::refilling_pool::refilling_pool::RefillingPool;

/// A buffer wrapper that wraps a pool-allocated Vec
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct AudioBufferData {
    /// Number of valid samples in this buffer
    pub n_samples: usize,
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

    pub fn len(&self) -> usize {
        self.n_samples
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.pooled.data[..self.n_samples]
    }

    pub fn data_ptr(&self) -> *const f32 {
        self.pooled.data.as_ptr()
    }

    /// Get sample at index (for testing)
    pub fn at(&self, index: usize) -> f32 {
        self.pooled.data[index]
    }
}

/// Snapshot of the buffer queue contents.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub data: Vec<AudioBufferData>,
    pub n_samples: u32,
    pub buffer_size: u32,
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            data: Vec::new(),
            n_samples: 0,
            buffer_size: 0,
        }
    }
}

/// FIFO queue for audio buffers with automatic eviction.
/// Mirrors C++ BufferQueue semantics exactly.
pub struct AudioBufferQueue {
    /// All buffers (completed + active)
    buffers: VecDeque<AudioBufferData>,
    /// Buffer pool for pre-allocated buffer reuse
    pool: RefillingPool<PooledBuffer>,
    /// Capacity of each buffer
    buffer_size: u32,
    /// Maximum number of buffers allowed
    max_buffers: u32,
    /// Position in the active buffer (how many samples written to it)
    active_pos: u32,
}

impl AudioBufferQueue {
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
            buffers: VecDeque::new(),
            pool,
            buffer_size: buffer_size as u32,
            max_buffers,
            active_pos: buffer_size as u32, // Start at buffer_size so first put creates buffer
        }
    }

    /// Number of samples in queue.
    /// C++: if (buffers->size() == 0) return 0;
    ///      return (buffers->size() - 1) * buffer_size + active_pos;
    pub fn n_samples(&self) -> u32 {
        if self.buffers.is_empty() {
            return 0;
        }
        ((self.buffers.len() - 1) as u32) * self.buffer_size + self.active_pos
    }

    pub fn single_buffer_size(&self) -> u32 {
        self.buffer_size
    }

    pub fn get_max_buffers(&self) -> u32 {
        self.max_buffers
    }

    pub fn set_max_buffers(&mut self, max: u32) {
        // Create new empty deque and return all buffers to pool (like C++ queued command)
        while let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
        self.max_buffers = max;
        self.active_pos = self.buffer_size; // Reset to buffer_size (like C++)
    }

    pub fn set_min_n_samples(&mut self, n: u32) {
        let bufs = (n + self.buffer_size - 1) / std::cmp::max(1, self.buffer_size);
        self.set_max_buffers(bufs);
    }

    pub fn n_buffers(&self) -> usize {
        self.buffers.len()
    }

    pub fn iter_buffers(&self) -> impl Iterator<Item = &AudioBufferData> {
        self.buffers.iter()
    }

    /// Add samples to queue. Mirrors C++ PROC_put exactly.
    pub fn put(&mut self, data: &[f32]) {
        let mut remaining = data;
        let buffer_size = self.buffer_size as usize;

        while !remaining.is_empty() {
            let space = buffer_size - self.active_pos as usize;

            if space == 0 {
                // Space is 0, mark active buffer as full
                if let Some(active) = self.buffers.back_mut() {
                    active.n_samples = buffer_size;
                }
                // Create new buffer
                let new_buf = self.get_pooled_buffer();
                self.buffers.push_back(new_buf);
                self.active_pos = 0;
                // Evict while over limit
                while self.buffers.len() > self.max_buffers as usize {
                    self.evict_front();
                }
                continue;
            }

            // Copy data to active buffer
            let to_copy = remaining.len().min(space);
            {
                let active = self.buffers.back_mut().unwrap();
                let start = self.active_pos as usize;
                // Copy data into the pooled buffer
                for i in 0..to_copy {
                    active.pooled.data[start + i] = remaining[i];
                }
            }
            self.active_pos += to_copy as u32;
            remaining = &remaining[to_copy..];
        }
    }

    fn get_pooled_buffer(&mut self) -> AudioBufferData {
        let boxed = self.pool.get();
        AudioBufferData {
            n_samples: 0,
            pooled: *boxed,
        }
    }

    fn return_to_pool(&mut self, buf: PooledBuffer) {
        self.pool.release(Box::new(buf));
    }

    fn evict_front(&mut self) {
        if let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
    }

    /// Get snapshot. Mirrors C++ PROC_get exactly.
    /// Returns vector copy of all buffers.
    pub fn snapshot(&self) -> Snapshot {
        let mut result = Snapshot {
            data: Vec::with_capacity(self.buffers.len()),
            n_samples: self.n_samples(),
            buffer_size: self.buffer_size,
        };

        for buf in &self.buffers {
            let mut cloned = AudioBufferData::new(self.buffer_size as usize);
            cloned.n_samples = buf.n_samples;
            // Copy all capacity worth of data (C++ buffer has full capacity)
            for i in 0..self.buffer_size as usize {
                cloned.pooled.data[i] = buf.pooled.data[i];
            }
            result.data.push(cloned);
        }

        result
    }

    pub fn process(&mut self) {
        // C++ PROC_process only handles command queue, no-op in Rust
    }
}

pub type AudioBufferQueueF32 = AudioBufferQueue;

impl Drop for AudioBufferQueue {
    fn drop(&mut self) {
        while let Some(buf) = self.buffers.pop_front() {
            self.return_to_pool(buf.pooled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.0001
    }

    // Test starting state - verify C++ semantics
    #[test]
    fn test_starting_state() {
        let queue = AudioBufferQueue::new(10, 5, 10, 10);
        assert_eq!(queue.n_samples(), 0); // C++: 0 when buffers empty
        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 0);
        assert_eq!(snap.data.len(), 0); // C++: empty
    }

    #[test]
    fn test_single_buf_full() {
        let mut queue = AudioBufferQueue::new(10, 5, 10, 10);
        queue.put(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);

        assert_eq!(queue.n_samples(), 10);
        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 10);
        assert_eq!(snap.data.len(), 1);
        assert!(approx_eq(snap.data[0].at(0), 1.0));
        assert!(approx_eq(snap.data[0].at(9), 10.0));
    }

    #[test]
    fn test_single_buf_partial() {
        let mut queue = AudioBufferQueue::new(10, 5, 10, 10);
        queue.put(&[1.0, 2.0, 3.0]);

        assert_eq!(queue.n_samples(), 3);
        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 3);
        assert_eq!(snap.data.len(), 1);
        assert!(approx_eq(snap.data[0].at(0), 1.0));
        assert!(approx_eq(snap.data[0].at(2), 3.0));
    }

    #[test]
    fn test_two_bufs_full() {
        let mut queue = AudioBufferQueue::new(10, 5, 4, 4);
        queue.put(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);

        assert_eq!(queue.n_samples(), 8);
        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 8);
        assert_eq!(snap.data.len(), 2);
        assert!(approx_eq(snap.data[0].at(0), 1.0));
        assert!(approx_eq(snap.data[0].at(3), 4.0));
        assert!(approx_eq(snap.data[1].at(0), 5.0));
        assert!(approx_eq(snap.data[1].at(3), 8.0));
    }

    #[test]
    fn test_two_bufs_partial() {
        let mut queue = AudioBufferQueue::new(10, 5, 4, 4);
        queue.put(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        assert_eq!(queue.n_samples(), 6);
        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 6);
        assert_eq!(snap.data.len(), 2);
        assert!(approx_eq(snap.data[0].at(0), 1.0));
        assert!(approx_eq(snap.data[0].at(3), 4.0));
        assert!(approx_eq(snap.data[1].at(0), 5.0));
        assert!(approx_eq(snap.data[1].at(1), 6.0));
    }

    #[test]
    fn test_drop_buffer() {
        let mut queue = AudioBufferQueue::new(2, 1, 2, 2);
        queue.put(&[1.0, 2.0, 3.0, 4.0]);

        assert_eq!(queue.n_samples(), 4);
        let snap1 = queue.snapshot();
        assert_eq!(snap1.n_samples, 4);
        assert_eq!(snap1.data.len(), 2);
        assert!(approx_eq(snap1.data[0].at(0), 1.0));
        assert!(approx_eq(snap1.data[1].at(1), 4.0));

        queue.put(&[5.0, 6.0]);
        assert_eq!(queue.n_samples(), 4);
        let snap2 = queue.snapshot();
        assert_eq!(snap2.n_samples, 4);
        assert_eq!(snap2.data.len(), 2);
        assert!(approx_eq(snap2.data[0].at(0), 3.0));
        assert!(approx_eq(snap2.data[1].at(0), 5.0));
    }

    #[test]
    fn test_drop_buffer_then_change_max() {
        let mut queue = AudioBufferQueue::new(2, 1, 2, 2);
        queue.put(&[1.0, 2.0, 3.0, 4.0]);

        let snap1 = queue.snapshot();
        assert_eq!(snap1.n_samples, 4);
        assert_eq!(snap1.data.len(), 2);

        queue.put(&[5.0, 6.0]);
        let snap2 = queue.snapshot();
        assert_eq!(snap2.n_samples, 4);
        assert_eq!(snap2.data.len(), 2);
        assert!(approx_eq(snap2.data[0].at(0), 3.0));
        assert!(approx_eq(snap2.data[0].at(1), 4.0));
        assert!(approx_eq(snap2.data[1].at(0), 5.0));

        // set_max_buffers(1) clears buffers in C++ (via queued command, executed in process())
        queue.set_max_buffers(1);
        queue.process();
        // After process: buffers=[], active_pos=buffer_size

        queue.put(&[7.0, 8.0, 9.0, 10.0]);
        // After put: buffers=[new_buf], active_pos=2, n_samples=2

        let snap3 = queue.snapshot();
        // n_samples = (1-1)*2 + 2 = 2
        assert_eq!(snap3.n_samples, 2);
        assert_eq!(snap3.data.len(), 1);
        assert!(approx_eq(snap3.data[0].at(0), 9.0));
        assert!(approx_eq(snap3.data[0].at(1), 10.0));
    }

    #[test]
    fn test_clone_then_drop() {
        let mut queue = AudioBufferQueue::new(2, 1, 2, 2);
        queue.put(&[1.0, 2.0, 3.0, 4.0]);

        let clone = queue.snapshot();

        queue.put(&[5.0, 6.0]);

        let snap = queue.snapshot();
        assert_eq!(snap.n_samples, 4);
        assert_eq!(snap.data.len(), 2);
        assert!(approx_eq(snap.data[0].at(0), 3.0));
        assert!(approx_eq(snap.data[1].at(0), 5.0));

        assert_eq!(clone.n_samples, 4);
        assert!(approx_eq(clone.data[0].at(0), 1.0));
        assert!(approx_eq(clone.data[1].at(0), 3.0));
    }
}
