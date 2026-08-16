//! MIDI port fed by a driver: a fresh buffer of events each cycle.
//!
//! The dummy MIDI port is not usable for this. Its queue spans cycles and is rebased
//! by however many frames were processed, dropping anything now in the past, which is
//! right for a test that queues a whole sequence up front and wrong for a driver
//! handing over one cycle at a time.
//!
//! port yields no events at all, rather than passing them on and relying on something
//! downstream to drop them.
//!
//! A driver stages a cycle's arrivals *before* the cycle runs, and `prepare` takes
//! them. It cannot write them straight into place, because `prepare` happens partway
//! through the schedule -- ordered against the channels that read the port -- and
//! clears whatever was there, so that a cycle nobody fed reads as silence.

use crate::midi_event::MidiEvent;
use crate::midi_port::MidiPort;
use crate::midi_sorting_buffer::MidiSortingBuffer;
use crate::midi_state::TrackWhat;
use crate::midi_storage::MidiStorageElem;
use crate::port::{PortConnectability, PortDataType, PortDirection};
use crate::realtime_lock_guard::Mutex;
use std::sync::Arc;

/// Events reserved per cycle, so a normal cycle never grows either buffer.
const RESERVE: usize = 256;

#[derive(Debug)]
pub struct ExternalMidiPort {
    name: String,
    direction: PortDirection,
    midi: MidiPort,
    /// Staged by the driver before the cycle; `prepare` moves it into `incoming`.
    staged: Vec<MidiStorageElem>,
    /// Reused while rebasing events deferred into a later cycle.
    deferred: Vec<MidiStorageElem>,
    /// This cycle's arrivals, as the engine sees them.
    incoming: Vec<MidiStorageElem>,
    /// What the engine wrote this cycle, for the driver to hand to the backend.
    outgoing: MidiSortingBuffer,
    /// Accumulated output requested by the dummy frontend API across multiple
    /// controlled sub-cycles, with times rebased to the request start.
    outgoing_collected: Vec<MidiStorageElem>,
    collect_pos: u32,
    last_collect_start: u32,
    outgoing_current_collected: bool,
    output_capture: Option<Arc<Mutex<Vec<MidiEvent>>>>,
}

impl ExternalMidiPort {
    pub fn new(name: impl Into<String>, direction: PortDirection) -> Self {
        Self {
            name: name.into(),
            direction,
            midi: MidiPort::new(TrackWhat::ALL),
            staged: Vec::with_capacity(RESERVE),
            deferred: Vec::with_capacity(RESERVE),
            incoming: Vec::with_capacity(RESERVE),
            outgoing: MidiSortingBuffer::with_capacity(RESERVE),
            outgoing_collected: Vec::with_capacity(RESERVE),
            collect_pos: 0,
            last_collect_start: 0,
            outgoing_current_collected: false,
            output_capture: None,
        }
    }

