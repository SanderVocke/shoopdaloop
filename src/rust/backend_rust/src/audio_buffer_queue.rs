//! Audio buffer queue implementation for audio processing.
//! 
//! This provides a FIFO queue of audio buffers with automatic memory management.
//! Ported from C++ BufferQueue.

use std::collections::VecDeque;

/// Wrapper around buffer data for audio samples
#[derive(Debug, Clone)]
pub struct AudioBufferData<T> {
    /// The audio sample data
    pub data: Vec<T>,
}

impl<T: Default + Copy + Clone> AudioBufferData<T> {
    /// Create a new buffer with the given size, initialized to T::default()
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![T::default(); size],
        }
    }

    /// Get the number of samples in this buffer
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Immutable snapshot of a buffer queue state.
/// This is created by taking a snapshot of the queue and can be safely read
/// without locking, since it's an owned copy.
#[derive(Debug, Clone)]
pub struct AudioBufferQueueSnapshot<T: Clone> {
    /// Total number of samples across all buffers
    pub n_samples: u32,
    /// Size of each full buffer
    pub buffer_size: u32,
    /// The buffer data
    pub buffers: Vec<AudioBufferData<T>>,
}

impl<T: Clone> AudioBufferQueueSnapshot<T> {
    /// Get the number of buffers in this snapshot
    pub fn n_buffers(&self) -> usize {
        self.buffers.len()
    }

    /// Get buffer at index
    pub fn get_buffer(&self, idx: usize) -> Option<&[T]> {
        self.buffers.get(idx).map(|b| b.data.as_slice())
    }
}

/// FIFO queue for audio buffers with automatic eviction.
/// 
/// When the queue exceeds `max_buffers`, the oldest buffer is automatically evicted.
/// This ensures bounded memory usage while maintaining a rolling window of audio data.
#[derive(Debug)]
pub struct AudioBufferQueue<T: Default + Copy + Clone> {
    /// Internal storage for buffers
    buffers: VecDeque<AudioBufferData<T>>,
    /// Capacity of each buffer
    buffer_size: u32,
    /// Maximum number of buffers to store
    max_buffers: u32,
    /// Current position within the active buffer
    active_buffer_pos: u32,
    /// Whether we have an active buffer started
    has_active_buffer: bool,
    /// Minimum number of samples to keep in the queue
    min_n_samples: u32,
}

impl<T: Default + Copy + Clone> AudioBufferQueue<T> {
    /// Create a new AudioBufferQueue.
    /// 
    /// - `max_buffers`: Maximum number of buffers to store (older ones are evicted)
    /// - `buffer_size`: Size of each buffer in samples
    pub fn new(max_buffers: u32, buffer_size: u32) -> Self {
        Self {
            buffers: VecDeque::with_capacity(max_buffers as usize),
            buffer_size,
            max_buffers,
            active_buffer_pos: 0,
            has_active_buffer: false,
            min_n_samples: 0,
        }
    }

    /// Get the number of samples currently in the queue
    pub fn n_samples(&self) -> u32 {
        if self.buffers.is_empty() {
            return 0;
        }
        // (completed buffers) * buffer_size + current position
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
        // Evict if necessary
        while self.buffers.len() > max as usize {
            self.buffers.pop_front();
        }
    }

    /// Set minimum number of samples to keep
    pub fn set_min_n_samples(&mut self, n: u32) {
        self.min_n_samples = n;
    }

    /// Get the number of buffers
    pub fn n_buffers(&self) -> usize {
        if self.has_active_buffer {
            self.buffers.len() + 1
        } else {
            self.buffers.len()
        }
    }

    /// Add samples to the queue
    pub fn put(&mut self, data: &[T]) {
        let mut remaining = data;
        
        while !remaining.is_empty() {
            // Get current buffer to fill, or create a new one
            let buf = if let Some(buf) = self.buffers.back_mut() {
                if (buf.data.len() as u32) < self.buffer_size {
                    buf
                } else {
                    // Current buffer is full, start a new one
                    self.start_new_buffer();
                    self.buffers.back_mut().unwrap()
                }
            } else {
                // No buffers, start the first one
                self.start_new_buffer();
                self.buffers.back_mut().unwrap()
            };

            // Calculate how much space is left in this buffer
            let space_in_buf = (self.buffer_size as usize).saturating_sub(buf.data.len());
            let to_copy = remaining.len().min(space_in_buf);
            
            // Extend buffer with data
            buf.data.extend_from_slice(&remaining[..to_copy]);
            remaining = &remaining[to_copy..];
            
            // Update position
            self.active_buffer_pos += to_copy as u32;
            
            // If buffer is now full, start a new one
            if (buf.data.len() as u32) >= self.buffer_size && !remaining.is_empty() {
                self.start_new_buffer();
            }
        }
    }

