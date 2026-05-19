//! AudioPort - Audio port implementation in Rust.
//!
//! Provides audio port functionality with:
//! - Atomic peak meters (input/output)
//! - Atomic gain control
//! - Atomic mute state
//! - Ringbuffer (AudioBufferQueue) for retroactive recording
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

    /// Working buffer for PROC_get_buffer
    /// This is the buffer that samples are written to/read from
    buffer: Vec<f32>,

    /// Current buffer size (number of frames)
    buffer_size: u32,
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
        AudioPort {
            ringbuffer: AudioBufferQueue::new(
                pool_capacity,
                low_water_mark,
                buffer_size,
                max_buffers,
            ),
            input_peak: AtomicF32::new(0.0),
            output_peak: AtomicF32::new(0.0),
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            buffer: vec![0.0f32; buffer_size],
            buffer_size: buffer_size as u32,
        }
    }

    /// Get the internal buffer for audio processing.
    /// Returns a mutable slice of the specified size.
    pub fn get_buffer(&mut self, n_frames: u32) -> *mut f32 {
        // Resize buffer if needed
        if n_frames as usize > self.buffer.len() {
            self.buffer.resize(n_frames as usize, 0.0);
        }
        self.buffer_size = n_frames;
        self.buffer.as_mut_ptr()
    }

    /// Get a slice view of the current buffer.
    pub fn get_buffer_slice(&self) -> &[f32] {
        &self.buffer[..self.buffer_size as usize]
    }

    /// Get a mutable slice view of the current buffer.
    pub fn get_buffer_slice_mut(&mut self) -> &mut [f32] {
        &mut self.buffer[..self.buffer_size as usize]
    }

    /// Process audio: applies gain/mute, tracks peaks, records to ringbuffer.
    ///
    /// This should be called after `get_buffer()` has been used to fill the buffer.
    pub fn process(&mut self, n_frames: u32) {
        let buf = &mut self.buffer[..n_frames as usize];
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

        // Record to ringbuffer
        if self.ringbuffer.single_buffer_size() > 0 {
            self.ringbuffer.put(buf);
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
        self.ringbuffer.single_buffer_size()
    }

    /// Get the number of buffers in the ringbuffer.
    pub fn get_ringbuffer_n_buffers(&self) -> usize {
        self.ringbuffer.n_buffers()
    }

    /// Get the ringbuffer contents as a vector of buffer pointers.
    /// Each entry contains a pointer to the data and the length.
    pub fn get_ringbuffer_contents_ptrs(&self) -> Vec<(*const f32, usize)> {
        self.ringbuffer
            .iter_buffers()
            .filter(|buf| buf.len() > 0)
            .map(|buf| (buf.data_ptr(), buf.len()))
            .collect()
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

        // Fill buffer with a sine wave-like data
        let n_frames = 128u32;
        {
            let buf = unsafe {
                std::slice::from_raw_parts_mut(port.get_buffer(n_frames), n_frames as usize)
            };
            for (i, sample) in buf.iter_mut().enumerate() {
                *sample = (i as f32 * 0.1).sin();
            }
        }

        port.process(n_frames);

        // Check that peaks were tracked
        assert!(port.get_input_peak() > 0.0);
        assert!(port.get_output_peak() > 0.0);
    }

    #[test]
    fn test_mute_zeroes_output() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        {
            let buf = unsafe {
                std::slice::from_raw_parts_mut(port.get_buffer(n_frames), n_frames as usize)
            };
            for sample in buf.iter_mut() {
                *sample = 0.5;
            }
        }

        port.set_muted(true);
        port.process(n_frames);

        // After processing with mute, buffer should be zeroed
        let buf_slice = port.get_buffer_slice();
        for &sample in buf_slice.iter() {
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn test_reset_peaks() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        {
            let buf = unsafe {
                std::slice::from_raw_parts_mut(port.get_buffer(n_frames), n_frames as usize)
            };
            for sample in buf.iter_mut() {
                *sample = 0.9;
            }
        }

        port.process(n_frames);
        assert!(port.get_input_peak() > 0.0);

        port.reset_input_peak();
        assert_eq!(port.get_input_peak(), 0.0);

        port.reset_output_peak();
        assert_eq!(port.get_output_peak(), 0.0);
    }

    #[test]
    fn test_ringbuffer_integration() {
        let mut port = AudioPort::new(20, 10, 128, 8);

        let n_frames = 128u32;
        {
            let buf = unsafe {
                std::slice::from_raw_parts_mut(port.get_buffer(n_frames), n_frames as usize)
            };
            for sample in buf.iter_mut() {
                *sample = 0.5;
            }
        }

        port.process(n_frames);

        // Ringbuffer should have recorded the audio
        let n_samples = port.get_ringbuffer_n_samples();
        assert_eq!(n_samples, 128);
    }
}