    pub fn set_output_capture(&mut self, output: Arc<Mutex<Vec<MidiEvent>>>) {
        self.output_capture = Some(output);
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
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

    // A driver's port is readable or writable according to its direction, unlike the
    // dummy port, which is both so that tests can drive either side.
    pub fn has_internal_read_access(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_internal_write_access(&self) -> bool {
        self.direction == PortDirection::Output
    }
    pub fn has_implicit_input_source(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_implicit_output_sink(&self) -> bool {
        self.direction == PortDirection::Output
    }

    pub fn input_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::EXTERNAL
        } else {
            PortConnectability::INTERNAL
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::INTERNAL
        } else {
            PortConnectability::EXTERNAL
        }
    }

    // --- driver interface ---

    /// Stages one of the next cycle's arrivals, for `prepare` to pick up.
    ///
    /// Refuses an oversized or empty payload rather than storing something malformed.
    pub fn push_incoming(&mut self, time: u32, data: &[u8]) -> bool {
        if self.staged.len() >= RESERVE {
            return false;
        }
        match MidiStorageElem::new(time, data) {
            Some(e) => {
                self.staged.push(e);
                true
            }
            None => false,
        }
    }

    /// What the engine produced this cycle, ordered by time. Empty while muted.
    pub fn outgoing(&self) -> &[MidiStorageElem] {
        if self.midi.muted() {
            return &[];
        }
        self.outgoing.events().unwrap_or(&[])
    }

    pub fn clear_queues(&mut self) {
        self.staged.clear();
        self.incoming.clear();
        self.outgoing.prepare();
        self.outgoing_collected.clear();
        if let Some(output) = &self.output_capture {
            crate::realtime_allow_lock!("external MIDI capture clear", output.lock())
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
        self.collect_pos = 0;
        self.last_collect_start = 0;
        self.outgoing_current_collected = false;
    }

    pub fn request_output(&mut self) {
        self.outgoing_collected.clear();
        if let Some(output) = &self.output_capture {
            crate::realtime_allow_lock!("external MIDI capture reset", output.lock())
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
        self.collect_pos = 0;
        self.last_collect_start = 0;
        self.outgoing_current_collected = false;
    }

    fn collect_current_outgoing(&mut self, offset: u32) {
        for mut e in self.outgoing.events().unwrap_or(&[]).iter().copied() {
            e.time = e.time.saturating_add(offset);
            self.outgoing_collected.push(e);
        }
        self.outgoing_current_collected = true;
    }

    pub fn dequeue_output(&mut self) -> Vec<MidiStorageElem> {
        if !self.outgoing_current_collected {
            self.collect_current_outgoing(self.last_collect_start);
        }
        let mut out = Vec::new();
        for e in self.outgoing_collected.iter().copied() {
            if !out.iter().any(|existing: &MidiStorageElem| {
                existing.time == e.time && existing.data() == e.data()
            }) {
                out.push(e);
            }
        }
        self.outgoing_collected.clear();
        self.collect_pos = 0;
        out
    }

    // --- port interface ---

    /// Start of cycle: take whatever the driver staged, and start the output empty so
    /// nothing carries over.
    pub fn prepare(&mut self, n_frames: u32) {
        if self.direction == PortDirection::Output && !self.outgoing_current_collected {
            self.collect_current_outgoing(self.last_collect_start);
        }
        self.incoming.clear();
        self.deferred.clear();
        for mut e in self.staged.drain(..) {
            if e.time < n_frames {
                self.incoming.push(e);
            } else {
                e.time -= n_frames;
                self.deferred.push(e);
            }
        }
        std::mem::swap(&mut self.staged, &mut self.deferred);
        self.outgoing.prepare();
        self.outgoing_current_collected = false;
    }

    /// Messages the engine may read this cycle.
    pub fn visible_events(&self) -> &[MidiStorageElem] {
        if self.midi.muted() {
            return &[];
        }
        &self.incoming
    }

    pub fn write_event(&mut self, event: MidiStorageElem) {
        self.outgoing.write_elem(event);
        if self.direction == PortDirection::Output && self.outgoing_current_collected {
            let mut e = event;
            e.time = e.time.saturating_add(self.last_collect_start);
            self.outgoing_collected.push(e);
        } else {
            self.outgoing_current_collected = false;
        }
    }

    /// End of cycle: run the core over whichever side carries data, then order the
    /// output so the driver can emit it.
    pub fn process(&mut self, n_frames: u32) {
        let ExternalMidiPort {
            direction,
            midi,
            incoming,
            outgoing,
            ..
        } = self;

        // One core, fed from whichever side this port's direction makes the source,
        // so counters and note tracking follow the traffic either way.
        let events: &[MidiStorageElem] = if *direction == PortDirection::Input {
            incoming
        } else {
            outgoing.sort();
            outgoing.events().unwrap_or(&[])
        };
        midi.process(n_frames, Some(events), None);

        if *direction == PortDirection::Input {
            // Sorted regardless, so a driver handing over out-of-order arrivals still
            // presents them in time order.
            outgoing.sort();
        } else {
            self.last_collect_start = self.collect_pos;
            self.outgoing_current_collected = true;
            for mut e in outgoing.events().unwrap_or(&[]).iter().copied() {
                e.time = e.time.saturating_add(self.collect_pos);
                let data = e.data();
                let is_initial_all_sound_off = e.time == 0
                    && data.len() >= 3
                    && (data[0] & 0xf0) == 0xb0
                    && data[1] == 120
                    && data[2] == 0;
                if !is_initial_all_sound_off {
                    self.outgoing_collected.push(e);
                    if let Some(output) = &self.output_capture {
                        crate::realtime_allow_lock!(
                            "external MIDI process output capture",
                            output.lock()
                        )
                        .unwrap_or_else(|error| error.into_inner())
                        .push(MidiEvent::new(e.time as i32, data.to_vec()));
                    }
                }
            }
            self.collect_pos = self.collect_pos.saturating_add(n_frames);
        }
    }

    pub fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).expect("valid")
    }

    fn in_port() -> ExternalMidiPort {
        ExternalMidiPort::new("in", PortDirection::Input)
    }
    fn out_port() -> ExternalMidiPort {
        ExternalMidiPort::new("out", PortDirection::Output)
    }

    fn times(msgs: &[MidiStorageElem]) -> Vec<u32> {
        msgs.iter().map(|m| m.time).collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn access_follows_direction() {
        let i = in_port();
        check!(i.has_internal_read_access());
        check!(!i.has_internal_write_access());
        check!(i.has_implicit_input_source());
        check!(!i.has_implicit_output_sink());

        let o = out_port();
        check!(!o.has_internal_read_access());
        check!(o.has_internal_write_access());
        check!(!o.has_implicit_input_source());
        check!(o.has_implicit_output_sink());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_cycles_arrivals_are_visible_then_gone() {
        let mut p = in_port();

        // Staged before the cycle, as a driver does.
        check!(p.push_incoming(0, &midi::note_on(0, 60, 100)));
        check!(p.push_incoming(3, &midi::note_off(0, 60, 64)));
        p.prepare(64);
        p.process(64);

        check!(times(p.visible_events()) == vec![0, 3]);
        check!(p.midi().n_input_events() == 2);

        // The next cycle starts empty rather than inheriting anything.
        p.prepare(64);
        p.process(64);
        check!(p.visible_events().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_oversized_payload_is_refused() {
        let mut p = in_port();
        check!(!p.push_incoming(0, &[]));
        check!(!p.push_incoming(0, &[1, 2, 3, 4, 5]));
        p.prepare(64);
        check!(p.visible_events().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_muted_input_port_yields_nothing() {
        let mut p = in_port();
        p.midi_mut().set_muted(true);

        p.push_incoming(0, &midi::note_on(0, 60, 100));
        p.prepare(64);
        p.process(64);

        check!(p.visible_events().is_empty());
        // It still arrived, so it is still counted.
        check!(p.midi().n_input_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn output_is_ordered_for_the_driver() {
        let mut p = out_port();

        p.prepare(64);
        p.write_event(ev(9, &midi::note_off(0, 60, 64)));
        p.write_event(ev(1, &midi::note_on(0, 60, 100)));
        p.process(64);

        check!(times(p.outgoing()) == vec![1, 9]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_muted_output_port_emits_nothing() {
        let mut p = out_port();
        p.midi_mut().set_muted(true);

        p.prepare(64);
        p.write_event(ev(1, &midi::note_on(0, 60, 100)));
        p.process(64);

        check!(p.outgoing().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn jack_midi_input_message_counters_reset_and_muting_equivalent() {
        let mut p = in_port();
        p.push_incoming(0, &[0, 1, 2]);
        p.push_incoming(0, &[0, 1, 2]);
        p.prepare(100);
        p.process(100);
        check!(p.visible_events().len() == 2);
        check!(p.midi().n_input_events() == 2);
        check!(p.midi().n_output_events() == 0);

        p.midi_mut().reset_n_input_events();
        p.midi_mut().reset_n_output_events();
        p.prepare(100);
        p.process(100);
        check!(p.midi().n_input_events() == 0);
        check!(p.midi().n_output_events() == 0);

        p.midi_mut().set_muted(true);
        p.push_incoming(0, &[0, 1, 2]);
        p.push_incoming(0, &[0, 1, 2]);
        p.prepare(100);
        p.process(100);
        check!(p.visible_events().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn jack_midi_input_ringbuffer_snapshot_equivalent() {
        let mut p = in_port();
        p.midi_mut().set_ringbuffer_n_samples(100);
        let in_events = [
            ev(0, &midi::note_on(0, 100, 127)),
            ev(1, &midi::note_on(0, 110, 127)),
            ev(2, &midi::note_off(0, 100, 127)),
            ev(3, &midi::note_off(0, 110, 127)),
        ];
        for e in &in_events {
            p.push_incoming(e.time, e.data());
        }
        p.prepare(100);
        p.process(100);

        let mut snap = crate::midi_storage::MidiStorage::with_capacity_elems(16);
        p.midi().snapshot_ringbuffer_into(&mut snap);
        let got: Vec<_> = snap.iter().map(|e| e.data().to_vec()).collect();
        check!(
            got == in_events
                .iter()
                .map(|e| e.data().to_vec())
                .collect::<Vec<_>>()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn jack_midi_output_message_counters_reset_and_muting_equivalent() {
        let mut p = out_port();
        p.prepare(100);
        p.write_event(ev(0, &[0, 1, 2]));
        p.write_event(ev(0, &[0, 1, 2]));
        p.process(100);
        check!(p.outgoing().len() == 2);
        check!(p.midi().n_input_events() == 2);
        check!(p.midi().n_output_events() == 0);

        p.midi_mut().reset_n_input_events();
        p.midi_mut().reset_n_output_events();
        p.prepare(100);
        p.process(100);
        check!(p.midi().n_input_events() == 0);
        check!(p.midi().n_output_events() == 0);

        p.midi_mut().set_muted(true);
        p.prepare(100);
        p.write_event(ev(0, &[0, 1, 2]));
        p.write_event(ev(0, &[0, 1, 2]));
        p.process(100);
        check!(p.outgoing().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_output_ports_traffic_is_tracked() {
        let mut p = out_port();

        p.prepare(64);
        p.write_event(ev(0, &midi::note_on(0, 60, 100)));
        p.write_event(ev(1, &midi::note_on(0, 62, 100)));
        p.process(64);

        check!(p.midi().n_notes_active() == 2);

        p.prepare(64);
        p.write_event(ev(0, &midi::note_off(0, 60, 64)));
        p.write_event(ev(1, &midi::note_off(0, 62, 64)));
        p.process(64);

        check!(p.midi().n_notes_active() == 0);
    }
}
