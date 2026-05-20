//! CXX bridge for InternalMidiPort to expose to C++.

#![allow(dead_code)]

use crate::internal_midi_port::InternalMidiPort;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type InternalMidiPort;

        fn new_internal_midi_port(
            name: &str,
            input_connectability: u32,
            output_connectability: u32,
        ) -> Box<InternalMidiPort>;

        fn name(self: &InternalMidiPort) -> &str;
        fn proc_prepare(self: &mut InternalMidiPort, nframes: u32);
        fn n_events(self: &InternalMidiPort) -> u32;
        fn get_event_time(self: &InternalMidiPort, idx: u32) -> u32;
        fn get_event_size(self: &InternalMidiPort, idx: u32) -> u16;
        unsafe fn get_event_bytes(self: &InternalMidiPort, idx: u32, out: *mut u8, max_len: usize);
        fn write_event(self: &mut InternalMidiPort, time: u32, size: u16, data: &[u8; 4]);
        fn input_connectability(self: &InternalMidiPort) -> u32;
        fn output_connectability(self: &InternalMidiPort) -> u32;
    }
}

fn new_internal_midi_port(
    name: &str,
    input_connectability: u32,
    output_connectability: u32,
) -> Box<InternalMidiPort> {
    Box::new(InternalMidiPort::new(
        name,
        input_connectability,
        output_connectability,
    ))
}

fn name(port: &InternalMidiPort) -> &str {
    port.name()
}

fn proc_prepare(port: &mut InternalMidiPort, nframes: u32) {
    port.proc_prepare(nframes);
}

fn n_events(port: &InternalMidiPort) -> u32 {
    port.n_events()
}

fn get_event_time(port: &InternalMidiPort, idx: u32) -> u32 {
    port.get_event_time(idx)
}

fn get_event_size(port: &InternalMidiPort, idx: u32) -> u16 {
    port.get_event_size(idx)
}

unsafe fn get_event_bytes(port: &InternalMidiPort, idx: u32, out: *mut u8, max_len: usize) {
    port.get_event_bytes(idx, out, max_len);
}

fn write_event(port: &mut InternalMidiPort, time: u32, size: u16, data: &[u8; 4]) {
    let slice = &data[..size as usize];
    port.write_event(time, size, slice);
}

fn input_connectability(port: &InternalMidiPort) -> u32 {
    port.input_connectability()
}

fn output_connectability(port: &InternalMidiPort) -> u32 {
    port.output_connectability()
}
