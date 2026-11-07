//! Global controls retain values while audio DSP sleeps and never enter capture.
use alloc::boxed::Box;
use rt_runtime::{
    buffer::{MidiBuffer, MidiCapacity},
    Error,
};
use shoop_rt_protocol::processors::midi::PendingControls;

use shoop_rt_protocol::processors::GlobalMidiDiagnostics as Diagnostics;
pub(crate) struct GlobalMidi {
    pending: Box<PendingControls>,
    current: MidiBuffer,
    deferred: MidiBuffer,
    capacity: MidiCapacity,
    diagnostics: Diagnostics,
}
impl GlobalMidi {
    fn capacities(capacity: MidiCapacity, global: MidiCapacity) -> (MidiCapacity, MidiCapacity) {
        use shoop_rt_protocol::processors::midi::CONTROL_KEYS;
        (
            MidiCapacity {
                events: capacity.events.min(global.events),
                bytes: capacity.bytes.min(global.bytes),
            },
            MidiCapacity {
                events: capacity.events.min(CONTROL_KEYS),
                bytes: capacity.bytes.min(3 * CONTROL_KEYS),
            },
        )
    }
    pub fn storage_bytes(capacity: MidiCapacity, global: MidiCapacity) -> Result<usize, Error> {
        let (current, deferred) = Self::capacities(capacity, global);
        current
            .prepared_bytes()?
            .checked_add(deferred.prepared_bytes()?)
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<PendingControls>()))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() + 5 * 64))
            .ok_or(Error::Capacity)
    }
    pub fn new(capacity: MidiCapacity, global: MidiCapacity) -> Result<Self, Error> {
        let (current, deferred) = Self::capacities(capacity, global);
        Ok(Self {
            pending: Box::new(PendingControls::default()),
            current: MidiBuffer::new(current.events, current.bytes)?,
            deferred: MidiBuffer::new(deferred.events, deferred.bytes)?,
            capacity,
            diagnostics: Diagnostics::default(),
        })
    }
    pub fn reset(&mut self) {
        *self.pending = PendingControls::default();
        self.current.clear();
        self.deferred.clear();
        self.diagnostics = Diagnostics::default();
    }
    pub fn controls(&self) -> rt_buffered::MidiControls<'_> {
        rt_buffered::MidiControls {
            deferred: &self.deferred,
            current: &self.current,
        }
    }
    #[cfg(test)]
    fn output<'a>(&'a self, ordinary: &'a MidiBuffer) -> rt_buffered::MidiInput<'a> {
        rt_buffered::MidiInput::new(ordinary, Some(self.controls()))
    }
    pub fn merge_into(&self, ordinary: &mut MidiBuffer, frames: usize) -> Result<(), Error> {
        if ordinary.is_recovery() {
            ordinary.clear();
        }
        for index in (0..self.deferred.len()).rev() {
            let (offset, data) = self.deferred.get(index).ok_or(Error::InvalidState)?;
            ordinary.insert_before(offset, frames, data)?;
        }
        for index in 0..self.current.len() {
            let (offset, data) = self.current.get(index).ok_or(Error::InvalidState)?;
            ordinary.insert(offset, frames, data)?;
        }
        Ok(())
    }
    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            pending: self.pending.len() as u16,
            ..self.diagnostics
        }
    }
    pub fn render(
        &mut self,
        global: &MidiBuffer,
        ordinary: &MidiBuffer,
        frames: usize,
        active: bool,
    ) -> Result<(), Error> {
        self.current.clear();
        self.deferred.clear();
        for index in 0..global.len() {
            let (_, data) = global.get(index).ok_or(Error::InvalidEvent)?;
            if data.len() == 3 && data[0] & 0xf0 == 0xb0 && data[1] < 128 && data[2] < 128 {
                self.diagnostics.latest_cc = Some([data[0], data[1], data[2]]);
            }
        }
        if !active {
            for index in 0..global.len() {
                let (_, data) = global.get(index).ok_or(Error::InvalidEvent)?;
                self.store(data);
            }
            return Ok(());
        }
        let ordinary_events = if ordinary.is_recovery() {
            0
        } else {
            ordinary.len()
        };
        let ordinary_bytes = if ordinary.is_recovery() {
            0
        } else {
            ordinary.byte_len()
        };
        let available_events = self
            .capacity
            .events
            .checked_sub(ordinary_events)
            .ok_or(Error::Full)?;
        let available_bytes = self
            .capacity
            .bytes
            .checked_sub(ordinary_bytes)
            .ok_or(Error::Full)?;
        let (mut events, mut bytes) = (available_events, available_bytes);
        for index in 0..global.len() {
            let (offset, data) = global.get(index).ok_or(Error::InvalidEvent)?;
            if !PendingControls::supports(data) {
                self.diagnostics.rejected = self.diagnostics.rejected.saturating_add(1);
                continue;
            }
            self.pending.clear(data);
            if events > 0 && bytes >= data.len() {
                self.current.push(offset, frames, data)?;
                events -= 1;
                bytes -= data.len();
            }
        }
        let pending_events = available_events - self.current.len();
        let pending_bytes = available_bytes - self.current.byte_len();
        let mut cursor = 0;
        if !self.pending.is_empty() {
            while let Some(message) = self.pending.next(&mut cursor) {
                let data = message.data();
                if self.deferred.len() == pending_events
                    || data.len() > pending_bytes - self.deferred.byte_len()
                {
                    break;
                }
                self.deferred.push(0, frames, data)?;
                self.pending.clear(data);
                self.diagnostics.drained = self.diagnostics.drained.saturating_add(1);
            }
        }
        let (mut events, mut bytes) = (available_events, available_bytes);
        for index in 0..global.len() {
            let (_, data) = global.get(index).ok_or(Error::InvalidEvent)?;
            if !PendingControls::supports(data) {
                continue;
            }
            if events > 0 && bytes >= data.len() {
                events -= 1;
                bytes -= data.len();
            } else {
                self.store(data);
                self.diagnostics.deferred = self.diagnostics.deferred.saturating_add(1);
            }
        }
        Ok(())
    }
    fn store(&mut self, data: &[u8]) {
        match self.pending.store(data) {
            Some(true) => {
                self.diagnostics.overwritten = self.diagnostics.overwritten.saturating_add(1)
            }
            Some(false) => {}
            None => self.diagnostics.rejected = self.diagnostics.rejected.saturating_add(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    #[shoop_test]
    fn deferred_controls_precede_ordinary_then_current_controls_at_equal_offsets() {
        let mut global = GlobalMidi::new(
            MidiCapacity {
                events: 8,
                bytes: 64,
            },
            MidiCapacity {
                events: 8,
                bytes: 64,
            },
        )
        .unwrap();
        let mut current = MidiBuffer::new(8, 64).unwrap();
        let mut ordinary = MidiBuffer::new(8, 64).unwrap();
        current.push(3, 16, &[0xb0, 7, 1]).unwrap();
        current.push(4, 16, &[0xb0, 7, 2]).unwrap();
        current.push(5, 16, &[0x90, 60, 100]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, false)).unwrap();
        assert!(global.output(&ordinary).is_empty());
        current.clear();
        ordinary.push(0, 16, &[0xb0, 9, 3]).unwrap();
        current.push(0, 16, &[0xb0, 11, 4]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        for (i, data) in [[0xb0, 7, 2], [0xb0, 9, 3], [0xb0, 11, 4]]
            .iter()
            .enumerate()
        {
            assert_eq!(global.output(&ordinary).iter().nth(i), Some((0, &data[..])));
        }
        assert_eq!(global.diagnostics.rejected, 1);
        assert_eq!(global.diagnostics.overwritten, 1);
        assert!(global.pending.is_empty());
    }
    #[shoop_test]
    fn ordinary_and_current_controls_reserve_capacity_before_deferred_values() {
        let mut global = GlobalMidi::new(
            MidiCapacity {
                events: 2,
                bytes: 6,
            },
            MidiCapacity {
                events: 4,
                bytes: 16,
            },
        )
        .unwrap();
        let mut current = MidiBuffer::new(4, 16).unwrap();
        let mut ordinary = MidiBuffer::new(4, 16).unwrap();
        current.push(0, 16, &[0xb0, 1, 1]).unwrap();
        global.render(&current, &ordinary, 16, false).unwrap();
        current.clear();
        ordinary.push(5, 16, &[0x90, 60, 100]).unwrap();
        current.push(6, 16, &[0xb0, 2, 2]).unwrap();
        current.push(7, 16, &[0xb0, 3, 3]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        assert_eq!(
            global.output(&ordinary).iter().nth(0),
            Some((5, &[0x90, 60, 100][..]))
        );
        assert_eq!(
            global.output(&ordinary).iter().nth(1),
            Some((6, &[0xb0, 2, 2][..]))
        );
        assert_eq!(global.pending.len(), 2);
        current.clear();
        ordinary.clear();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        assert_eq!(
            global.output(&ordinary).iter().nth(0),
            Some((0, &[0xb0, 1, 1][..]))
        );
        assert_eq!(
            global.output(&ordinary).iter().nth(1),
            Some((0, &[0xb0, 3, 3][..]))
        );
        assert!(global.pending.is_empty());
        assert_no_heap(|| global.reset());
        assert_eq!(global.diagnostics, Diagnostics::default());
    }
    #[shoop_test]
    fn external_in_place_merge_matches_the_borrowed_stream_with_equal_time_controls() {
        let mut global = GlobalMidi::new(
            MidiCapacity {
                events: 8,
                bytes: 64,
            },
            MidiCapacity {
                events: 8,
                bytes: 64,
            },
        )
        .unwrap();
        let mut current = MidiBuffer::new(8, 64).unwrap();
        let mut ordinary = MidiBuffer::new(8, 64).unwrap();
        current.push(0, 16, &[0xb0, 7, 1]).unwrap();
        global.render(&current, &ordinary, 16, false).unwrap();
        current.clear();
        current.push(0, 16, &[0xb0, 11, 4]).unwrap();
        ordinary.push(0, 16, &[0xb0, 9, 3]).unwrap();
        ordinary.push(7, 16, &[0x90, 60, 100]).unwrap();
        assert_no_heap(|| {
            global.render(&current, &ordinary, 16, true).unwrap();
            let expected = [
                (0, [0xb0, 7, 1]),
                (0, [0xb0, 9, 3]),
                (0, [0xb0, 11, 4]),
                (7, [0x90, 60, 100]),
            ];
            for ((offset, packet), (at, data)) in global.output(&ordinary).iter().zip(expected) {
                assert_eq!((offset, packet), (at, &data[..]));
            }
            global.merge_into(&mut ordinary, 16).unwrap();
            assert_eq!(ordinary.len(), expected.len());
            for (index, (at, data)) in expected.into_iter().enumerate() {
                assert_eq!(ordinary.get(index), Some((at, &data[..])));
            }
        });
    }
}
