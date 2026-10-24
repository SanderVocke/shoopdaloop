//! Borrowed audio blocks and caller-owned bounded event storage.

use crate::{Error, Frame};

/// Byte range in its containing event buffer. Construction is validated when a
/// packet enters the buffer; callers cannot inject arbitrary ranges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Event {
    offset: u32,
    start: u32,
    len: u32,
}

impl Event {
    pub fn offset(self) -> u32 {
        self.offset
    }
}

/// Event count and byte count are bounded independently. Payload semantics are
/// validated by MIDI admission, not by this format-independent storage primitive.
pub struct EventBuffer<'a> {
    events: &'a mut [Event],
    bytes: &'a mut [u8],
    event_len: usize,
    byte_len: usize,
}

impl<'a> EventBuffer<'a> {
    pub fn new(events: &'a mut [Event], bytes: &'a mut [u8]) -> Result<Self, Error> {
        if events.len() > u32::MAX as usize || bytes.len() > u32::MAX as usize {
            return Err(Error::Capacity);
        }
        Ok(Self {
            events,
            bytes,
            event_len: 0,
            byte_len: 0,
        })
    }

    /// Refusal does not modify either arena. Events must be appended in time order.
    pub fn push(&mut self, offset: u32, frames: usize, payload: &[u8]) -> Result<(), Error> {
        if offset as usize >= frames || payload.is_empty() {
            return Err(Error::InvalidEvent);
        }
        if self.event_len > 0 && self.events[self.event_len - 1].offset > offset {
            return Err(Error::InvalidEvent);
        }
        if self.event_len == self.events.len() || payload.len() > self.bytes.len() - self.byte_len {
            return Err(Error::Full);
        }
        let end = self.byte_len + payload.len();
        self.bytes[self.byte_len..end].copy_from_slice(payload);
        self.events[self.event_len] = Event {
            offset,
            start: self.byte_len as u32,
            len: payload.len() as u32,
        };
        self.event_len += 1;
        self.byte_len = end;
        Ok(())
    }

    pub fn get(&self, index: usize) -> Option<(u32, &[u8])> {
        if index >= self.event_len {
            return None;
        }
        let event = self.events[index];
        let start = event.start as usize;
        Some((event.offset, &self.bytes[start..start + event.len as usize]))
    }

    pub fn len(&self) -> usize {
        self.event_len
    }

    pub fn is_empty(&self) -> bool {
        self.event_len == 0
    }

    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub fn remaining_events(&self) -> usize {
        self.events.len() - self.event_len
    }

    pub fn remaining_bytes(&self) -> usize {
        self.bytes.len() - self.byte_len
    }

    pub fn clear(&mut self) {
        self.event_len = 0;
        self.byte_len = 0;
    }
}

/// Safe Rust borrowing makes input/output aliasing impossible for safe callers.
/// The frame count selects a prefix of each channel; suffixes are left untouched.
pub struct AudioBlock<'a, 'samples> {
    pub frame: Frame,
    pub frames: usize,
    pub inputs: &'a [&'samples [f32]],
    pub outputs: &'a mut [&'samples mut [f32]],
}

impl AudioBlock<'_, '_> {
    pub(crate) fn clear_bounded(&mut self, channels: usize, frames: usize) {
        for output in self.outputs.iter_mut().take(channels) {
            let count = output.len().min(frames);
            output[..count].fill(0.0);
        }
    }
}
