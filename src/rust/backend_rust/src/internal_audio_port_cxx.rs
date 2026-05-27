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
        fn input_connectability(self: &InternalAudioPort) -> u32;
        fn output_connectability(self: &InternalAudioPort) -> u32;
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

fn input_connectability(port: &InternalAudioPort) -> u32 {
    port.input_connectability()
}

fn output_connectability(port: &InternalAudioPort) -> u32 {
    port.output_connectability()
}
