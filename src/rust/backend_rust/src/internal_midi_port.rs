//! InternalMidiPort - MIDI port with local event buffer management.
//!
//! Created from scratch following the Rust-core / C++-wrapper pattern.
//!
//! The Rust core owns the local MIDI event buffer and metadata.
//! MIDI processing (state tracking, ringbuffer) is handled by the
//! C++ MidiPort base class.

use crate::midi_storage::MidiStorageElem;

pub struct InternalMidiPort {
    events: Vec<MidiStorageElem>,
    name: String,
    input_connectability: u32,
    output_connectability: u32,
}

impl InternalMidiPort {
    pub fn new(name: &str, input_connectability: u32, output_connectability: u32) -> Self {
        InternalMidiPort {
            events: Vec::new(),
            name: name.to_string(),
            input_connectability,
            output_connectability,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn proc_prepare(&mut self, _nframes: u32) {
        self.events.clear();
    }

    pub fn n_events(&self) -> u32 {
        self.events.len() as u32
    }

    pub fn get_event_time(&self, idx: u32) -> u32 {
        self.events
            .get(idx as usize)
            .map(|e| e.time)
            .unwrap_or(INVALID_EVENT_TIME)
    }

    pub fn get_event_size(&self, idx: u32) -> u16 {
        self.events
            .get(idx as usize)
            .map(|e| e.size)
            .unwrap_or(INVALID_EVENT_SIZE)
    }

    pub unsafe fn get_event_bytes(&self, idx: u32, out: *mut u8, max_len: usize) {
        if let Some(elem) = self.events.get(idx as usize) {
            let len = std::cmp::min(elem.size as usize, max_len);
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }

    pub fn write_event(&mut self, time: u32, size: u16, data: &[u8]) {
        if let Some(elem) = MidiStorageElem::new(time, size, data) {
            self.events.push(elem);
        }
    }

    pub fn input_connectability(&self) -> u32 {
        self.input_connectability
    }

    pub fn output_connectability(&self) -> u32 {
        self.output_connectability
    }
}

const INVALID_EVENT_TIME: u32 = 0xFFFFFFFF;
const INVALID_EVENT_SIZE: u16 = 0xFFFF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let port = InternalMidiPort::new("test", 1, 2);
        assert_eq!(port.name(), "test");
        assert_eq!(port.input_connectability(), 1);
        assert_eq!(port.output_connectability(), 2);
        assert_eq!(port.n_events(), 0);
    }

    #[test]
    fn test_prepare_clears_events() {
        let mut port = InternalMidiPort::new("test", 0, 0);
        port.write_event(10, 3, &[0x90, 0x3C, 0x7F]);
        assert_eq!(port.n_events(), 1);
        port.proc_prepare(128);
        assert_eq!(port.n_events(), 0);
    }

    #[test]
    fn test_write_then_read_event_round_trip() {
        let mut port = InternalMidiPort::new("test", 0, 0);
        port.write_event(100, 3, &[0x90, 0x3C, 0x7F]);
        assert_eq!(port.n_events(), 1);
        assert_eq!(port.get_event_time(0), 100);
        assert_eq!(port.get_event_size(0), 3);

        let mut bytes = [0u8; 4];
        unsafe {
            port.get_event_bytes(0, bytes.as_mut_ptr(), 4);
        }
        assert_eq!(&bytes[..3], &[0x90, 0x3C, 0x7F]);
    }

    #[test]
    fn test_out_of_bounds_returns_sentinels() {
        let port = InternalMidiPort::new("test", 0, 0);
        assert_eq!(port.get_event_time(0), INVALID_EVENT_TIME);
        assert_eq!(port.get_event_size(0), INVALID_EVENT_SIZE);
    }
}
