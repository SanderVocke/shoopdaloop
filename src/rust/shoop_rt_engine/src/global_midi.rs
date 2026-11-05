//! Global controls retain values while audio DSP sleeps and never enter capture.
use alloc::boxed::Box;
use rt_runtime::{buffer::MidiBuffer, Error};
use shoop_rt_protocol::processors::midi::PendingControls;

use shoop_rt_protocol::processors::GlobalMidiDiagnostics as Diagnostics;
pub(crate) struct GlobalMidi {
    pending: Box<PendingControls>,
    current: MidiBuffer,
    merged: MidiBuffer,
    diagnostics: Diagnostics,
}
impl GlobalMidi {
    pub fn storage_bytes(events: usize, bytes: usize) -> Result<usize, Error> {
        events
            .checked_add(rt_runtime::midi::ResetState::EVENTS)
            .and_then(|n| n.checked_mul(core::mem::size_of::<rt_runtime::buffer::Event>()))
            .and_then(|n| n.checked_add(bytes))
            .and_then(|n| n.checked_add(rt_runtime::midi::ResetState::BYTES))
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(core::mem::size_of::<PendingControls>()))
            .and_then(|n| n.checked_add(core::mem::size_of::<Self>()))
            .and_then(|n| n.checked_add(5 * 64))
            .ok_or(Error::Capacity)
    }
    pub fn new(events: usize, bytes: usize) -> Result<Self, Error> {
        Ok(Self {
            pending: Box::new(PendingControls::default()),
            current: MidiBuffer::new(events, bytes)?,
            merged: MidiBuffer::new(events, bytes)?,
            diagnostics: Diagnostics::default(),
        })
    }
    pub fn reset(&mut self) {
        *self.pending = PendingControls::default();
        self.current.clear();
        self.merged.clear();
        self.diagnostics = Diagnostics::default();
    }
    pub fn output(&self) -> &MidiBuffer {
        &self.merged
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
        self.merged.clear();
        if !active {
            if !ordinary.is_recovery() {
                for index in 0..ordinary.len() {
                    let (offset, data) = ordinary.get(index).ok_or(Error::InvalidEvent)?;
                    self.merged.insert(offset, frames, data)?;
                }
            }
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
            .merged
            .remaining_events()
            .checked_sub(ordinary_events)
            .ok_or(Error::Full)?;
        let available_bytes = self
            .merged
            .remaining_bytes()
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
                if self.merged.len() == pending_events
                    || data.len() > pending_bytes - self.merged.byte_len()
                {
                    break;
                }
                self.merged.push(0, frames, data)?;
                self.pending.clear(data);
                self.diagnostics.drained = self.diagnostics.drained.saturating_add(1);
            }
        }
        if !ordinary.is_recovery() {
            for index in 0..ordinary.len() {
                let (offset, data) = ordinary.get(index).ok_or(Error::InvalidEvent)?;
                self.merged.insert(offset, frames, data)?;
            }
        }
        for index in 0..self.current.len() {
            let (offset, data) = self.current.get(index).ok_or(Error::InvalidEvent)?;
            self.merged.insert(offset, frames, data)?;
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
        let mut global = GlobalMidi::new(8, 64).unwrap();
        let mut current = MidiBuffer::new(8, 64).unwrap();
        let mut ordinary = MidiBuffer::new(8, 64).unwrap();
        current.push(3, 16, &[0xb0, 7, 1]).unwrap();
        current.push(4, 16, &[0xb0, 7, 2]).unwrap();
        current.push(5, 16, &[0x90, 60, 100]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, false)).unwrap();
        assert!(global.output().is_empty());
        current.clear();
        ordinary.push(0, 16, &[0xb0, 9, 3]).unwrap();
        current.push(0, 16, &[0xb0, 11, 4]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        for (i, data) in [[0xb0, 7, 2], [0xb0, 9, 3], [0xb0, 11, 4]]
            .iter()
            .enumerate()
        {
            assert_eq!(global.output().get(i), Some((0, &data[..])));
        }
        assert_eq!(global.diagnostics.rejected, 1);
        assert_eq!(global.diagnostics.overwritten, 1);
        assert!(global.pending.is_empty());
    }
    #[shoop_test]
    fn ordinary_and_current_controls_reserve_capacity_before_deferred_values() {
        let mut global = GlobalMidi::new(2, 6).unwrap();
        let mut current = MidiBuffer::new(4, 16).unwrap();
        let mut ordinary = MidiBuffer::new(4, 16).unwrap();
        current.push(0, 16, &[0xb0, 1, 1]).unwrap();
        global.render(&current, &ordinary, 16, false).unwrap();
        current.clear();
        ordinary.push(5, 16, &[0x90, 60, 100]).unwrap();
        current.push(6, 16, &[0xb0, 2, 2]).unwrap();
        current.push(7, 16, &[0xb0, 3, 3]).unwrap();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        assert_eq!(global.output().get(0), Some((5, &[0x90, 60, 100][..])));
        assert_eq!(global.output().get(1), Some((6, &[0xb0, 2, 2][..])));
        assert_eq!(global.pending.len(), 2);
        current.clear();
        ordinary.clear();
        assert_no_heap(|| global.render(&current, &ordinary, 16, true)).unwrap();
        assert_eq!(global.output().get(0), Some((0, &[0xb0, 1, 1][..])));
        assert_eq!(global.output().get(1), Some((0, &[0xb0, 3, 3][..])));
        assert!(global.pending.is_empty());
        assert_no_heap(|| global.reset());
        assert_eq!(global.diagnostics, Diagnostics::default());
    }
}
