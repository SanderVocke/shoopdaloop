//! CXX bridge for AudioPort to expose to C++.
//!
//! ============================================================================
//! ## AudioPort CXX Bridge
//! ============================================================================
//! This bridge exposes the Rust AudioPort implementation to C++.
//! Uses the same pattern as audio_buffer_queue_cxx.rs for snapshot returns.
//!
//! NOTE: The audio buffer is managed by C++ code. The Rust implementation
//! receives buffer pointers via FFI for processing (peak tracking, gain/mute).
//!
//! CXX Limitations:
//! - snapshot() returns Vec<BufferPtrInfo> with raw pointers
//! - C++ must memcpy data into AudioBuffer objects (acceptable for consumer thread)
//! - Cannot return SharedPtr<Vec<f32>> - CXX doesn't support this pattern
//! ============================================================================

use crate::audio_port::AudioPort;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // Buffer info struct for ringbuffer snapshots
    struct AudioBufferInfo {
        data_ptr: *const f32,
        len: usize,
    }

    extern "Rust" {
        // Main AudioPort type
        type AudioPort;

        // Constructor with pool configuration
        // pool_capacity: max buffers in pool
        // low_water_mark: min buffers to keep in pool
        // buffer_size: size of each buffer in samples
        // max_buffers: max buffers in ringbuffer
        fn new_audio_port(
            pool_capacity: usize,
            low_water_mark: usize,
            buffer_size: usize,
            max_buffers: u32,
        ) -> Box<AudioPort>;

        // Process audio: applies gain/mute, tracks peaks, records to ringbuffer
        // Takes a buffer pointer managed by C++ code
        //
        // Safety: buffer must point to valid memory with at least n_frames elements
        unsafe fn audio_port_process(port: &mut AudioPort, buffer: *mut f32, n_frames: u32);

        // Gain control
        fn audio_port_set_gain(port: &AudioPort, gain: f32);
        fn audio_port_get_gain(port: &AudioPort) -> f32;

        // Mute control
        fn audio_port_set_muted(port: &AudioPort, muted: bool);
        fn audio_port_get_muted(port: &AudioPort) -> bool;

        // Peak meters
        fn audio_port_get_input_peak(port: &AudioPort) -> f32;
        fn audio_port_reset_input_peak(port: &AudioPort);
        fn audio_port_get_output_peak(port: &AudioPort) -> f32;
        fn audio_port_reset_output_peak(port: &AudioPort);

        // Ringbuffer configuration
        fn audio_port_set_ringbuffer_n_samples(port: &mut AudioPort, n: u32);
        fn audio_port_get_ringbuffer_n_samples(port: &AudioPort) -> u32;

        // Ringbuffer access
        // Returns vector of buffer info (raw pointers) for snapshot
        fn audio_port_get_ringbuffer_contents(port: &AudioPort) -> Vec<AudioBufferInfo>;

        // Single buffer size query
        fn audio_port_get_single_buffer_size(port: &AudioPort) -> u32;
    }
}

// Constructor
fn new_audio_port(
    pool_capacity: usize,
    low_water_mark: usize,
    buffer_size: usize,
    max_buffers: u32,
) -> Box<AudioPort> {
    Box::new(AudioPort::new(
        pool_capacity,
        low_water_mark,
        buffer_size,
        max_buffers,
    ))
}

// Process audio - takes buffer pointer from C++
unsafe fn audio_port_process(port: &mut AudioPort, buffer: *mut f32, n_frames: u32) {
    port.process_raw(buffer, n_frames);
}

// Gain control
fn audio_port_set_gain(port: &AudioPort, gain: f32) {
    port.set_gain(gain);
}

fn audio_port_get_gain(port: &AudioPort) -> f32 {
    port.get_gain()
}

// Mute control
fn audio_port_set_muted(port: &AudioPort, muted: bool) {
    port.set_muted(muted);
}

fn audio_port_get_muted(port: &AudioPort) -> bool {
    port.get_muted()
}

// Peak meters
fn audio_port_get_input_peak(port: &AudioPort) -> f32 {
    port.get_input_peak()
}

fn audio_port_reset_input_peak(port: &AudioPort) {
    port.reset_input_peak();
}

fn audio_port_get_output_peak(port: &AudioPort) -> f32 {
    port.get_output_peak()
}

fn audio_port_reset_output_peak(port: &AudioPort) {
    port.reset_output_peak();
}

// Ringbuffer configuration
fn audio_port_set_ringbuffer_n_samples(port: &mut AudioPort, n: u32) {
    port.set_ringbuffer_n_samples(n);
}

fn audio_port_get_ringbuffer_n_samples(port: &AudioPort) -> u32 {
    port.get_ringbuffer_n_samples()
}

// Ringbuffer access - returns Vec<AudioBufferInfo> with raw pointers
fn audio_port_get_ringbuffer_contents(port: &AudioPort) -> Vec<ffi::AudioBufferInfo> {
    let contents = port.get_ringbuffer_contents_ptrs();
    contents
        .into_iter()
        .map(|(data_ptr, len)| ffi::AudioBufferInfo { data_ptr, len })
        .collect()
}

// Single buffer size query
fn audio_port_get_single_buffer_size(port: &AudioPort) -> u32 {
    port.get_single_buffer_size()
}
