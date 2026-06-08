//! Rust-native DecoupledMidiPort.
//!
//! Owns a C++ MidiPort through bridge-object handles and maintains a bounded
//! lock-free queue for decoupled MIDI traffic.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crossbeam_queue::ArrayQueue;

use crate::audio_midi_driver_bridge_cxx;
use crate::cpp_midi_port_cxx;
use crate::midi_storage::MidiStorageElem;
use crate::port_direction::PortDirection;

pub struct DecoupledMidiQueue {
    queue: ArrayQueue<MidiStorageElem>,
}

impl DecoupledMidiQueue {
    pub fn new(queue_size: usize) -> Self {
        Self {
            queue: ArrayQueue::new(queue_size),
        }
    }

    pub fn push(&self, msg: MidiStorageElem) -> bool {
        self.queue.push(msg).is_ok()
    }

    pub fn pop(&self) -> Option<MidiStorageElem> {
        self.queue.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

pub struct DecoupledMidiPort {
    queue: DecoupledMidiQueue,
    direction: PortDirection,
    midi_port: Mutex<cxx::UniquePtr<cpp_midi_port_cxx::ffi::MidiPortBridgeStrong>>,
    maybe_driver_ptr: usize,
    registry_handle: AtomicU64,
    closed: AtomicBool,
}

impl DecoupledMidiPort {
    pub fn new(
        midi_port: cxx::UniquePtr<cpp_midi_port_cxx::ffi::MidiPortBridgeStrong>,
        maybe_driver_ptr: usize,
        queue_size: usize,
        direction: PortDirection,
    ) -> Self {
        Self {
            queue: DecoupledMidiQueue::new(queue_size),
            direction,
            midi_port: Mutex::new(midi_port),
            maybe_driver_ptr,
            registry_handle: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }

    pub fn process(&self, nframes: u32) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut guard) = self.midi_port.lock() else {
            return;
        };
        let Some(strong) = guard.as_mut() else {
            return;
        };
        let mut port = unsafe { strong.get_pin_mut() };

        match self.direction {
            PortDirection::Input => {
                cpp_midi_port_cxx::ffi::midi_port_prepare(port.as_mut(), nframes);
                cpp_midi_port_cxx::ffi::midi_port_process(port.as_mut(), nframes);
                let buf_ptr =
                    cpp_midi_port_cxx::ffi::midi_port_get_read_output_data_buffer(port, nframes);
                let n_events = cpp_midi_port_cxx::ffi::midi_readable_buffer_n_events(buf_ptr);
                for idx in 0..n_events {
                    let mut time = 0u32;
                    let mut size = 0u16;
                    let mut data = [0u8; 4];
                    let ok = unsafe {
                        cpp_midi_port_cxx::ffi::midi_readable_buffer_get_event(
                            buf_ptr,
                            idx,
                            &mut time,
                            &mut size,
                            data.as_mut_ptr(),
                        )
                    };
                    if ok {
                        if let Some(elem) = MidiStorageElem::new(time, size, &data[..size as usize]) {
                            let _ = self.queue.push(elem);
                        }
                    }
                }
            }
            PortDirection::Output => {
                cpp_midi_port_cxx::ffi::midi_port_prepare(port.as_mut(), nframes);
                let buf_ptr = cpp_midi_port_cxx::ffi::midi_port_get_write_data_into_port_buffer(
                    port.as_mut(),
                    nframes,
                );
                while let Some(msg) = self.queue.pop() {
                    let _ = unsafe {
                        cpp_midi_port_cxx::ffi::midi_writeable_buffer_write_event(
                            buf_ptr,
                            msg.time,
                            msg.size,
                            msg.data().as_ptr(),
                        )
                    };
                }
                cpp_midi_port_cxx::ffi::midi_port_process(port, nframes);
            }
        }
    }

    pub fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(mut guard) = self.midi_port.lock() {
            if let Some(strong) = guard.as_mut() {
                let port = unsafe { strong.get_pin_mut() };
                cpp_midi_port_cxx::ffi::midi_port_close(port);
            }
        }
    }

    pub fn request_close(&self) {
        let handle = self.registry_handle();
        if handle == 0 || self.maybe_driver_ptr == 0 {
            self.close();
            return;
        }
        audio_midi_driver_bridge_cxx::ffi::audiomididriver_request_close_decoupled_midi_port(
            self.maybe_driver_ptr,
            handle,
        );
    }

    pub fn pop_incoming(&self) -> Option<MidiStorageElem> {
        if self.direction != PortDirection::Input {
            return None;
        }
        self.queue.pop()
    }

    pub fn push_outgoing(&self, msg: MidiStorageElem) -> bool {
        self.queue.push(msg)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn direction(&self) -> PortDirection {
        self.direction
    }

    pub fn registry_handle(&self) -> u64 {
        self.registry_handle.load(Ordering::SeqCst)
    }

    pub fn set_registry_handle(&self, handle: u64) {
        self.registry_handle.store(handle, Ordering::SeqCst);
    }

    pub fn clone_cpp_midi_port(
        &self,
    ) -> cxx::UniquePtr<cpp_midi_port_cxx::ffi::MidiPortBridgeStrong> {
        let Ok(guard) = self.midi_port.lock() else {
            return cxx::UniquePtr::null();
        };
        let Some(strong) = guard.as_ref() else {
            return cxx::UniquePtr::null();
        };
        let weak = strong.downgrade();
        if weak.is_null() {
            return cxx::UniquePtr::null();
        }
        weak.upgrade()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_push_and_pop() {
        let queue = DecoupledMidiQueue::new(4);
        let msg = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        assert!(queue.push(msg.clone()));
        assert_eq!(queue.pop(), Some(msg));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_drops_when_full() {
        let queue = DecoupledMidiQueue::new(1);
        let msg1 = MidiStorageElem::new(10, 1, &[0x90]).unwrap();
        let msg2 = MidiStorageElem::new(20, 1, &[0x91]).unwrap();
        assert!(queue.push(msg1.clone()));
        assert!(!queue.push(msg2));
        assert_eq!(queue.pop(), Some(msg1));
    }
}
