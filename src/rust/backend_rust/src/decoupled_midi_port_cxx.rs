//! CXX bridge for the Rust-native DecoupledMidiPort and its Rust bridge handles.

#![allow(dead_code)]

use std::sync::Arc;

use crate::decoupled_midi_port::DecoupledMidiPort;
use crate::midi_storage::MidiStorageElem;
use crate::port_direction::PortDirection;

crate::define_rust_bridge_object_wrappers!(
    DecoupledMidiPortBridgeStrong,
    DecoupledMidiPortBridgeWeak,
    DecoupledMidiPort
);

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/MidiPortCxxBridge.h");

        #[namespace = ""]
        type MidiPortBridgeStrong = crate::cpp_midi_port_cxx::ffi::MidiPortBridgeStrong;
    }

    extern "Rust" {
        type DecoupledMidiPortBridgeStrong;
        type DecoupledMidiPortBridgeWeak;

        fn new_decoupled_midi_port(
            midi_port: UniquePtr<MidiPortBridgeStrong>,
            queue_size: u32,
            direction: u32,
        ) -> Box<DecoupledMidiPortBridgeStrong>;

        fn id(self: &DecoupledMidiPortBridgeStrong) -> u64;
        fn valid(self: &DecoupledMidiPortBridgeStrong) -> bool;
        fn downgrade(self: &DecoupledMidiPortBridgeStrong) -> Box<DecoupledMidiPortBridgeWeak>;
        fn clone_strong(self: &DecoupledMidiPortBridgeStrong)
            -> Box<DecoupledMidiPortBridgeStrong>;
        fn strong_count(self: &DecoupledMidiPortBridgeStrong) -> usize;
        fn weak_count(self: &DecoupledMidiPortBridgeStrong) -> usize;

        fn id(self: &DecoupledMidiPortBridgeWeak) -> u64;
        fn valid(self: &DecoupledMidiPortBridgeWeak) -> bool;
        fn upgrade(self: &DecoupledMidiPortBridgeWeak) -> Box<DecoupledMidiPortBridgeStrong>;
        fn clone(self: &DecoupledMidiPortBridgeWeak) -> Box<DecoupledMidiPortBridgeWeak>;
        fn strong_count(self: &DecoupledMidiPortBridgeWeak) -> usize;
        fn weak_count(self: &DecoupledMidiPortBridgeWeak) -> usize;

        fn decoupled_process(port: &DecoupledMidiPortBridgeStrong, nframes: u32) -> bool;
        fn decoupled_close(port: &DecoupledMidiPortBridgeStrong) -> bool;
        fn decoupled_request_close(port: &DecoupledMidiPortBridgeStrong) -> bool;
        unsafe fn decoupled_pop(
            port: &DecoupledMidiPortBridgeStrong,
            out_time: &mut u32,
            out_size: &mut u16,
            out_data: *mut u8,
        ) -> bool;
        fn decoupled_push(
            port: &DecoupledMidiPortBridgeStrong,
            time: u32,
            size: u16,
            data: &[u8; 4],
        ) -> bool;
        fn decoupled_is_empty(port: &DecoupledMidiPortBridgeStrong) -> bool;
        fn decoupled_cpp_midi_port(
            port: &DecoupledMidiPortBridgeStrong,
        ) -> UniquePtr<MidiPortBridgeStrong>;
    }
}

fn new_decoupled_midi_port(
    midi_port: cxx::UniquePtr<ffi::MidiPortBridgeStrong>,
    queue_size: u32,
    direction: u32,
) -> Box<DecoupledMidiPortBridgeStrong> {
    let port_direction = if direction == 1 {
        PortDirection::Output
    } else {
        PortDirection::Input
    };
    Box::new(DecoupledMidiPortBridgeStrong::from_arc(Arc::new(
        DecoupledMidiPort::new(midi_port, queue_size as usize, port_direction),
    )))
}

pub(crate) fn decoupled_process(port: &DecoupledMidiPortBridgeStrong, nframes: u32) -> bool {
    port.0
        .with(|inner| {
            inner.process(nframes);
            true
        })
        .unwrap_or(false)
}

pub(crate) fn decoupled_close(port: &DecoupledMidiPortBridgeStrong) -> bool {
    port.0
        .with(|inner| {
            inner.close();
            true
        })
        .unwrap_or(false)
}

pub(crate) fn decoupled_request_close(port: &DecoupledMidiPortBridgeStrong) -> bool {
    port.0
        .with(|inner| {
            inner.request_close();
            true
        })
        .unwrap_or(false)
}

unsafe fn decoupled_pop(
    port: &DecoupledMidiPortBridgeStrong,
    out_time: &mut u32,
    out_size: &mut u16,
    out_data: *mut u8,
) -> bool {
    port.0
        .with(|inner| {
            if let Some(elem) = inner.pop_incoming() {
                *out_time = elem.time;
                *out_size = elem.size;
                std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out_data, elem.size as usize);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}

fn decoupled_push(
    port: &DecoupledMidiPortBridgeStrong,
    time: u32,
    size: u16,
    data: &[u8; 4],
) -> bool {
    port.0
        .with(|inner| {
            MidiStorageElem::new(time, size, &data[..size as usize])
                .map(|elem| inner.push_outgoing(elem))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn decoupled_is_empty(port: &DecoupledMidiPortBridgeStrong) -> bool {
    port.0.with(|inner| inner.is_empty()).unwrap_or(true)
}

pub(crate) fn decoupled_is_closed(port: &DecoupledMidiPortBridgeStrong) -> bool {
    port.0.with(|inner| inner.is_closed()).unwrap_or(true)
}

fn decoupled_cpp_midi_port(
    port: &DecoupledMidiPortBridgeStrong,
) -> cxx::UniquePtr<ffi::MidiPortBridgeStrong> {
    port.0
        .with(|inner| inner.clone_cpp_midi_port())
        .unwrap_or_else(cxx::UniquePtr::null)
}
