//! MidiPort - Main MIDI port implementation in Rust.
//!
//! Uses composition to delegate IMidiStateTracking and IMidiRingbuffer
//! functionality to an internal MidiPortBase instance.
//!
//! Ported from C++ MidiPort.h/cpp

use std::sync::atomic::{AtomicPtr, AtomicU32};

use crate::midi_port_base::MidiPortBase;

/// Buffer pointers stored atomically for thread safety
pub struct MidiPortBuffers {
    // Buffer which the port will read from and write into the ringbuffer
    pub write_data_into_port_buffer: AtomicPtr<()>,
    // Buffer which the port will read from for output
    pub read_output_data_buffer: AtomicPtr<()>,
    // Internal buffer for reading input data
    pub internal_read_input_data_buffer: AtomicPtr<()>,
    // Internal buffer for writing output data
    pub internal_write_output_data_to_buffer: AtomicPtr<()>,
}

impl Default for MidiPortBuffers {
    fn default() -> Self {
        MidiPortBuffers {
            write_data_into_port_buffer: AtomicPtr::new(std::ptr::null_mut()),
            read_output_data_buffer: AtomicPtr::new(std::ptr::null_mut()),
            internal_read_input_data_buffer: AtomicPtr::new(std::ptr::null_mut()),
            internal_write_output_data_to_buffer: AtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

/// MidiPort - Main MIDI port implementation.
///
/// Maintains full backward compatibility with the C++ implementation.
/// Contains a MidiPortBase for core logic and atomic buffer pointers.
#[allow(dead_code)]
pub struct MidiPort {
    // Composed core logic holder (contains mute state, ringbuffer, state tracking)
    base: MidiPortBase,

    // Buffer pointers (atomic for thread safety)
    buffers: MidiPortBuffers,

    // Event counters for tracking
    n_input_events: AtomicU32,
    n_output_events: AtomicU32,
}

impl MidiPort {
    /// Create a new MidiPort with tracking configuration
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        MidiPort {
            base: MidiPortBase::new_with_flags(track_notes, track_controls, track_programs),
            buffers: MidiPortBuffers::default(),
            n_input_events: AtomicU32::new(0),
            n_output_events: AtomicU32::new(0),
        }
    }

    /// Get reference to the base (for delegating methods)
    pub fn base(&self) -> &MidiPortBase {
        &self.base
    }

    /// Get mutable reference to the base
    pub fn base_mut(&mut self) -> &mut MidiPortBase {
        &mut self.base
    }

    // State tracking methods (delegated to composed MidiPortBase)
    pub fn reset_n_input_events(&self) {
        self.base.reset_n_input_events();
    }

    pub fn get_n_input_events(&self) -> u32 {
        self.base.get_n_input_events()
    }

    pub fn input_event_count(&self) -> u32 {
        self.base.input_event_count_direct()
    }

    pub fn reset_n_output_events(&self) {
        self.base.reset_n_output_events();
    }

    pub fn get_n_output_events(&self) -> u32 {
        self.base.get_n_output_events()
    }

    pub fn output_event_count(&self) -> u32 {
        self.base.output_event_count_direct()
    }

    pub fn n_notes_active(&self) -> u32 {
        self.base.n_notes_active_direct()
    }

    pub fn get_n_input_notes_active(&self) -> u32 {
        // Delegate to base's midi_state_tracker
        self.base.n_notes_active_direct()
    }

    pub fn get_n_output_notes_active(&self) -> u32 {
        if self.base.get_muted() {
            0
        } else {
            self.base.n_notes_active_direct()
        }
    }

    // Mute state
    pub fn set_muted(&self, muted: bool) {
        self.base.set_muted(muted);
    }

    pub fn get_muted(&self) -> bool {
        self.base.get_muted()
    }

    // Ringbuffer methods (delegated to composed MidiPortBase)
    pub fn set_ringbuffer_n_samples(&mut self, n: u32) {
        self.base.set_n_samples_direct(n);
    }

    pub fn get_ringbuffer_n_samples(&self) -> u32 {
        self.base.get_n_samples_direct()
    }

    pub fn get_n_samples(&self) -> u32 {
        self.get_ringbuffer_n_samples()
    }

    pub fn get_current_n_samples(&self) -> u32 {
        self.get_ringbuffer_n_samples()
    }

    /// Increment input event counter
    pub fn increment_input_events(&self, count: u32) {
        self.base.increment_input_events(count);
    }

    /// Increment output event counter
    pub fn increment_output_events(&self, count: u32) {
        self.base.increment_output_events(count);
    }

    // Process state from a MIDI message (for FFI)
    pub unsafe fn process_msg_to_state(&mut self, data: *const u8, size: usize) {
        self.base.process_msg_to_state_raw(data, size);
    }

    /// Get raw pointer to base for CXX bridge
    pub fn base_ptr(&mut self) -> *mut MidiPortBase {
        &mut self.base as *mut MidiPortBase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_port_creation() {
        let port = MidiPort::new(true, true, true);
        assert!(!port.get_muted());
        assert_eq!(port.n_notes_active(), 0);
    }

    #[test]
    fn test_mute_state() {
        let port = MidiPort::new(true, true, true);

        assert!(!port.get_muted());
        port.set_muted(true);
        assert!(port.get_muted());
        port.set_muted(false);
        assert!(!port.get_muted());
    }

    #[test]
    fn test_event_counters() {
        let port = MidiPort::new(true, true, true);

        assert_eq!(port.get_n_input_events(), 0);
        assert_eq!(port.get_n_output_events(), 0);

        port.increment_input_events(5);
        port.increment_output_events(10);

        assert_eq!(port.get_n_input_events(), 5);
        assert_eq!(port.get_n_output_events(), 10);

        port.reset_n_input_events();
        assert_eq!(port.get_n_input_events(), 0);

        port.reset_n_output_events();
        assert_eq!(port.get_n_output_events(), 0);
    }

    #[test]
    fn test_ringbuffer_operations() {
        let mut port = MidiPort::new(true, true, true);

        assert_eq!(port.get_ringbuffer_n_samples(), 0);

        port.set_ringbuffer_n_samples(1024);
        assert_eq!(port.get_ringbuffer_n_samples(), 1024);
    }

    #[test]
    fn test_notes_active() {
        let port = MidiPort::new(true, true, true);
        assert_eq!(port.n_notes_active(), 0);
    }

    #[test]
    fn test_output_notes_active_muted() {
        let port = MidiPort::new(true, true, true);
        assert_eq!(port.get_n_output_notes_active(), 0);

        port.set_muted(true);
        assert_eq!(port.get_n_output_notes_active(), 0); // Should be 0 when muted
    }
}
