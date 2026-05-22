//! Audio buffer queue implementation for audio processing.
//!
//! Ported from C++ BufferQueue.
//! Uses reference-counted buffers (Arc<SharedBufferHandle>) to avoid
//! deep copies when creating snapshots, matching C++ shared_ptr behavior.

use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::sync::Weak;

use crate::refilling_pool::refilling_pool::RefillingPool;
use crate::refilling_pool::refilling_pool_cxx::BufferHandle;

/// A reference-counted buffer handle.
///
/// Wraps a Box<BufferHandle> from the pool. When the last Arc reference
/// is dropped, the buffer is returned to the pool for reuse.
///
/// This mirrors the C++ pattern where AudioBuffer has a weak reference
/// back to the BufferPool and returns itself in its destructor.
pub struct SharedBufferHandle {
    buffer: UnsafeCell<Option<Box<BufferHandle>>>,
    pool: Weak<RefillingPool<BufferHandle>>,
}

impl SharedBufferHandle {
    /// Create a shared handle from a Box obtained from the pool.
    fn new(buffer: Box<BufferHandle>, pool: &std::sync::Arc<RefillingPool<BufferHandle>>) -> Self {
        Self {
            buffer: UnsafeCell::new(Some(buffer)),
            pool: std::sync::Arc::downgrade(pool),
        }
    }

    /// Get mutable access to the underlying data as f32 slice.
    /// SAFETY: This is safe as long as there are no concurrent mutable accesses.
    /// The AudioBufferQueue ensures exclusive access to the active buffer.
    pub fn as_f32_mut(&self) -> &mut [f32] {
        let buf_opt = unsafe { &mut *self.buffer.get() };
        let buf = buf_opt.as_mut().expect("buffer was already released");
        let bytes = buf.as_bytes_mut();
        let len = bytes.len() / std::mem::size_of::<f32>();
        let ptr = bytes.as_mut_ptr() as *mut f32;
        unsafe { std::slice::from_raw_parts_mut(ptr, len) }
    }

    /// Get immutable access to the underlying data as f32 slice.
    pub fn as_f32(&self) -> &[f32] {
        let buf_opt = unsafe { &*self.buffer.get() };
        let buf = buf_opt.as_ref().expect("buffer was already released");
        let bytes = buf.as_bytes();
        let len = bytes.len() / std::mem::size_of::<f32>();
        let ptr = bytes.as_ptr() as *const f32;
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }

    /// Get raw pointer to data.
    pub fn data_ptr(&self) -> *const f32 {
        let buf_opt = unsafe { &*self.buffer.get() };
        let buf = buf_opt.as_ref().expect("buffer was already released");
        buf.as_bytes().as_ptr() as *const f32
    }

    /// Get total capacity in samples.
    pub fn capacity(&self) -> usize {
        let buf_opt = unsafe { &*self.buffer.get() };
        let buf = buf_opt.as_ref().expect("buffer was already released");
        buf.len() / std::mem::size_of::<f32>()
    }
}

impl Drop for SharedBufferHandle {
    fn drop(&mut self) {
        // When the last Arc is dropped, return the buffer to the pool
        let buf_opt = unsafe { &mut *self.buffer.get() };
        if let Some(buf) = buf_opt.take() {
            if let Some(pool) = self.pool.upgrade() {
                let pool: &RefillingPool<BufferHandle> = &pool;
                pool.release(buf);
            }
            // If pool is gone, buf is dropped and memory is freed
        }
    }
}

/// Wrapper around audio buffer data for the queue.
/// Holds an Arc to a SharedBufferHandle for cheap cloning.
#[derive(Clone)]
pub struct AudioBufferData {
    /// Number of valid samples in this buffer
    pub n_samples: usize,
    /// Reference-counted buffer handle
    buffer: std::sync::Arc<SharedBufferHandle>,
}

impl AudioBufferData {
    pub fn len(&self) -> usize {
        self.n_samples
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.buffer.as_f32()[..self.n_samples]
    }

    pub fn data_ptr(&self) -> *const f32 {
        self.buffer.data_ptr()
    }

    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    pub fn as_f32_mut(&self) -> &mut [f32] {
        self.buffer.as_f32_mut()
    }

