//! CXX bridge for DummyAudioPort to expose to C++.

#![allow(dead_code)]

use crate::dummy_audio_port::DummyAudioPort;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DummyAudioPort;

        fn new_dummy_audio_port(
            name: &str,
            is_output: bool,
            driver_handle: usize,
        ) -> Box<DummyAudioPort>;

        // Port metadata
        fn name(self: &DummyAudioPort) -> &str;
        fn maybe_driver_handle(self: &DummyAudioPort) -> usize;

        // Dummy-specific methods
        fn queue_data(self: &mut DummyAudioPort, n_frames: u32, data: &[f32]);
        fn get_queue_empty(self: &DummyAudioPort) -> bool;
        fn request_data(self: &mut DummyAudioPort, n_frames: u32);
        fn dequeue_data(self: &mut DummyAudioPort, n: u32) -> Vec<f32>;

        // Processing
        fn proc_prepare(self: &mut DummyAudioPort, nframes: u32);
        fn proc_process(self: &mut DummyAudioPort, nframes: u32);
        unsafe fn proc_get_buffer(self: &mut DummyAudioPort, nframes: u32) -> *mut f32;
    }
}

fn new_dummy_audio_port(name: &str, is_output: bool, driver_handle: usize) -> Box<DummyAudioPort> {
    Box::new(DummyAudioPort::new(name, is_output, driver_handle))
}

fn name(port: &DummyAudioPort) -> &str {
    port.name()
}

fn maybe_driver_handle(port: &DummyAudioPort) -> usize {
    port.maybe_driver_handle()
}

fn queue_data(port: &mut DummyAudioPort, n_frames: u32, data: &[f32]) {
    port.queue_data(n_frames, data);
}

fn get_queue_empty(port: &DummyAudioPort) -> bool {
    port.get_queue_empty()
}

fn request_data(port: &mut DummyAudioPort, n_frames: u32) {
    port.request_data(n_frames);
}

fn dequeue_data(port: &mut DummyAudioPort, n: u32) -> Vec<f32> {
    port.dequeue_data(n)
}

fn proc_prepare(port: &mut DummyAudioPort, nframes: u32) {
    port.proc_prepare(nframes);
}

fn proc_process(port: &mut DummyAudioPort, nframes: u32) {
    port.proc_process(nframes);
}

unsafe fn proc_get_buffer(port: &mut DummyAudioPort, nframes: u32) -> *mut f32 {
    port.proc_get_buffer(nframes)
}
