//! MIDI input port that copies its source's messages into its own buffer.
//!
//! Downstream consumers read a port, not a driver. Buffering the source here
//! means several consumers can read the same arrivals within a cycle, and the
//! copy is what a sorting stage downstream operates on.
//!
//! Muting is applied at the copy: a muted port buffers nothing, so consumers see
//! silence rather than having to check the mute themselves.

use crate::midi_port::MidiPort;
use crate::midi_state::TrackWhat;
use crate::midi_storage::MidiStorageElem;
use crate::port::PortDataType;

/// Messages reserved up front, so a cycle does not allocate.
pub const DEFAULT_RESERVE: usize = 1024;

#[derive(Debug)]
pub struct MidiBufferingInputPort {
    buffered: Vec<MidiStorageElem>,
    reserved: usize,
    n_overflows: u32,
    midi: MidiPort,
}

impl Default for MidiBufferingInputPort {
    fn default() -> Self {
        Self::with_reserve(DEFAULT_RESERVE)
    }
}

impl MidiBufferingInputPort {
    pub fn with_reserve(reserved: usize) -> Self {
        Self {
            buffered: Vec::with_capacity(reserved),
            reserved,
            n_overflows: 0,
            midi: MidiPort::new(TrackWhat::ALL),
        }
    }

    pub fn data_type(&self) -> PortDataType {
        PortDataType::Midi
    }
    pub fn midi(&self) -> &MidiPort {
        &self.midi
    }
    pub fn midi_mut(&mut self) -> &mut MidiPort {
        &mut self.midi
    }

    /// Messages buffered this cycle, in arrival order.
    pub fn events(&self) -> &[MidiStorageElem] {
        &self.buffered
    }
    pub fn n_events(&self) -> usize {
        self.buffered.len()
    }
    /// How many cycles needed more room than was reserved.
    pub fn n_overflows(&self) -> u32 {
        self.n_overflows
    }

    /// Start of cycle: discard last cycle's copy.
    pub fn prepare(&mut self, _n_frames: u32) {
        self.buffered.clear();
    }

    /// Copies the source's messages, unless muted, then runs the port core.
    pub fn process(&mut self, n_frames: u32, source: &[MidiStorageElem]) {
        if !self.midi.muted() {
            if self.buffered.len() + source.len() > self.reserved {
                self.n_overflows += 1;
            }
            self.buffered.extend_from_slice(source);
        }
        // The port core sees exactly what was buffered, so a muted port reports no
        // input events either.
        let events = std::mem::take(&mut self.buffered);
        self.midi.process(n_frames, Some(&events), None);
        self.buffered = events;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    fn times(msgs: &[MidiStorageElem]) -> Vec<u32> {
        msgs.iter().map(|m| m.time).collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn starts_empty() {
        let p = MidiBufferingInputPort::with_reserve(8);
        check!(p.n_events() == 0);
        check!(p.data_type() == PortDataType::Midi);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn buffers_the_sources_messages() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        p.process(
            4,
            &[ev(1, &midi::note_on(0, 60, 1)), ev(3, &midi::cc(0, 7, 5))],
        );
        check!(times(p.events()) == vec![1, 3]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn arrival_order_is_preserved() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        // Out-of-order input is copied as-is; sorting is a downstream concern.
        p.process(
            4,
            &[
                ev(3, &midi::note_on(0, 60, 1)),
                ev(1, &midi::note_on(0, 61, 1)),
            ],
        );
        check!(times(p.events()) == vec![3, 1]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prepare_discards_the_previous_cycle() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        p.process(4, &[ev(1, &midi::note_on(0, 60, 1))]);
        check!(p.n_events() == 1);
        p.prepare(4);
        check!(p.n_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn muting_buffers_nothing() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.midi_mut().set_muted(true);
        p.prepare(4);
        p.process(4, &[ev(1, &midi::note_on(0, 60, 100))]);
        check!(p.n_events() == 0);
        // And the port core sees no input either.
        check!(p.midi().n_input_events() == 0);
        check!(p.midi().n_notes_active() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn buffered_messages_update_port_state() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        p.process(4, &[ev(0, &midi::note_on(0, 60, 100))]);
        assert2::assert!(let Some(s) = p.midi().midi_state());
        check!(s.note_velocity(0, 60) == Some(100));
        check!(p.midi().n_input_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn exceeding_the_reservation_is_counted() {
        let mut p = MidiBufferingInputPort::with_reserve(2);
        p.prepare(4);
        p.process(
            4,
            &[
                ev(0, &midi::note_on(0, 60, 1)),
                ev(1, &midi::note_on(0, 61, 1)),
                ev(2, &midi::note_on(0, 62, 1)),
            ],
        );
        check!(p.n_events() == 3);
        check!(p.n_overflows() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn several_sources_accumulate_within_one_cycle() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        p.process(4, &[ev(1, &midi::note_on(0, 60, 1))]);
        p.process(4, &[ev(2, &midi::note_on(0, 61, 1))]);
        check!(times(p.events()) == vec![1, 2]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_empty_source_is_harmless() {
        let mut p = MidiBufferingInputPort::with_reserve(8);
        p.prepare(4);
        p.process(4, &[]);
        check!(p.n_events() == 0);
        check!(p.n_overflows() == 0);
    }
}