    /// Start a new buffer and evict old ones if necessary
    fn start_new_buffer(&mut self) {
        // Evict old buffers first if we're at capacity
        while self.buffers.len() >= self.max_buffers as usize {
            self.buffers.pop_front();
        }
        
        // Add new empty buffer
        self.buffers.push_back(AudioBufferData::new(self.buffer_size as usize));
        self.active_buffer_pos = 0;
        self.has_active_buffer = true;
    }

    /// Take a snapshot of the current queue state.
    /// Returns an immutable copy that can be safely read.
    pub fn snapshot(&self) -> AudioBufferQueueSnapshot<T> {
        let n_complete_buffers = if self.has_active_buffer {
            self.buffers.len().saturating_sub(1)
        } else {
            self.buffers.len()
        };
        
        let total_samples = self.n_samples();
        
        let mut buffers = Vec::with_capacity(self.buffers.len());
        
        for (i, buf) in self.buffers.iter().enumerate() {
            if i == self.buffers.len() - 1 && self.has_active_buffer {
                // Last buffer - only include filled portion
                let filled_len = self.active_buffer_pos as usize;
                let mut truncated = buf.clone();
                truncated.data.truncate(filled_len);
                buffers.push(truncated);
            } else {
                buffers.push(buf.clone());
            }
        }
        
        AudioBufferQueueSnapshot {
            n_samples: total_samples,
            buffer_size: self.buffer_size,
            buffers,
        }
    }

    /// Clear all buffers
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.active_buffer_pos = 0;
        self.has_active_buffer = false;
    }
}

// Type aliases for common sample types
pub type AudioBufferQueueF32 = AudioBufferQueue<f32>;
pub type AudioBufferQueueSnapshotF32 = AudioBufferQueueSnapshot<f32>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_queue_is_empty() {
        let queue: AudioBufferQueueF32 = AudioBufferQueueF32::new(10, 128);
        assert_eq!(queue.n_samples(), 0);
        assert_eq!(queue.n_buffers(), 0);
    }

    #[test]
    fn test_single_buffer() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        let data = vec![1.0f32; 128];
        queue.put(&data);
        
        assert_eq!(queue.n_samples(), 128);
        assert_eq!(queue.n_buffers(), 1);
    }

    #[test]
    fn test_partial_buffer() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        let data = vec![1.0f32; 64];
        queue.put(&data);
        
        assert_eq!(queue.n_samples(), 64);
        assert_eq!(queue.n_buffers(), 1);
    }

    #[test]
    fn test_multiple_buffers() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        
        // Add 3 full buffers worth of data
        let data = vec![1.0f32; 300];
        queue.put(&data);
        
        assert_eq!(queue.n_samples(), 300);
        // Should have 3 buffers: 2 full (256) + 1 partial (44)
        assert_eq!(queue.n_buffers(), 3);
    }

    #[test]
    fn test_eviction() {
        let mut queue = AudioBufferQueueF32::new(3, 128);
        
        // Fill with 4 buffers worth of data
        let data = vec![1.0f32; 512];
        queue.put(&data);
        
        // Should only have 3 buffers max
        assert!(queue.n_buffers() <= 4);
        // 512 samples / 128 buffer_size = 4 buffers worth
        assert_eq!(queue.n_samples(), 512);
    }

    #[test]
    fn test_snapshot() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        let data = vec![1.0f32; 256];
        queue.put(&data);
        
        let snap = queue.snapshot();
        
        assert_eq!(snap.n_samples, 256);
        assert_eq!(snap.buffer_size, 128);
        assert_eq!(snap.buffers.len(), 2);
    }

    #[test]
    fn test_snapshot_independence() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
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
        let mut queue = AudioBufferQueueF32::new(10, 128);
        let data = vec![1.0f32; 256];
        queue.put(&data);
        
        queue.clear();
        
        assert_eq!(queue.n_samples(), 0);
        assert_eq!(queue.n_buffers(), 0);
    }

    #[test]
    fn test_set_max_buffers() {
        let mut queue = AudioBufferQueueF32::new(5, 128);
        let data = vec![1.0f32; 512];
        queue.put(&data);
        
        assert!(queue.n_buffers() <= 6);
        
        // Reduce max buffers
        queue.set_max_buffers(2);
        
        assert!(queue.n_buffers() <= 3);
    }

    #[test]
    fn test_zero_samples_after_clear() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        
        // Clear without adding anything
        queue.clear();
        
        assert_eq!(queue.n_samples(), 0);
    }

    #[test]
    fn test_empty_data() {
        let mut queue = AudioBufferQueueF32::new(10, 128);
        queue.put(&[]);
        
        assert_eq!(queue.n_samples(), 0);
    }
}