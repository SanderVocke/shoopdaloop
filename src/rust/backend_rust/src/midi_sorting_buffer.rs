//! MIDI sorting buffer implementation in Rust.
//!
//! A buffer that stores unordered MIDI messages, sorts them efficiently
//! and outputs them in order. Used for merging MIDI from multiple sources.

use crate::midi_storage::MidiStorageElem;

/// A MIDI buffer that can store and sort MIDI messages.
pub struct MidiSortingBuffer {
    /// Storage for MIDI messages
    messages: Vec<MidiStorageElem>,
    /// Whether the buffer needs sorting
    dirty: bool,
}

impl MidiSortingBuffer {
    /// Create a new MidiSortingBuffer with default capacity
    pub fn new() -> Self {
        MidiSortingBuffer {
            messages: Vec::with_capacity(1024),
            dirty: false,
        }
    }

    /// Get the number of events in the buffer
    pub fn n_events(&self) -> u32 {
        self.messages.len() as u32
    }

    /// Get an event by index
    /// # Panics
    /// Panics if buffer is dirty (needs sorting) - call sort first
    pub fn get_event(&self, idx: u32) -> MidiStorageElem {
        if self.dirty {
            panic!("Access in merging buffer which is unsorted");
        }
        self.messages[idx as usize].clone()
    }

    /// Sort the messages by time
    pub fn sort(&mut self) {
        if self.dirty {
            self.messages.sort_by(|a, b| a.time.cmp(&b.time));
            self.dirty = false;
        }
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.messages.clear();
        self.dirty = false;
    }

    /// Write an event to the buffer
    /// Returns false if the message was dropped (size > 4)
    pub fn write_event(&mut self, event: MidiStorageElem) -> bool {
        if event.size > 4 {
            return false;
        }
        self.messages.push(event);
        self.dirty = true;
        true
    }
}

impl Default for MidiSortingBuffer {
    fn default() -> Self {
        Self::new()
    }
}
