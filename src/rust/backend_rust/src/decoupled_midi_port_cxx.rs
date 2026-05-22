//! CXX bridge for DecoupledMidiPort to expose to C++.

#![allow(dead_code)]

use crate::decoupled_midi_port::DecoupledMidiPort;
use crate::midi_storage::MidiStorageElem;
use crate::port_direction::PortDirection;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DecoupledMidiPort;

        fn new_decoupled_midi_port(queue_size: u32, direction: u32) -> Box<DecoupledMidiPort>;
        fn decoupled_push(port: &DecoupledMidiPort, time: u32, size: u16, data: &[u8; 4]) -> bool;
        unsafe fn decoupled_pop(
            port: &DecoupledMidiPort,
            out_time: &mut u32,
            out_size: &mut u16,
            out_data: *mut u8,
        ) -> bool;
        fn decoupled_is_empty(port: &DecoupledMidiPort) -> bool;
    }
}

fn new_decoupled_midi_port(queue_size: u32, direction: u32) -> Box<DecoupledMidiPort> {
    let port_direction = if direction == 1 {
        PortDirection::Output
    } else {
        PortDirection::Input
    };
    Box::new(DecoupledMidiPort::new(queue_size as usize, port_direction))
}

fn decoupled_push(port: &DecoupledMidiPort, time: u32, size: u16, data: &[u8; 4]) -> bool {
    if let Some(elem) = MidiStorageElem::new(time, size, &data[..size as usize]) {
        port.push(elem)
    } else {
        false
    }
}

unsafe fn decoupled_pop(
    port: &DecoupledMidiPort,
    out_time: &mut u32,
    out_size: &mut u16,
    out_data: *mut u8,
) -> bool {
    if let Some(elem) = port.pop() {
        *out_time = elem.time;
        *out_size = elem.size;
        std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out_data, elem.size as usize);
        true
    } else {
        false
    }
}

fn decoupled_is_empty(port: &DecoupledMidiPort) -> bool {
    port.is_empty()
}
