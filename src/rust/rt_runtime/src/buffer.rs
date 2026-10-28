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
    event_capacity: usize,
    byte_capacity: usize,
    recovering: bool,
}

impl MidiBuffer {
    pub fn new(events: usize, bytes: usize) -> Result<Self, Error> {
        let reserved_events = events
            .checked_add(crate::midi::ResetState::EVENTS)
            .ok_or(Error::Capacity)?;
        let reserved_bytes = bytes
            .checked_add(crate::midi::ResetState::BYTES)
            .ok_or(Error::Capacity)?;
        if reserved_events
            .checked_mul(size_of::<Event>())
            .is_none_or(|n| n > i32::MAX as usize)
            || reserved_bytes > i32::MAX as usize
        {
            return Err(Error::Capacity);
        }
        let initial = [Event::default(); 64];
        let mut event_storage = Vec::with_capacity(reserved_events);
        while event_storage.len() < reserved_events {
            let count = (reserved_events - event_storage.len()).min(initial.len());
            event_storage.extend_from_slice(&initial[..count]);
        }
        let mut buffer = Self {
            events: event_storage.into_boxed_slice(),
            bytes: vec![0; reserved_bytes].into_boxed_slice(),
            event_len: 0,
            byte_len: 0,
            event_capacity: events,
            byte_capacity: bytes,
            recovering: false,
        };
        for chunk in std::hint::black_box(&mut *buffer.events).chunks_mut(initial.len()) {
            chunk.copy_from_slice(&initial[..chunk.len()]);
        }
        std::hint::black_box(&mut *buffer.bytes).fill(0);
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
        self.recovering = false;
    }
    pub fn is_recovery(&self) -> bool {
        self.recovering
    }
    pub(crate) fn force_reset(&mut self, offset: u32) {
        for index in 0..crate::midi::ResetState::EVENTS {
            let start = index * 3;
            self.events[index] = Event {
                offset,
                start: start as u32,
                len: 3,
            };
            self.bytes[start..start + 3].copy_from_slice(&[
                0xb0 | (index / 2) as u8,
                if index % 2 == 0 { 120 } else { 123 },
                0,
            ]);
        }
        self.event_len = crate::midi::ResetState::EVENTS;
        self.byte_len = crate::midi::ResetState::BYTES;
        self.recovering = true;
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
        if self.recovering || payload.len() > self.byte_capacity {
            return Err(Error::Full);
        }
        crate::midi::Message::parse(payload, self.byte_capacity)?;
        let mut view = EventBuffer {
            events: &mut self.events[..self.event_capacity],
            bytes: &mut self.bytes[..self.byte_capacity],
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
        if self.recovering
            || self.event_len == self.event_capacity
            || payload.len() > self.byte_capacity - self.byte_len
        {
            return Err(Error::Full);
        }
        crate::midi::Message::parse(payload, self.byte_capacity)?;
        let index = if self.event_len == 0 || self.events[self.event_len - 1].offset <= offset {
            self.event_len
        } else {
            let index =
                self.events[..self.event_len].partition_point(|event| event.offset <= offset);
            self.events.copy_within(index..self.event_len, index + 1);
            index
        };
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
        if source.events[..source.event_len]
            .iter()
            .any(|event| event.offset as usize >= frames)
        {
            return Err(Error::InvalidEvent);
        }
        self.merge_window(source, 0, frames, 0, frames)
    }

    pub(crate) fn merge_window(
        &mut self,
        source: &Self,
        source_start: usize,
        frames: usize,
        target_start: usize,
        target_frames: usize,
    ) -> Result<(), Error> {
        let source_end = source_start
            .checked_add(frames)
            .ok_or(Error::InvalidFrame)?;
        let target_end = target_start
            .checked_add(frames)
            .ok_or(Error::InvalidFrame)?;
        if target_end > target_frames || target_end > u32::MAX as usize {
            return Err(Error::InvalidFrame);
        }
        if self.recovering || source.recovering {
            self.force_reset(target_start as u32);
            return Err(Error::Full);
        }
        let events = &source.events[..source.event_len];
        let first = events.partition_point(|event| (event.offset as usize) < source_start);
        let end = events.partition_point(|event| (event.offset as usize) < source_end);
        let whole_source = first == 0 && end == source.event_len;
        let bytes: usize = if whole_source {
            source.byte_len
        } else {
            events[first..end]
                .iter()
                .map(|event| event.len as usize)
                .sum()
        };
        if end - first > self.event_capacity - self.event_len
            || bytes > self.byte_capacity - self.byte_len
        {
            self.force_reset(target_start as u32);
            return Err(Error::Full);
        }
        if whole_source && !events.is_empty() {
            let first_offset = (events[0].offset as usize - source_start + target_start) as u32;
            if self.event_len == 0 || self.events[self.event_len - 1].offset <= first_offset {
                // All source packets are validated; append preserves stable fan-in order.
                let target = &mut self.events[self.event_len..self.event_len + end];
                target.copy_from_slice(events);
                for event in target {
                    event.offset = (event.offset as usize - source_start + target_start) as u32;
                    event.start += self.byte_len as u32;
                }
                self.bytes[self.byte_len..self.byte_len + bytes]
                    .copy_from_slice(&source.bytes[..bytes]);
                self.event_len += end;
                self.byte_len += bytes;
                return Ok(());
            }
        }
        for index in first..end {
            let (offset, data) = source.get(index).ok_or(Error::InvalidEvent)?;
            let offset = offset as usize - source_start + target_start;
            self.insert(offset as u32, target_frames, data)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[global_allocator]
    static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
        shoop_wasm_test_support::heap::TrackingAllocator;

    type Packet = (u32, Vec<u8>);

    struct Model {
        packets: Vec<Packet>,
        events: usize,
        bytes: usize,
        recovering: bool,
    }

    impl Model {
        fn new(events: usize, bytes: usize) -> Self {
            Self {
                packets: Vec::new(),
                events,
                bytes,
                recovering: false,
            }
        }
        fn reset(&mut self, offset: u32) {
            self.packets = (0..16)
                .flat_map(|channel| [120, 123].map(move |cc| (offset, vec![0xb0 | channel, cc, 0])))
                .collect();
            self.recovering = true;
        }
        fn merge(&mut self, source: &Self, window: [usize; 4]) -> Result<(), Error> {
            let [start, frames, target, target_frames] = window;
            let end = start.checked_add(frames).ok_or(Error::InvalidFrame)?;
            let target_end = target.checked_add(frames).ok_or(Error::InvalidFrame)?;
            if target_end > target_frames || target_end > u32::MAX as usize {
                return Err(Error::InvalidFrame);
            }
            if self.recovering || source.recovering {
                self.reset(target as u32);
                return Err(Error::Full);
            }
            let selected: Vec<_> = source
                .packets
                .iter()
                .filter(|(offset, _)| (*offset as usize) >= start && (*offset as usize) < end)
                .map(|(offset, data)| (((*offset as usize) - start + target) as u32, data.clone()))
                .collect();
            let bytes: usize = self
                .packets
                .iter()
                .chain(&selected)
                .map(|(_, data)| data.len())
                .sum();
            if self.packets.len() + selected.len() > self.events || bytes > self.bytes {
                self.reset(target as u32);
                return Err(Error::Full);
            }
            self.packets.extend(selected);
            self.packets.sort_by_key(|packet| packet.0);
            Ok(())
        }
    }

    fn snapshot(buffer: &MidiBuffer) -> Vec<Packet> {
        (0..buffer.len())
            .map(|index| {
                let (offset, data) = buffer.get(index).unwrap();
                (offset, data.to_vec())
            })
            .collect()
    }

    #[shoop_test]
    fn bulk_preparation_and_window_merges_match_scalar_packets_through_refusal_reset_and_reuse() {
        let (_, counts) = shoop_wasm_test_support::heap::measure(|| {
            let mut data = std::hint::black_box(Vec::with_capacity(1));
            data.push(1_u8);
            data.reserve_exact(128);
            std::hint::black_box(&data);
            drop(data);
        });
        assert!(counts.allocations > 0 && counts.reallocations > 0 && counts.deallocations > 0);
        for events in [0, 1, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
            let buffer = MidiBuffer::new(events, events * 3).unwrap();
            assert_eq!(buffer.events.len(), events + 32);
            assert_eq!(buffer.bytes.len(), events * 3 + 96);
            assert!(buffer.events.iter().all(|event| *event == Event::default()));
            assert!(buffer.bytes.iter().all(|byte| *byte == 0));
            assert!(buffer.is_empty());
        }
        let mut seed = 0x913a_d4_u64;
        let mut outcomes = [0; 3];
        for capacity in [0, 1, 8, 32, 65] {
            let mut source = MidiBuffer::new(32, 512).unwrap();
            let mut target = MidiBuffer::new(capacity, capacity * 8).unwrap();
            for round in 0..256 {
                assert_no_heap(|| {
                    source.clear();
                    target.clear();
                });
                let mut from = Model::new(32, 512);
                let mut to = Model::new(capacity, capacity * 8);
                let mut sysex = vec![0x01; 25];
                sysex[0] = 0xf0;
                sysex[24] = 0xf7;
                let payloads = [vec![0xf8], vec![0xc1, 7], vec![0x90, 60, 100], sysex];
                for index in 0..round % 21 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let offset = match round % 4 {
                        0 => 0,
                        1 => index % 16,
                        2 => 15 - index % 16,
                        _ => (seed >> 32) as usize % 16,
                    } as u32;
                    let data = &payloads[index % payloads.len()];
                    assert_eq!(assert_no_heap(|| source.insert(offset, 16, data)), Ok(()));
                    from.packets.push((offset, data.clone()));
                }
                from.packets.sort_by_key(|packet| packet.0);
                assert_eq!(
                    assert_no_heap(|| source.insert(7, 16, &[0x90, 128, 1])),
                    Err(Error::InvalidEvent)
                );
                assert_eq!(snapshot(&source), from.packets);
                for index in 0..capacity.min(2) {
                    let data = [0x91, 62 + index as u8, 1];
                    let offset = (round % 16) as u32;
                    assert_eq!(assert_no_heap(|| target.insert(offset, 16, &data)), Ok(()));
                    to.packets.push((offset, data.to_vec()));
                }
                if round % 23 == 0 {
                    assert_no_heap(|| source.force_reset(2));
                    from.reset(2);
                }
                if round % 29 == 0 {
                    assert_no_heap(|| target.force_reset(3));
                    to.reset(3);
                }
                let window = match round % 8 {
                    0 => [0, 16, 0, 16],
                    1 => [0, 16, 16, 32],
                    2 => [2, 10, 1, 16],
                    3 => [5, 0, 3, 16],
                    4 => [usize::MAX, 1, 0, 16],
                    5 => [0, 1, usize::MAX, usize::MAX],
                    6 => [0, 1, u32::MAX as usize, usize::MAX],
                    _ => [0, 16, 1, 16],
                };
                let expected = to.merge(&from, window);
                let actual = assert_no_heap(|| {
                    target.merge_window(&source, window[0], window[1], window[2], window[3])
                });
                assert_eq!(actual, expected, "capacity={capacity} round={round}");
                match actual {
                    Ok(()) => outcomes[0] += 1,
                    Err(Error::Full) => outcomes[1] += 1,
                    Err(Error::InvalidFrame) => outcomes[2] += 1,
                    other => panic!("unexpected {other:?}"),
                }
                assert_eq!(
                    snapshot(&target),
                    to.packets,
                    "capacity={capacity} round={round}"
                );
                assert_eq!(target.is_recovery(), to.recovering);
                assert_eq!(
                    target.byte_len(),
                    to.packets.iter().map(|(_, data)| data.len()).sum::<usize>()
                );
                assert!(target.get(target.len()).is_none());
                assert_eq!(snapshot(&source), from.packets);
            }
        }
        assert!(outcomes.iter().all(|count| *count > 0));
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
