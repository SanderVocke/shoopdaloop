//! Reusable, bounded MIDI storage for native plugin transport.

use crate::MidiEvent;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiBufferError {
    Events,
    Bytes,
}

impl fmt::Display for MidiBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Events => "MIDI event capacity exceeded",
            Self::Bytes => "MIDI event byte capacity exceeded",
        })
    }
}

impl std::error::Error for MidiBufferError {}

#[derive(Debug)]
pub struct PreparedMidiBuffer {
    events: Vec<MidiEvent>,
    count: usize,
}

impl PreparedMidiBuffer {
    pub fn new(event_capacity: usize, event_byte_capacity: usize) -> Self {
        Self {
            events: (0..event_capacity)
                .map(|_| MidiEvent {
                    frame_offset: 0,
                    data: Vec::with_capacity(event_byte_capacity),
                })
                .collect(),
            count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.count = 0;
    }

    pub fn push(&mut self, frame_offset: u32, data: &[u8]) -> Result<(), MidiBufferError> {
        let event = self
            .events
            .get_mut(self.count)
            .ok_or(MidiBufferError::Events)?;
        if data.len() > event.data.capacity() {
            return Err(MidiBufferError::Bytes);
        }
        event.frame_offset = frame_offset;
        event.data.clear();
        event.data.extend_from_slice(data);
        self.count += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[MidiEvent] {
        &self.events[..self.count]
    }

    pub fn storage_mut(&mut self) -> (&mut [MidiEvent], &mut usize) {
        (&mut self.events, &mut self.count)
    }
}
