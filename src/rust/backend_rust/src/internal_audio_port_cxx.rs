//! CXX bridge for InternalAudioPort to expose to C++.

#![allow(dead_code)]

use crate::internal_audio_port::InternalAudioPort;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type InternalAudioPort;

        fn new_internal_audio_port(
            name: &str,
            n_frames: u32,
            input_connectability: u32,
            output_connectability: u32,
        ) -> Box<InternalAudioPort>;

        fn name(self: &InternalAudioPort) -> &str;
        fn proc_prepare(self: &mut InternalAudioPort, nframes: u32);
        unsafe fn proc_get_buffer(self: &mut InternalAudioPort, nframes: u32) -> *mut f32;
        fn proc_process(self: &mut InternalAudioPort, nframes: u32);
        fn input_connectability(self: &InternalAudioPort) -> u32;
        fn output_connectability(self: &InternalAudioPort) -> u32;

        // Gain/mute/peak control
        fn set_gain(self: &InternalAudioPort, gain: f32);
        fn get_gain(self: &InternalAudioPort) -> f32;
        fn set_muted(self: &InternalAudioPort, muted: bool);
        fn get_muted(self: &InternalAudioPort) -> bool;
        fn get_input_peak(self: &InternalAudioPort) -> f32;
        fn get_output_peak(self: &InternalAudioPort) -> f32;
        fn reset_input_peak(self: &InternalAudioPort);
        fn reset_output_peak(self: &InternalAudioPort);

        // Ringbuffer methods
        fn set_ringbuffer_n_samples(self: &mut InternalAudioPort, n: u32);
        fn get_ringbuffer_n_samples(self: &InternalAudioPort) -> u32;
        fn get_ringbuffer_n_buffers(self: &InternalAudioPort) -> usize;
        fn get_ringbuffer_single_buffer_size(self: &InternalAudioPort) -> u32;
    }

    /// Buffer info for ringbuffer snapshots (FFI-safe struct)
    struct BufferInfo {
        data_ptr: *const f32,
        len: usize,
    }

    extern "Rust" {
        unsafe fn take_snapshot(port: &mut InternalAudioPort) -> *mut Vec<BufferInfo>;
    }
}

fn new_internal_audio_port(
    name: &str,
    n_frames: u32,
    input_connectability: u32,
    output_connectability: u32,
) -> Box<InternalAudioPort> {
    Box::new(InternalAudioPort::new(
        name,
        n_frames,
        input_connectability,
        output_connectability,
    ))
}

fn name(port: &InternalAudioPort) -> &str {
    port.name()
}

fn proc_prepare(port: &mut InternalAudioPort, nframes: u32) {
    port.proc_prepare(nframes);
}

unsafe fn proc_get_buffer(port: &mut InternalAudioPort, nframes: u32) -> *mut f32 {
    port.proc_get_buffer(nframes)
}

fn proc_process(port: &mut InternalAudioPort, nframes: u32) {
    port.proc_process(nframes);
}

fn input_connectability(port: &InternalAudioPort) -> u32 {
    port.input_connectability()
}

fn output_connectability(port: &InternalAudioPort) -> u32 {
    port.output_connectability()
}

// Gain/mute/peak control
fn set_gain(port: &InternalAudioPort, gain: f32) {
    port.set_gain(gain);
}

fn get_gain(port: &InternalAudioPort) -> f32 {
    port.get_gain()
}

fn set_muted(port: &InternalAudioPort, muted: bool) {
    port.set_muted(muted);
}

fn get_muted(port: &InternalAudioPort) -> bool {
    port.get_muted()
}

fn get_input_peak(port: &InternalAudioPort) -> f32 {
    port.get_input_peak()
}

fn get_output_peak(port: &InternalAudioPort) -> f32 {
    port.get_output_peak()
}

fn reset_input_peak(port: &InternalAudioPort) {
    port.reset_input_peak();
}

fn reset_output_peak(port: &InternalAudioPort) {
    port.reset_output_peak();
}

// Ringbuffer methods
fn set_ringbuffer_n_samples(port: &mut InternalAudioPort, n: u32) {
    port.set_ringbuffer_n_samples(n);
}

fn get_ringbuffer_n_samples(port: &InternalAudioPort) -> u32 {
    port.get_ringbuffer_n_samples()
}

/// Wrapper that returns number of buffers in ringbuffer
fn get_ringbuffer_n_buffers(port: &InternalAudioPort) -> usize {
    port.get_ringbuffer_contents().len()
}

/// Wrapper that returns single buffer size
fn get_ringbuffer_single_buffer_size(port: &InternalAudioPort) -> u32 {
    port.get_ringbuffer_single_buffer_size()
}

/// Wrapper that returns ringbuffer contents as a Vec<BufferInfo>
/// Returns raw pointer to Vec - caller must delete it.
/// # Safety
/// The returned pointer must be deleted by the caller using `delete`.
fn take_snapshot(port: &mut InternalAudioPort) -> *mut Vec<ffi::BufferInfo> {
    let tuple_vec = port.get_ringbuffer_contents();
    let mut result = Vec::with_capacity(tuple_vec.len());
    for (data_ptr, len) in tuple_vec {
        result.push(ffi::BufferInfo { data_ptr, len });
    }
    Box::into_raw(Box::new(result))
}
