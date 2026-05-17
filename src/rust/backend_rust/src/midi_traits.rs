//! MIDI trait definitions corresponding to C++ interfaces.
//!
//! This module defines Rust traits that mirror the C++ interfaces:
//! - IMidiStateTracking -> trait MidiStateTracking
//! - IMidiRingbuffer -> trait MidiRingbufferOps
//! - IMidiReadableBuffer -> trait MidiReadableBuffer
//! - IMidiWriteableBuffer -> trait MidiWritableBuffer

use crate::midi_storage::MidiStorageElem;

/// Corresponds to C++ IMidiStateTracking
pub trait MidiStateTracking {
    /// Returns the number of notes currently active.
    fn n_notes_active(&self) -> u32;

    /// Returns the number of input events processed.
    fn input_event_count(&self) -> u32;

    /// Returns the number of output events processed.
    fn output_event_count(&self) -> u32;
}

/// Corresponds to C++ IMidiRingbuffer
pub trait MidiRingbufferOps {
    /// Sets the number of samples to track in the ringbuffer.
    fn set_n_samples(&mut self, n: u32);

    /// Returns the number of samples being tracked.
    fn get_n_samples(&self) -> u32;

    /// Takes a snapshot of the ringbuffer contents into the provided storage.
    /// The start_offset_from_end parameter, if provided, specifies the time window
    /// from the end of the buffer (0 means use n_samples from the beginning).
    fn snapshot(&self, target: &mut MidiStorageCore, start_offset_from_end: Option<u32>);

    /// Returns the current number of samples in the ringbuffer.
    fn get_current_n_samples(&self) -> u32 {
        self.get_n_samples()
    }
}

/// Corresponds to C++ IMidiReadableBuffer
pub trait MidiReadableBuffer {
    /// Returns the number of MIDI events currently in the buffer.
    fn n_events(&self) -> u32;

    /// Retrieves a MIDI event by index. The index must be less than n_events().
    fn get_event(&self, idx: u32) -> Option<MidiStorageElem>;
}

/// Corresponds to C++ IMidiWriteableBuffer
pub trait MidiWritableBuffer {
    /// Writes a MIDI event to the buffer.
    fn write_event(&mut self, event: MidiStorageElem) -> bool;
}

// Re-export MidiStorageCore from midi_storage for convenience
pub use crate::midi_storage::MidiStorageCore;

#[cfg(test)]
mod tests {
    use super::*;

    // Test that MidiStateTracking can be implemented
    struct MockStateTracker {
        notes: u32,
        input_events: u32,
        output_events: u32,
    }

    impl MidiStateTracking for MockStateTracker {
        fn n_notes_active(&self) -> u32 {
            self.notes
        }
        fn input_event_count(&self) -> u32 {
            self.input_events
        }
        fn output_event_count(&self) -> u32 {
            self.output_events
        }
    }

    #[test]
    fn test_midi_state_tracking_trait() {
        let tracker = MockStateTracker {
            notes: 5,
            input_events: 100,
            output_events: 50,
        };

        assert_eq!(tracker.n_notes_active(), 5);
        assert_eq!(tracker.input_event_count(), 100);
        assert_eq!(tracker.output_event_count(), 50);
    }

    // Test that MidiRingbufferOps can be implemented
    struct MockRingbuffer {
        n_samples: u32,
    }

    impl MidiRingbufferOps for MockRingbuffer {
        fn set_n_samples(&mut self, n: u32) {
            self.n_samples = n;
        }

        fn get_n_samples(&self) -> u32 {
            self.n_samples
        }

        fn snapshot(&self, _target: &mut MidiStorageCore, _start_offset_from_end: Option<u32>) {
            // Mock implementation - do nothing
        }
    }

    #[test]
    fn test_midi_ringbuffer_ops_trait() {
        let mut rb = MockRingbuffer { n_samples: 0 };
        rb.set_n_samples(1024);
        assert_eq!(rb.get_n_samples(), 1024);
        assert_eq!(rb.get_current_n_samples(), 1024);
    }

    // Test that MidiReadableBuffer can be implemented
    struct MockReadableBuffer {
        events: Vec<MidiStorageElem>,
    }

    impl MidiReadableBuffer for MockReadableBuffer {
        fn n_events(&self) -> u32 {
            self.events.len() as u32
        }

        fn get_event(&self, idx: u32) -> Option<MidiStorageElem> {
            self.events.get(idx as usize).cloned()
        }
    }

    #[test]
    fn test_midi_readable_buffer_trait() {
        let buf = MockReadableBuffer {
            events: vec![
                MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap(),
                MidiStorageElem::new(200, 3, &[0x90, 0x40, 0x7F]).unwrap(),
            ],
        };

        assert_eq!(buf.n_events(), 2);
        assert!(buf.get_event(0).is_some());
        assert!(buf.get_event(1).is_some());
        assert!(buf.get_event(2).is_none());
    }

    // Test that MidiWritableBuffer can be implemented
    struct MockWritableBuffer {
        events: Vec<MidiStorageElem>,
    }

    impl MidiWritableBuffer for MockWritableBuffer {
        fn write_event(&mut self, event: MidiStorageElem) -> bool {
            self.events.push(event);
            true
        }
    }

    #[test]
    fn test_midi_writable_buffer_trait() {
        let mut buf = MockWritableBuffer { events: vec![] };
        let event = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        assert!(buf.write_event(event.clone()));
        assert_eq!(buf.events.len(), 1);
    }
}