    /// Get sample at index (for testing).
    pub fn at(&self, index: usize) -> f32 {
        self.as_slice()[index]
    }
}

impl std::fmt::Debug for AudioBufferData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioBufferData")
            .field("n_samples", &self.n_samples)
            .field("capacity", &self.capacity())
            .finish()
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
    /// All buffers (completed + active) - reference-counted for cheap snapshots
    buffers: VecDeque<AudioBufferData>,
    /// Buffer pool for pre-allocated buffer reuse
    pool: std::sync::Arc<RefillingPool<BufferHandle>>,
    /// Capacity of each buffer in samples
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
        let factory = move || {
            let bytes = vec![0u8; buffer_size * std::mem::size_of::<f32>()];
            Box::new(BufferHandle(bytes))
        };
        let pool = RefillingPool::new(pool_capacity, low_water_mark, factory)
            .expect("Failed to create buffer pool");
        let pool = std::sync::Arc::new(pool);

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
        while let Some(_buf) = self.buffers.pop_front() {
            // Arc dropped here, SharedBufferHandle::Drop returns to pool
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
                // Active buffer is full - mark it as full
                if let Some(active) = self.buffers.back_mut() {
                    active.n_samples = buffer_size;
                }
                // Create new buffer from pool
                let boxed = self.pool.get();
                let shared = SharedBufferHandle::new(boxed, &self.pool);
                let arc = std::sync::Arc::new(shared);
                let new_buf = AudioBufferData {
                    n_samples: 0,
                    buffer: arc,
                };
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
                let dest = active.as_f32_mut();
                let dest_start = self.active_pos as usize;
                dest[dest_start..dest_start + to_copy].copy_from_slice(&remaining[..to_copy]);
                // Update n_samples to reflect valid data count
                active.n_samples = (self.active_pos as usize) + to_copy;
            }
            self.active_pos += to_copy as u32;
            remaining = &remaining[to_copy..];
        }
    }

    fn evict_front(&mut self) {
        // Pop front - Arc dropped, SharedBufferHandle::Drop returns to pool
        if let Some(_buf) = self.buffers.pop_front() {
            // Dropping the Arc here triggers SharedBufferHandle::Drop
            // which returns the Box<BufferHandle> to the pool
        }
    }

    /// Get snapshot. Mirrors C++ PROC_get exactly.
    /// Returns vector of Arc-cloned buffers (cheap, no data copy).
    pub fn snapshot(&self) -> Snapshot {
        let mut result = Snapshot {
            data: Vec::with_capacity(self.buffers.len()),
            n_samples: self.n_samples(),
            buffer_size: self.buffer_size,
        };

        for buf in &self.buffers {
            // Clone the Arc - cheap, just increments refcount
            // Both snapshot and queue now share the same underlying buffer
            result.data.push(buf.clone());
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
        while let Some(_buf) = self.buffers.pop_front() {
            // Arc dropped, SharedBufferHandle::Drop returns to pool
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

    /// Test that snapshot shares data with queue (no deep copy).
    /// Both should see the same underlying buffer.
    #[test]
    fn test_snapshot_shares_data() {
        let mut queue = AudioBufferQueue::new(10, 5, 10, 10);
        queue.put(&[1.0, 2.0, 3.0]);

        let snap = queue.snapshot();

        // Snapshots share the same Arc - verify by checking data_ptr
        assert_eq!(snap.data[0].data_ptr(), queue.buffers[0].data_ptr());
    }

    /// Test that evicted buffers are returned to pool (pool count recovers).
    #[test]
    fn test_eviction_returns_to_pool() {
        let mut queue = AudioBufferQueue::new(2, 1, 2, 2);
        queue.put(&[1.0, 2.0, 3.0, 4.0]);

        // 2 buffers in queue, pool should be empty (capacity 2, both taken)
        assert_eq!(queue.pool.n_buffers_available(), 0);

        // Add more data to trigger eviction
        queue.put(&[5.0, 6.0]);
        // First buffer evicted and returned to pool
        assert_eq!(queue.pool.n_buffers_available(), 1);
    }
}
