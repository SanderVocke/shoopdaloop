//! AudioPort - Audio port implementation in Rust.
//!
//! Provides audio port functionality with:
//! - Atomic peak meters (input/output)
//! - Atomic gain control
//! - Atomic mute state
//! - Ringbuffer (AudioBufferQueue) for retroactive recording
//!
//! The actual audio buffer is managed by C++ code. This Rust implementation
//! receives buffer pointers via FFI for processing (peak tracking, gain/mute).
//!
//! Ported from C++ AudioPort.h/cpp

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::audio_buffer_queue::AudioBufferQueue;

/// Wrapper for atomic f32 operations using AtomicU32 internally
#[derive(Debug)]
pub struct AtomicF32 {
    inner: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        AtomicF32 {
            inner: AtomicU32::new(val.to_bits()),
        }
    }

    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.inner.load(order))
    }

    pub fn store(&self, val: f32, order: Ordering) {
        self.inner.store(val.to_bits(), order);
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// AudioPort - Audio port implementation with peak metering and ringbuffer.
///
/// This mirrors the C++ AudioPort<SampleT> template, primarily used with float samples.
/// Thread-safe via atomic operations.
///
/// NOTE: The actual audio buffer is managed by C++ code. This Rust implementation
/// receives buffer pointers via FFI for processing (peak tracking, gain/mute application).
pub struct AudioPort {
    /// Ringbuffer for retroactive recording
    ringbuffer: AudioBufferQueue,

    /// Input peak meter
    input_peak: AtomicF32,

    /// Output peak meter
    output_peak: AtomicF32,

    /// Gain factor (applied to audio)
    gain: AtomicF32,

    /// Mute state
    muted: AtomicBool,

    /// Single buffer size from ringbuffer (cached for efficiency)
    single_buffer_size: usize,
}

impl AudioPort {
    /// Create a new AudioPort with a ringbuffer.
    ///
    /// # Arguments
    /// * `pool_capacity` - Maximum number of buffers in the pool
    /// * `low_water_mark` - Minimum buffers to keep in pool
    /// * `buffer_size` - Size of each buffer in samples
    /// * `max_buffers` - Maximum number of buffers in ringbuffer
    pub fn new(
        pool_capacity: usize,
        low_water_mark: usize,
        buffer_size: usize,
        max_buffers: u32,
    ) -> Self {
        let ringbuffer =
            AudioBufferQueue::new(pool_capacity, low_water_mark, buffer_size, max_buffers);
        AudioPort {
            ringbuffer,
            input_peak: AtomicF32::new(0.0),
            output_peak: AtomicF32::new(0.0),
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            single_buffer_size: buffer_size,
        }
    }

    /// Process audio: applies gain/mute, tracks peaks, records to ringbuffer.
    ///
    /// This processes the buffer at the given pointer. The buffer is managed by C++ code.
    ///
    /// # Safety
    /// - `buffer` must point to valid memory with at least `n_frames` elements
    /// - The memory must be mutable as this function modifies the buffer in place
    pub unsafe fn process_raw(&mut self, buffer: *mut f32, n_frames: u32) {
        if n_frames == 0 || buffer.is_null() {
            return;
        }

        let buf = unsafe { std::slice::from_raw_parts_mut(buffer, n_frames as usize) };

        let muted = self.muted.load(Ordering::SeqCst);
        let gain = self.gain.load(Ordering::SeqCst);

        // Calculate input peak and apply gain/mute
        let mut input_peak_val: f32 = self.input_peak.load(Ordering::SeqCst);

        for sample in buf.iter_mut() {
            let abs_val = sample.abs();
            if abs_val > input_peak_val {
                input_peak_val = abs_val;
            }

            if muted {
                *sample = 0.0;
            } else {
                *sample *= gain;
            }
        }

        self.input_peak.store(input_peak_val, Ordering::SeqCst);

        // Calculate output peak
        let output_peak_val = if muted { 0.0 } else { input_peak_val * gain };

        let prev_output = self.output_peak.load(Ordering::SeqCst);
        self.output_peak
            .store(output_peak_val.max(prev_output), Ordering::SeqCst);

        // Record to ringbuffer if properly configured
        // Note: The ringbuffer will handle partial buffers internally
        if self.single_buffer_size > 0 && n_frames > 0 {
            // Copy the data to pass to ringbuffer - slice of first n_frames elements
            let ringbuf_slice = &buf[..n_frames as usize];
            self.ringbuffer.put(ringbuf_slice);
        }
    }

    /// Set the gain factor.
    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain, Ordering::SeqCst);
    }

    /// Get the current gain factor.
    pub fn get_gain(&self) -> f32 {
        self.gain.load(Ordering::SeqCst)
    }

    /// Set the mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }

    /// Get the mute state.
    pub fn get_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    /// Get the input peak value.
    pub fn get_input_peak(&self) -> f32 {
        self.input_peak.load(Ordering::SeqCst)
    }

    /// Reset the input peak to zero.
    pub fn reset_input_peak(&self) {
        self.input_peak.store(0.0, Ordering::SeqCst);
    }

    /// Get the output peak value.
    pub fn get_output_peak(&self) -> f32 {
        self.output_peak.load(Ordering::SeqCst)
    }

    /// Reset the output peak to zero.
    pub fn reset_output_peak(&self) {
        self.output_peak.store(0.0, Ordering::SeqCst);
    }

    /// Set the minimum number of samples in the ringbuffer.
    pub fn set_ringbuffer_n_samples(&mut self, n: u32) {
        self.ringbuffer.set_min_n_samples(n);
    }

    /// Get the current number of samples in the ringbuffer.
    pub fn get_ringbuffer_n_samples(&self) -> u32 {
        self.ringbuffer.n_samples()
    }

    /// Get the single buffer size of the ringbuffer.
    pub fn get_single_buffer_size(&self) -> u32 {
        self.single_buffer_size as u32
    }

    /// Get the number of buffers in the ringbuffer.
    pub fn get_ringbuffer_n_buffers(&self) -> usize {
        self.ringbuffer.n_buffers()
    }

    /// Get the ringbuffer contents as a vector of buffer pointers.
    /// Each entry contains a pointer to the data and the length.
    pub fn get_ringbuffer_contents_ptrs(&self) -> Vec<(*const f32, usize)> {
        let mut result = Vec::new();
        for buf in self.ringbuffer.iter_buffers() {
            if buf.len() > 0 {
                result.push((buf.data_ptr(), buf.len()));
            }
        }
        result
    }
}

