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

/// A planar contiguous processor view. A stride separates ports; each view can
/// select a subrange without constructing temporary vectors of slice references.
pub struct AudioPorts<'a> {
    inputs: &'a [f32],
    outputs: &'a mut [f32],
    stride: usize,
    start: usize,
    frames: usize,
}

impl<'a> AudioPorts<'a> {
    pub fn new(
        inputs: &'a [f32],
        outputs: &'a mut [f32],
        stride: usize,
        start: usize,
        frames: usize,
    ) -> Result<Self, Error> {
        if stride == 0
            || inputs.len() % stride != 0
            || outputs.len() % stride != 0
            || start > stride
            || frames > stride - start
        {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            inputs,
            outputs,
            stride,
            start,
            frames,
        })
    }

    pub fn input_count(&self) -> usize {
        self.inputs.len() / self.stride
    }
    pub fn output_count(&self) -> usize {
        self.outputs.len() / self.stride
    }
    pub fn input(&self, port: usize) -> Option<&[f32]> {
        if port >= self.input_count() {
            return None;
        }
        let start = port * self.stride + self.start;
        Some(&self.inputs[start..start + self.frames])
    }
    pub fn output(&mut self, port: usize) -> Option<&mut [f32]> {
        if port >= self.output_count() {
            return None;
        }
        let start = port * self.stride + self.start;
        Some(&mut self.outputs[start..start + self.frames])
    }
    /// Disjoint input/output borrowing for a processor without temporary storage.
    pub fn pair(&mut self, input: usize, output: usize) -> Option<(&[f32], &mut [f32])> {
        if input >= self.input_count() || output >= self.output_count() {
            return None;
        }
        let source = input * self.stride + self.start;
        let target = output * self.stride + self.start;
        Some((
            &self.inputs[source..source + self.frames],
            &mut self.outputs[target..target + self.frames],
        ))
    }
}

/// Owned fixed-capacity MIDI/event storage. Construct and destroy outside RT;
/// clearing and appending never resize or free the backing arenas.
pub struct MidiBuffer {
    events: Box<[Event]>,
    bytes: Box<[u8]>,
    event_len: usize,
    byte_len: usize,
}

impl MidiBuffer {
    pub fn new(events: usize, bytes: usize) -> Result<Self, Error> {
        if events
            .checked_mul(size_of::<Event>())
            .is_none_or(|n| n > i32::MAX as usize)
            || bytes > i32::MAX as usize
        {
            return Err(Error::Capacity);
        }
        let mut buffer = Self {
            events: vec![Event::default(); events].into_boxed_slice(),
            bytes: vec![0; bytes].into_boxed_slice(),
            event_len: 0,
            byte_len: 0,
        };
        for event in &mut buffer.events {
            *std::hint::black_box(event) = Event::default();
        }
        for byte in &mut buffer.bytes {
            *std::hint::black_box(byte) = 0;
        }
        Ok(buffer)
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
    pub fn clear(&mut self) {
        self.event_len = 0;
        self.byte_len = 0;
    }
    pub fn get(&self, index: usize) -> Option<(u32, &[u8])> {
        if index >= self.event_len {
            return None;
        }
        let event = self.events[index];
        Some((
            event.offset,
            &self.bytes[event.start as usize..event.start as usize + event.len as usize],
        ))
    }
    pub fn push(&mut self, offset: u32, frames: usize, payload: &[u8]) -> Result<(), Error> {
        let mut view = EventBuffer {
            events: &mut self.events,
            bytes: &mut self.bytes,
            event_len: self.event_len,
            byte_len: self.byte_len,
        };
        let result = view.push(offset, frames, payload);
        self.event_len = view.event_len;
        self.byte_len = view.byte_len;
        result
    }
    /// Stable ordered insertion for fan-in. Equal-time events keep source order.
    /// Moving descriptors is bounded by event capacity; byte payloads never move.
    pub fn insert(&mut self, offset: u32, frames: usize, payload: &[u8]) -> Result<(), Error> {
        if offset as usize >= frames || payload.is_empty() {
            return Err(Error::InvalidEvent);
        }
        if self.event_len == self.events.len() || payload.len() > self.bytes.len() - self.byte_len {
            return Err(Error::Full);
        }
        let index = self.events[..self.event_len].partition_point(|event| event.offset <= offset);
        self.events.copy_within(index..self.event_len, index + 1);
        self.events[index] = Event {
            offset,
            start: self.byte_len as u32,
            len: payload.len() as u32,
        };
        self.bytes[self.byte_len..self.byte_len + payload.len()].copy_from_slice(payload);
        self.event_len += 1;
        self.byte_len += payload.len();
        Ok(())
    }
    pub(crate) fn merge(&mut self, source: &Self, frames: usize) -> Result<(), Error> {
        // Reserve the entire source before inserting: refusal never half-routes it.
        if source.len() > self.events.len() - self.event_len
            || source.byte_len() > self.bytes.len() - self.byte_len
        {
            return Err(Error::Full);
        }
        if source.events[..source.event_len]
            .iter()
            .any(|event| event.offset as usize >= frames)
        {
            return Err(Error::InvalidEvent);
        }
        for index in 0..source.len() {
            let (offset, data) = source.get(index).ok_or(Error::InvalidEvent)?;
            self.insert(offset, frames, data)?;
        }
        Ok(())
    }
}

pub struct MidiBlock<'a> {
    pub inputs: &'a [MidiBuffer],
    pub outputs: &'a mut [MidiBuffer],
}

impl AudioBlock<'_, '_> {
    pub(crate) fn clear_bounded(&mut self, channels: usize, frames: usize) {
        for output in self.outputs.iter_mut().take(channels) {
            let count = output.len().min(frames);
            output[..count].fill(0.0);
        }
    }
}
