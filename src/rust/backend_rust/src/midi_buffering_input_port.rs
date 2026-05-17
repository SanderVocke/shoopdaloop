//! MidiBufferingInputPort - A MIDI port that buffers messages from another input source.
//!
//! Buffers MIDI messages so they can be passed on and sorted downstream.
//!
//! Ported from C++ MidiBufferingInputPort.h/cpp

use crate::midi_port::MidiPort;
use crate::midi_storage::MidiStorageElem;
use crate::midi_traits::MidiReadableBuffer;

/// MidiBufferingInputPort - A MIDI port that buffers messages for downstream processing.
///
/// Inherits from MidiPort for base functionality and adds buffering capability.
pub struct MidiBufferingInputPort {
    // Base MIDI port
    base: MidiPort,

    // Temporary storage for buffered messages
    temp_midi_storage: Vec<MidiStorageElem>,
}

impl MidiBufferingInputPort {
    /// Create a new MidiBufferingInputPort
    pub fn new(reserve_size: u32) -> Self {
        MidiBufferingInputPort {
            base: MidiPort::new(true, true, true), // track_notes, track_controls, track_programs
            temp_midi_storage: Vec::with_capacity(reserve_size as usize),
        }
    }

    /// Get reference to base MidiPort
    pub fn base(&self) -> &MidiPort {
        &self.base
    }

    /// Get mutable reference to base MidiPort
    pub fn base_mut(&mut self) -> &mut MidiPort {
        &mut self.base
    }

    /// Clear the internal buffer
    pub fn clear(&mut self) {
        self.temp_midi_storage.clear();
    }

    /// Buffer a MIDI event
    pub fn buffer_event(&mut self, event: MidiStorageElem) {
        self.temp_midi_storage.push(event);
    }

    /// Buffer a MIDI event from components (for FFI)
    pub fn buffer_event_with_params(&mut self, time: u32, size: u16, data: &[u8]) {
        if let Some(elem) = MidiStorageElem::new(time, size, data) {
            self.buffer_event(elem);
        }
    }

    /// Buffer events from another readable buffer
    pub fn buffer_from_buffer(
        &mut self,
        n_events: u32,
        get_event_fn: impl Fn(u32) -> Option<MidiStorageElem>,
    ) {
        for i in 0..n_events {
            if let Some(event) = get_event_fn(i) {
                self.buffer_event(event);
            }
        }
    }

    // Delegation methods for CXX bridge compatibility
    pub fn get_muted(&self) -> bool {
        self.base.get_muted()
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.base.set_muted(muted);
    }

    pub fn n_notes_active(&self) -> u32 {
        self.base.n_notes_active()
    }

    pub fn n_events(&self) -> u32 {
        MidiReadableBuffer::n_events(self)
    }

    pub fn get_event_time(&self, idx: u32) -> u32 {
        MidiReadableBuffer::get_event(self, idx)
            .map(|e| e.time)
            .unwrap_or(u32::MAX)
    }

    pub fn get_event_size(&self, idx: u32) -> u16 {
        MidiReadableBuffer::get_event(self, idx)
            .map(|e| e.size)
            .unwrap_or(u16::MAX)
    }

    pub unsafe fn get_event_bytes(&self, idx: u32, out: *mut u8, max_len: usize) {
        if let Some(elem) = MidiReadableBuffer::get_event(self, idx) {
            let len = std::cmp::min(elem.size as usize, max_len);
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }
}

// Implement MidiReadableBuffer for MidiBufferingInputPort
impl MidiReadableBuffer for MidiBufferingInputPort {
    fn n_events(&self) -> u32 {
        self.temp_midi_storage.len() as u32
    }

    fn get_event(&self, idx: u32) -> Option<MidiStorageElem> {
        self.temp_midi_storage.get(idx as usize).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffering_input_port_creation() {
        let port = MidiBufferingInputPort::new(1024);
        assert!(!port.base().get_muted());
    }

    #[test]
    fn test_buffer_events() {
        let mut port = MidiBufferingInputPort::new(1024);
        let event = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        port.buffer_event(event);
        assert_eq!(port.n_events(), 1);
    }

    #[test]
    fn test_clear_buffer() {
        let mut port = MidiBufferingInputPort::new(1024);
        let event = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        port.buffer_event(event);
        assert_eq!(port.n_events(), 1);
        port.clear();
        assert_eq!(port.n_events(), 0);
    }

    #[test]
    fn test_get_event() {
        let mut port = MidiBufferingInputPort::new(1024);
        let event = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        port.buffer_event(event.clone());

        let retrieved = port.get_event(0);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().time, 100);
    }

    #[test]
    fn test_buffer_multiple_events() {
        let mut port = MidiBufferingInputPort::new(1024);

        for i in 0..5 {
            let event = MidiStorageElem::new(i * 100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
            port.buffer_event(event);
        }

        assert_eq!(port.n_events(), 5);
    }
}