// Buffer pointer info for FFI
#[repr(C)]
pub struct BufferPtrInfo {
    pub data_ptr: *const f32,
    pub len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_port_creation() {
        let port = AudioPort::new(20, 10, 128, 8);
        assert!(!port.get_muted());
        assert_eq!(port.get_gain(), 1.0);
        assert_eq!(port.get_input_peak(), 0.0);
        assert_eq!(port.get_output_peak(), 0.0);
    }

    #[test]
    fn test_gain_control() {
        let port = AudioPort::new(20, 10, 128, 8);

        assert_eq!(port.get_gain(), 1.0);
        port.set_gain(0.5);
        assert_eq!(port.get_gain(), 0.5);
    }

    #[test]
    fn test_mute_control() {
        let port = AudioPort::new(20, 10, 128, 8);

        assert!(!port.get_muted());
        port.set_muted(true);
        assert!(port.get_muted());
        port.set_muted(false);
        assert!(!port.get_muted());
    }

    #[test]
    fn test_process_audio() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        // Create a buffer with test data
        let n_frames = 128u32;
        let mut buffer: Vec<f32> = (0..128).map(|i| (i as f32 * 0.1).sin()).collect();

        // Process the buffer
        unsafe {
            port.process_raw(buffer.as_mut_ptr(), n_frames);
        }

        // Check that peaks were tracked
        assert!(port.get_input_peak() > 0.0);
        assert!(port.get_output_peak() > 0.0);
    }

    #[test]
    fn test_mute_zeroes_output() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        let mut buffer: Vec<f32> = vec![0.5; n_frames as usize];

        port.set_muted(true);
        unsafe {
            port.process_raw(buffer.as_mut_ptr(), n_frames);
        }

        // After processing with mute, buffer should be zeroed
        for &sample in buffer.iter() {
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn test_gain_applies_to_buffer() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        let mut buffer: Vec<f32> = (0..128).map(|i| i as f32).collect();
        let original_sum: f32 = buffer.iter().sum();

        port.set_gain(0.5);
        unsafe {
            port.process_raw(buffer.as_mut_ptr(), n_frames);
        }

        // After gain, sum should be halved
        let new_sum: f32 = buffer.iter().sum();
        assert!((new_sum - original_sum * 0.5).abs() < 0.001);
    }

    #[test]
    fn test_reset_peaks() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        let mut buffer: Vec<f32> = vec![0.9; n_frames as usize];

        unsafe {
            port.process_raw(buffer.as_mut_ptr(), n_frames);
        }
        assert!(port.get_input_peak() > 0.0);

        port.reset_input_peak();
        assert_eq!(port.get_input_peak(), 0.0);

        port.reset_output_peak();
        assert_eq!(port.get_output_peak(), 0.0);
    }
}
