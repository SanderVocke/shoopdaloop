//! InternalAudioPort - Audio port with local buffer management.
//!
//! Ported from C++ InternalAudioPort.h/cpp
//!
//! The Rust core fully owns the local audio buffer, AudioPort functionality,
//! and metadata. This provides all port operations including peak/gain/mute
//! and ringbuffer support.

use crate::audio_port::AudioPort;

pub struct InternalAudioPort {
    /// Local audio buffer
    buffer: Vec<f32>,
    /// Port name
    name: String,
    /// Input connectability (flags)
    input_connectability: u32,
    /// Output connectability (flags)
    output_connectability: u32,
    /// Audio processing (gain/mute/peaks/ringbuffer)
    audio_port: AudioPort,
}

impl InternalAudioPort {
    /// Create a new InternalAudioPort with embedded AudioPort.
    pub fn new(
        name: &str,
        n_frames: u32,
        input_connectability: u32,
        output_connectability: u32,
    ) -> Self {
        // Ringbuffer settings - pool_capacity, low_water_mark, buffer_size, max_buffers
        let audio_port = AudioPort::new(20, 10, n_frames as usize, 8);
        InternalAudioPort {
            buffer: vec![0.0f32; n_frames as usize],
            name: name.to_string(),
            input_connectability,
            output_connectability,
            audio_port,
        }
    }

    /// Get the port name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Prepare the port for processing (zero the buffer).
    pub fn proc_prepare(&mut self, nframes: u32) {
        let nframes = nframes as usize;
        if self.buffer.len() < nframes {
            self.buffer.resize(nframes, 0.0f32);
        }
        for sample in &mut self.buffer[..nframes] {
            *sample = 0.0f32;
        }
    }

    /// Get pointer to the internal buffer for audio processing.
    /// Returns a raw pointer that C++ can use to access the buffer.
    pub fn proc_get_buffer(&mut self, nframes: u32) -> *mut f32 {
        let nframes = nframes as usize;
        if self.buffer.len() < nframes {
            self.buffer.resize(nframes, 0.0f32);
        }
        self.buffer.as_mut_ptr()
    }

    /// Process the buffer: apply gain/mute and track peaks.
    /// This should be called after audio data is written to the buffer.
    pub fn proc_process(&mut self, nframes: u32) {
        let buffer_ptr = self.proc_get_buffer(nframes);
        unsafe {
            self.audio_port.process_raw(buffer_ptr, nframes);
        }
    }

    /// Get the input connectability flags.
    pub fn input_connectability(&self) -> u32 {
        self.input_connectability
    }

    /// Get the output connectability flags.
    pub fn output_connectability(&self) -> u32 {
        self.output_connectability
    }

    /// Set the gain factor for this port.
    pub fn set_gain(&self, gain: f32) {
        self.audio_port.set_gain(gain);
    }

    /// Get the current gain factor.
    pub fn get_gain(&self) -> f32 {
        self.audio_port.get_gain()
    }

    /// Set the mute state for this port.
    pub fn set_muted(&self, muted: bool) {
        self.audio_port.set_muted(muted);
    }

    /// Get the current mute state.
    pub fn get_muted(&self) -> bool {
        self.audio_port.get_muted()
    }

    /// Get the input peak value (after processing).
    pub fn get_input_peak(&self) -> f32 {
        self.audio_port.get_input_peak()
    }

    /// Get the output peak value (after processing).
    pub fn get_output_peak(&self) -> f32 {
        self.audio_port.get_output_peak()
    }

    /// Reset the input peak to zero.
    pub fn reset_input_peak(&self) {
        self.audio_port.reset_input_peak();
    }

    /// Reset the output peak to zero.
    pub fn reset_output_peak(&self) {
        self.audio_port.reset_output_peak();
    }

    /// Set the minimum number of samples in the ringbuffer.
    pub fn set_ringbuffer_n_samples(&mut self, n: u32) {
        self.audio_port.set_ringbuffer_n_samples(n);
    }

    /// Get the current number of samples in the ringbuffer.
    pub fn get_ringbuffer_n_samples(&self) -> u32 {
        self.audio_port.get_ringbuffer_n_samples()
    }

    /// Get the number of buffers in the ringbuffer.
    pub fn get_ringbuffer_n_buffers(&self) -> usize {
        self.audio_port.get_ringbuffer_n_buffers()
    }

