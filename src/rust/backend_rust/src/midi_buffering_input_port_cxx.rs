//! CXX bridge for MidiBufferingInputPort to expose to C++.

#![allow(dead_code)]

use crate::midi_buffering_input_port::MidiBufferingInputPort;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiBufferingInputPort;

        // Constructors
        fn new_midi_buffering_input_port(reserve_size: u32) -> Box<MidiBufferingInputPort>;

        // State accessors (delegate to base)
        fn get_muted(self: &MidiBufferingInputPort) -> bool;
        fn set_muted(self: &mut MidiBufferingInputPort, muted: bool);
        fn n_notes_active(self: &MidiBufferingInputPort) -> u32;

        // Buffer management
        fn clear(self: &mut MidiBufferingInputPort);
        fn buffer_event_with_params(
            self: &mut MidiBufferingInputPort,
            time: u32,
            size: u16,
            data: &[u8; 4],
        );

        // MidiReadableBuffer
        fn n_events(self: &MidiBufferingInputPort) -> u32;
        fn get_event_time(self: &MidiBufferingInputPort, idx: u32) -> u32;
        fn get_event_size(self: &MidiBufferingInputPort, idx: u32) -> u16;
        unsafe fn get_event_bytes(
            self: &MidiBufferingInputPort,
            idx: u32,
            out: *mut u8,
            max_len: usize,
        );
    }
}

fn new_midi_buffering_input_port(reserve_size: u32) -> Box<MidiBufferingInputPort> {
    Box::new(MidiBufferingInputPort::new(reserve_size))
}

fn get_muted(port: &MidiBufferingInputPort) -> bool {
    port.get_muted()
}

fn set_muted(port: &mut MidiBufferingInputPort, muted: bool) {
    port.set_muted(muted);
}

fn n_notes_active(port: &MidiBufferingInputPort) -> u32 {
    port.n_notes_active()
}

fn clear(port: &mut MidiBufferingInputPort) {
    port.clear();
}

fn buffer_event_with_params(
    port: &mut MidiBufferingInputPort,
    time: u32,
    size: u16,
    data: &[u8; 4],
) {
    let slice = &data[..size as usize];
    port.buffer_event_with_params(time, size, slice);
}

fn n_events(port: &MidiBufferingInputPort) -> u32 {
    port.n_events()
}

fn get_event_time(port: &MidiBufferingInputPort, idx: u32) -> u32 {
    port.get_event_time(idx)
}

fn get_event_size(port: &MidiBufferingInputPort, idx: u32) -> u16 {
    port.get_event_size(idx)
}

unsafe fn get_event_bytes(port: &MidiBufferingInputPort, idx: u32, out: *mut u8, max_len: usize) {
    port.get_event_bytes(idx, out, max_len);
}
