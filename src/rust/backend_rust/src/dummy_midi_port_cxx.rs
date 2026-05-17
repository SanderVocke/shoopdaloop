//! CXX bridge for DummyMidiPort to expose to C++.

#![allow(dead_code)]

use crate::dummy_midi_port::{DummyMidiPort, PortDirection};

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DummyMidiPort;
        type PortDirection;

        // Constructors
        fn new_dummy_midi_port(is_output: bool) -> Box<DummyMidiPort>;

        // State accessors (delegate to base)
        fn get_muted(self: &DummyMidiPort) -> bool;
        fn set_muted(self: &mut DummyMidiPort, muted: bool);
        fn n_notes_active(self: &DummyMidiPort) -> u32;

        // Queue management
        fn queue_msg(self: &mut DummyMidiPort, time: u32, size: u16, data: &[u8; 4]);
        fn get_queue_empty(self: &DummyMidiPort) -> bool;
        fn clear_queues(self: &mut DummyMidiPort);

        // Request/response
        fn request_data(self: &mut DummyMidiPort, n_frames: u32);
        fn get_n_written_requested_msgs(self: &DummyMidiPort) -> u32;
        fn get_written_msg_time(self: &DummyMidiPort, idx: u32) -> u32;
        fn get_written_msg_size(self: &DummyMidiPort, idx: u32) -> u16;
        unsafe fn get_written_msg_bytes(
            self: &DummyMidiPort,
            idx: u32,
            out: *mut u8,
            max_len: usize,
        );
        fn clear_written_msgs(self: &mut DummyMidiPort);

        // Processing
        fn prepare(self: &mut DummyMidiPort, nframes: u32);
        fn process(self: &mut DummyMidiPort, nframes: u32);

        // MidiReadableBuffer
        fn n_events(self: &DummyMidiPort) -> u32;
        fn get_event_time(self: &DummyMidiPort, idx: u32) -> u32;
        fn get_event_size(self: &DummyMidiPort, idx: u32) -> u16;
        unsafe fn get_event_bytes(self: &DummyMidiPort, idx: u32, out: *mut u8, max_len: usize);

        // MidiWritableBuffer
        fn dummy_write_event(self: &mut DummyMidiPort, time: u32, size: u16, data: &[u8; 4]);
    }
}

fn new_dummy_midi_port(is_output: bool) -> Box<DummyMidiPort> {
    let direction = if is_output {
        PortDirection::Output
    } else {
        PortDirection::Input
    };
    Box::new(DummyMidiPort::new(direction))
}

fn get_muted(port: &DummyMidiPort) -> bool {
    port.base().get_muted()
}

fn set_muted(port: &mut DummyMidiPort, muted: bool) {
    port.base_mut().set_muted(muted);
}

fn n_notes_active(port: &DummyMidiPort) -> u32 {
    port.base().n_notes_active()
}

fn queue_msg(port: &mut DummyMidiPort, time: u32, size: u16, data: &[u8; 4]) {
    let slice = &data[..size as usize];
    port.queue_msg(time, size, slice);
}

fn get_queue_empty(port: &DummyMidiPort) -> bool {
    port.get_queue_empty()
}

fn clear_queues(port: &mut DummyMidiPort) {
    port.clear_queues();
}

fn request_data(port: &mut DummyMidiPort, n_frames: u32) {
    port.request_data(n_frames);
}

fn get_n_written_requested_msgs(port: &DummyMidiPort) -> u32 {
    port.get_n_written_requested_msgs()
}

fn get_written_msg_time(port: &DummyMidiPort, idx: u32) -> u32 {
    port.get_written_msg_time(idx)
}

fn get_written_msg_size(port: &DummyMidiPort, idx: u32) -> u16 {
    port.get_written_msg_size(idx)
}

unsafe fn get_written_msg_bytes(port: &DummyMidiPort, idx: u32, out: *mut u8, max_len: usize) {
    port.get_written_msg_bytes(idx, out, max_len);
}

fn clear_written_msgs(port: &mut DummyMidiPort) {
    port.clear_written_msgs();
}

fn prepare(port: &mut DummyMidiPort, nframes: u32) {
    port.prepare(nframes);
}

fn process(port: &mut DummyMidiPort, nframes: u32) {
    port.process(nframes);
}

fn n_events(port: &DummyMidiPort) -> u32 {
    crate::midi_traits::MidiReadableBuffer::n_events(port)
}

fn get_event_time(port: &DummyMidiPort, idx: u32) -> u32 {
    crate::midi_traits::MidiReadableBuffer::get_event(port, idx)
        .map(|e| e.time)
        .unwrap_or(INVALID_EVENT_TIME)
}

fn get_event_size(port: &DummyMidiPort, idx: u32) -> u16 {
    crate::midi_traits::MidiReadableBuffer::get_event(port, idx)
        .map(|e| e.size)
        .unwrap_or(INVALID_EVENT_SIZE)
}

unsafe fn get_event_bytes(port: &DummyMidiPort, idx: u32, out: *mut u8, max_len: usize) {
    if let Some(elem) = crate::midi_traits::MidiReadableBuffer::get_event(port, idx) {
        let len = std::cmp::min(elem.size as usize, max_len);
        std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
    }
}

fn dummy_write_event(port: &mut DummyMidiPort, time: u32, size: u16, data: &[u8; 4]) {
    let slice = &data[..size as usize];
    port.write_event_cxx(time, size, slice);
}

// Sentinel value for invalid/missing events
const INVALID_EVENT_TIME: u32 = 0xFFFFFFFF;
const INVALID_EVENT_SIZE: u16 = 0xFFFF;