    /// Get the single buffer size of the ringbuffer.
    pub fn get_ringbuffer_single_buffer_size(&self) -> u32 {
        self.audio_port.get_single_buffer_size()
    }

    /// Get the ringbuffer contents as a vector of (pointer, length) tuples.
    pub fn get_ringbuffer_contents(&self) -> Vec<(*const f32, usize)> {
        self.audio_port.get_ringbuffer_contents_ptrs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let port = InternalAudioPort::new("test", 16, 1, 2);
        assert_eq!(port.name(), "test");
        assert_eq!(port.input_connectability(), 1);
        assert_eq!(port.output_connectability(), 2);
    }

    #[test]
    fn test_prepare_zero_fills() {
        let mut port = InternalAudioPort::new("test", 4, 0, 0);
        // Fill buffer with non-zero values
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        }
        port.proc_prepare(4);
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 4);
            assert_eq!(slice, &[0.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn test_buffer_pointer_non_null_after_prepare() {
        let mut port = InternalAudioPort::new("test", 8, 0, 0);
        port.proc_prepare(8);
        let buf = port.proc_get_buffer(8);
        assert!(!buf.is_null());
    }

    #[test]
    fn test_name_round_trip() {
        let port = InternalAudioPort::new("my_port", 10, 0, 0);
        assert_eq!(port.name(), "my_port");
    }

    #[test]
    fn test_gain_control() {
        let port = InternalAudioPort::new("test", 16, 0, 0);
        assert_eq!(port.get_gain(), 1.0); // default gain
        port.set_gain(0.5);
        assert_eq!(port.get_gain(), 0.5);
    }

    #[test]
    fn test_mute_control() {
        let port = InternalAudioPort::new("test", 16, 0, 0);
        assert!(!port.get_muted());
        port.set_muted(true);
        assert!(port.get_muted());
        port.set_muted(false);
        assert!(!port.get_muted());
    }

    #[test]
    fn test_process_applies_gain() {
        let mut port = InternalAudioPort::new("test", 4, 0, 0);

        // Get buffer and fill with known values
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        }

        port.set_gain(2.0);
        port.proc_process(4);

        // Check peaks were tracked (input and output should be same since gain=2)
        let input_peak = port.get_input_peak();
        let output_peak = port.get_output_peak();
        assert_eq!(input_peak, 4.0); // max of 1,2,3,4
        assert_eq!(output_peak, 8.0); // input_peak * gain
    }

    #[test]
    fn test_mute_zeroes_buffer() {
        let mut port = InternalAudioPort::new("test", 4, 0, 0);

        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        }

        port.set_muted(true);
        port.proc_process(4);

        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 4);
            for &sample in slice {
                assert_eq!(sample, 0.0);
            }
        }

        // Output peak should be 0 when muted
        assert_eq!(port.get_output_peak(), 0.0);
    }

    #[test]
    fn test_reset_peaks() {
        let mut port = InternalAudioPort::new("test", 4, 0, 0);

        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[0.5, 0.5, 0.5, 0.5]);
        }
        port.proc_process(4);

        assert!(port.get_input_peak() > 0.0);
        port.reset_input_peak();
        assert_eq!(port.get_input_peak(), 0.0);

        port.reset_output_peak();
        assert_eq!(port.get_output_peak(), 0.0);
    }

    #[test]
    fn test_ringbuffer_config() {
        let mut port = InternalAudioPort::new("test", 16, 0, 0);

        // Initially empty ringbuffer
        assert_eq!(port.get_ringbuffer_n_samples(), 0);

        // Configure minimum samples setting
        port.set_ringbuffer_n_samples(32);

        // Ringbuffer now configured for at least 32 samples
        // (max_buffers = ceil(32/16) = 2)
        // Put some audio data to populate the ringbuffer
        port.proc_prepare(16);
        let buf = port.proc_get_buffer(16);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 16);
            for (i, s) in slice.iter_mut().enumerate() {
                *s = i as f32 * 0.1;
            }
        }
        port.proc_process(16);

        // After processing, ringbuffer should have 16 samples
        assert_eq!(port.get_ringbuffer_n_samples(), 16);
    }

    #[test]
    fn test_ringbuffer_contents() {
        let port = InternalAudioPort::new("test", 16, 0, 0);

        // Get contents should return empty vec initially
        let contents = port.get_ringbuffer_contents();
        assert!(contents.is_empty());
    }
}
