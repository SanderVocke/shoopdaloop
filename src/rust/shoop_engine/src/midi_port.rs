//! MIDI port: state tracking, event metering, muting and always-on capture.
//!
//! which tied the base class to each driver. Here the source events and the
//! optional output sink are passed into `process`, so this holds only the
//! driver-independent behaviour.
//!
//! Two state trackers are kept: one for the port's current state, and one lagging
//! behind at the tail of the capture window, fed by messages as they age out. The
//! lagging one is what lets a retroactive recording know the state it began in.

use crate::midi_ringbuffer::MidiRingbuffer;
use crate::midi_sorting_buffer::MidiSortingBuffer;
use crate::midi_state::{MidiStateTracker, TrackWhat, MAX_DIFF_MESSAGES};
use crate::midi_storage::{MidiStorage, MidiStorageElem};
use crate::port::PortDataType;
use crate::state::LatestMidiMessage;
use crate::state_mirror::MidiPortStateMirror;
use std::sync::Arc;

/// Capacity of a port's capture storage, in messages.
///
/// divided by the element size; expressed directly in elements here.
pub const DEFAULT_RINGBUFFER_CAPACITY_ELEMS: usize = 65536 * 8 / 12;

#[derive(Debug)]
pub struct MidiPort {
    muted: bool,
    passthrough_muted: bool,
    /// `None` when nothing is being tracked.
    midi_state: Option<MidiStateTracker>,
    /// State as of the oldest message still in the capture window.
    ringbuffer_tail_state: MidiStateTracker,
    /// Notes that have actually crossed this port's passthrough connections.
    passthrough_state: MidiStateTracker,
    passthrough_cleanup: Vec<MidiStorageElem>,
    passthrough_cleanup_pending: bool,
    /// Created lazily, only once capture is asked for.
    ringbuffer: Option<MidiRingbuffer>,
    ringbuffer_capacity: usize,
    n_input_events: u32,
    n_output_events: u32,
    latest_input_message: Option<LatestMidiMessage>,
    state: Arc<MidiPortStateMirror>,
}

impl MidiPort {
    pub fn new(track: TrackWhat) -> Self {
        Self {
            muted: false,
            passthrough_muted: false,
            midi_state: track.anything().then(|| MidiStateTracker::new(track)),
            ringbuffer_tail_state: MidiStateTracker::new(track),
            passthrough_state: MidiStateTracker::new(TrackWhat::ALL),
            passthrough_cleanup: Vec::with_capacity(MAX_DIFF_MESSAGES),
            passthrough_cleanup_pending: false,
            ringbuffer: None,
            ringbuffer_capacity: DEFAULT_RINGBUFFER_CAPACITY_ELEMS,
            n_input_events: 0,
            n_output_events: 0,
            latest_input_message: None,
            state: Arc::new(MidiPortStateMirror::default()),
        }
    }

    /// Smaller capture storage, for tests.
    pub fn with_ringbuffer_capacity(track: TrackWhat, capacity_elems: usize) -> Self {
        let mut p = Self::new(track);
        p.ringbuffer_capacity = capacity_elems;
        p
    }

    pub fn set_state_mirror(&mut self, state: Arc<MidiPortStateMirror>) {
        self.state = state;
        self.publish_state();
    }

    pub(crate) fn adopt_state_mirror(&mut self, state: Arc<MidiPortStateMirror>) {
        (self.muted, self.passthrough_muted) = state.control_values();
        self.state = state;
        self.publish_runtime_state();
    }

    fn publish_state(&self) {
        self.state.publish_values(
            self.n_notes_active(),
            0,
            self.muted,
            self.passthrough_muted,
            self.ringbuffer_n_samples(),
        );
    }

    fn publish_runtime_state(&self) {
        self.state
            .publish_runtime_values(self.n_notes_active(), 0, self.ringbuffer_n_samples());
    }

    pub fn data_type(&self) -> PortDataType {
        PortDataType::Midi
    }

    pub fn muted(&self) -> bool {
        self.muted
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.state.set_muted(muted);
    }
    pub fn passthrough_muted(&self) -> bool {
        self.passthrough_muted
    }
    pub fn set_passthrough_muted(&mut self, muted: bool) {
        if muted && !self.passthrough_muted {
            self.passthrough_state
                .all_notes_off_into(&mut self.passthrough_cleanup);
            self.passthrough_cleanup_pending = !self.passthrough_cleanup.is_empty();
        }
        self.passthrough_muted = muted;
        self.state.set_passthrough_muted(muted);
    }

    pub(crate) fn record_passthrough(&mut self, events: &[MidiStorageElem]) {
        for event in events {
            self.passthrough_state.process(event.data());
        }
    }

    pub(crate) fn take_passthrough_cleanup(&mut self) -> Option<Vec<MidiStorageElem>> {
        self.passthrough_cleanup_pending
            .then(|| std::mem::take(&mut self.passthrough_cleanup))
    }

    pub(crate) fn finish_passthrough_cleanup(&mut self, mut cleanup: Vec<MidiStorageElem>) {
        cleanup.clear();
        self.passthrough_cleanup = cleanup;
        self.passthrough_cleanup_pending = false;
        self.passthrough_state.clear();
    }

    pub fn n_input_events(&self) -> u32 {
        self.n_input_events
    }
    pub fn reset_n_input_events(&mut self) {
        self.n_input_events = 0;
    }
    pub fn n_output_events(&self) -> u32 {
        self.n_output_events
    }
    pub fn reset_n_output_events(&mut self) {
        self.n_output_events = 0;
    }
    pub fn latest_input_message(&self) -> Option<LatestMidiMessage> {
        self.latest_input_message
    }

    pub fn midi_state(&self) -> Option<&MidiStateTracker> {
        self.midi_state.as_ref()
    }
    pub fn ringbuffer_tail_state(&self) -> &MidiStateTracker {
        &self.ringbuffer_tail_state
    }
    pub fn n_notes_active(&self) -> u32 {
        self.midi_state.as_ref().map_or(0, |s| s.n_notes_active())
    }

    pub fn ringbuffer_n_samples(&self) -> u32 {
        self.ringbuffer.as_ref().map_or(0, |r| r.n_samples())
    }

    /// Sets the capture window, creating the storage on first use.
    pub fn set_ringbuffer_n_samples(&mut self, n: u32) {
        if n > 0 && self.ringbuffer.is_none() {
            self.ringbuffer = Some(MidiRingbuffer::with_capacity_elems(
                self.ringbuffer_capacity,
            ));
        }
        if let Some(r) = self.ringbuffer.as_mut() {
            r.set_n_samples(n);
        }
        self.publish_runtime_state();
    }

    /// Copies the capture window into `target`, rebased to start at zero.
    /// Clears `target` when no capture is active.
    pub fn snapshot_ringbuffer_into(&self, target: &mut MidiStorage) {
        match self.ringbuffer.as_ref() {
            Some(r) => r.snapshot(target, None),
            None => target.clear(),
        }
    }

    /// Advances one cycle.
    ///
    /// `input` is the port's source events, with times relative to the cycle.
    /// `output` receives them when the port is not muted and actually has a sink;
    /// an input-only port passes `None` and merely tracks state.
    pub fn process(
        &mut self,
        n_frames: u32,
        input: Option<&[MidiStorageElem]>,
        output: Option<&mut MidiSortingBuffer>,
    ) {
        // Age the capture window first, so messages leaving it update the lagging
        // state before anything new arrives.
        if let Some(r) = self.ringbuffer.as_mut() {
            let tail = &mut self.ringbuffer_tail_state;
            let mut cb = |e: &MidiStorageElem| tail.process(e.data());
            r.next_buffer(n_frames, Some(&mut cb));
        }

        let Some(events) = input else {
            self.publish_runtime_state();
            return;
        };
        if let Some(message) = events
            .last()
            .and_then(|event| LatestMidiMessage::new(event.data()))
        {
            self.latest_input_message = Some(message);
            self.state.publish_latest_input_message(message);
        }
        self.n_input_events += events.len() as u32;
        let input_count = events.len() as u32;

        for e in events {
            if let Some(s) = self.midi_state.as_mut() {
                s.process(e.data());
            }
            if let Some(r) = self.ringbuffer.as_mut() {
                let tail = &mut self.ringbuffer_tail_state;
                let mut cb = |dropped: &MidiStorageElem| tail.process(dropped.data());
                r.put(e.time, e.data(), Some(&mut cb));
            }
        }

        if self.muted {
            self.state.record_events(input_count, 0);
            self.publish_runtime_state();
            return;
        }
        let mut output_count = 0;
        if let Some(out) = output {
            self.n_output_events += events.len() as u32;
            output_count = events.len() as u32;
            for e in events {
                out.write_elem(*e);
            }
        }
        self.state.record_events(input_count, output_count);
        self.publish_runtime_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    fn port() -> MidiPort {
        MidiPort::with_ringbuffer_capacity(TrackWhat::ALL, 64)
    }

    #[shoop_wasm_test_support::shoop_test]
    fn adopted_control_state_survives_stale_runtime_publication() {
        let state = Arc::new(MidiPortStateMirror::default());
        state.set_muted(true);
        state.set_passthrough_muted(true);
        let mut p = port();
        p.adopt_state_mirror(Arc::clone(&state));

        check!(p.muted());
        check!(p.passthrough_muted());

        state.set_muted(false);
        state.set_passthrough_muted(false);
        p.process(1, None, None);
        let published = state.read(String::new());
        check!(!published.muted);
        check!(!published.passthrough_muted);
    }

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    fn out_times(b: &MidiSortingBuffer) -> Vec<u32> {
        b.events().unwrap().iter().map(|e| e.time).collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reports_its_data_type() {
        check!(port().data_type() == PortDataType::Midi);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn tracks_state_of_incoming_messages() {
        let mut p = port();
        let input = [
            ev(0, &midi::note_on(0, 60, 100)),
            ev(1, &midi::cc(0, 7, 42)),
        ];
        p.process(4, Some(&input), None);
        assert2::assert!(let Some(s) = p.midi_state());
        check!(s.note_velocity(0, 60) == Some(100));
        check!(s.cc_value(0, 7) == Some(42));
        check!(p.n_notes_active() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn counts_input_events() {
        let mut p = port();
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.n_input_events() == 1);
        p.process(4, Some(&[ev(0, &midi::note_off(0, 60, 0))]), None);
        check!(p.n_input_events() == 2);
        p.reset_n_input_events();
        check!(p.n_input_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn latest_input_message_is_exact_persistent_and_pre_mute() {
        let mut p = port();
        for data in [
            &[0xf8][..],
            &[0xc1, 7][..],
            &[0xb2, 11, 64][..],
            &[0xf0, 1, 2, 0xf7][..],
        ] {
            p.process(4, Some(&[ev(0, data)]), None);
            check!(p.latest_input_message().unwrap().data() == data);
            check!(
                p.state
                    .read(String::new())
                    .latest_input_message
                    .unwrap()
                    .data()
                    == data
            );
        }
        p.set_muted(true);
        p.process(
            4,
            Some(&[ev(0, &[0x90, 60, 1]), ev(1, &[0xb4, 19, 88])]),
            None,
        );
        check!(p.latest_input_message().unwrap().data() == [0xb4, 19, 88]);
        p.process(4, Some(&[]), None);
        check!(p.latest_input_message().unwrap().data() == [0xb4, 19, 88]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn forwards_events_to_the_output_sink() {
        let mut p = port();
        let mut out = MidiSortingBuffer::with_capacity(8);
        let input = [
            ev(1, &midi::note_on(0, 60, 1)),
            ev(3, &midi::note_off(0, 60, 0)),
        ];
        p.process(4, Some(&input), Some(&mut out));
        out.sort();
        check!(out_times(&out) == vec![1, 3]);
        check!(p.n_output_events() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_input_only_port_counts_no_output() {
        let mut p = port();
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.n_input_events() == 1);
        // No sink, so nothing was emitted -- an input port must not report its
        // own arrivals as output.
        check!(p.n_output_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn muting_stops_output_but_not_state_tracking() {
        let mut p = port();
        p.set_muted(true);
        let mut out = MidiSortingBuffer::with_capacity(8);
        p.process(
            4,
            Some(&[ev(0, &midi::note_on(0, 60, 100))]),
            Some(&mut out),
        );
        check!(out.n_events() == 0);
        check!(p.n_output_events() == 0);
        // State still follows the input, so unmuting resumes coherently.
        check!(p.n_notes_active() == 1);
        check!(p.n_input_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn muting_passthrough_queues_cleanup_for_forwarded_notes_once() {
        let mut p = port();
        p.record_passthrough(&[
            ev(0, &midi::note_on(0, 60, 100)),
            ev(1, &midi::note_on(2, 61, 100)),
        ]);

        p.set_passthrough_muted(true);
        let cleanup = p.take_passthrough_cleanup().expect("cleanup");
        check!(cleanup.len() == 2);
        check!(cleanup
            .iter()
            .any(|event| event.data() == midi::note_off(0, 60, 0).as_slice()));
        check!(cleanup
            .iter()
            .any(|event| event.data() == midi::note_off(2, 61, 0).as_slice()));
        p.finish_passthrough_cleanup(cleanup);
        check!(p.take_passthrough_cleanup().is_none());

        p.set_passthrough_muted(true);
        check!(p.take_passthrough_cleanup().is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn released_and_never_forwarded_notes_need_no_passthrough_cleanup() {
        let mut p = port();
        p.record_passthrough(&[
            ev(0, &midi::note_on(0, 60, 100)),
            ev(1, &midi::note_off(0, 60, 0)),
        ]);

        p.set_passthrough_muted(true);
        check!(p.take_passthrough_cleanup().is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn muting_then_unmuting_before_processing_preserves_pending_cleanup() {
        let mut p = port();
        p.record_passthrough(&[ev(0, &midi::note_on(0, 60, 100))]);

        p.set_passthrough_muted(true);
        p.set_passthrough_muted(false);

        let cleanup = p.take_passthrough_cleanup().expect("cleanup");
        check!(cleanup.len() == 1);
        p.finish_passthrough_cleanup(cleanup);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn untracked_port_has_no_state() {
        let mut p = MidiPort::with_ringbuffer_capacity(TrackWhat::NOTHING, 64);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.midi_state().is_none());
        check!(p.n_notes_active() == 0);
        // Counting still works.
        check!(p.n_input_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn capture_is_off_until_asked_for() {
        let mut p = port();
        check!(p.ringbuffer_n_samples() == 0);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);

        let mut snap = MidiStorage::with_capacity_elems(8);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn capture_retains_recent_messages() {
        let mut p = port();
        p.set_ringbuffer_n_samples(100);
        p.process(4, Some(&[ev(1, &midi::note_on(0, 60, 100))]), None);
        p.process(4, Some(&[ev(2, &midi::note_off(0, 60, 0))]), None);

        let mut snap = MidiStorage::with_capacity_elems(64);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn snapshot_clears_the_target_when_capture_is_inactive() {
        let p = port();
        let mut snap = MidiStorage::with_capacity_elems(8);
        snap.append(0, &midi::note_on(0, 60, 1), false, None);
        check!(snap.n_events() == 1);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn messages_ageing_out_of_capture_update_the_lagging_state() {
        let mut p = port();
        // A window of only 4 frames, so messages age out quickly.
        p.set_ringbuffer_n_samples(4);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 100))]), None);
        // The note is still inside the window, so the lagging state predates it.
        check!(p.ringbuffer_tail_state().n_notes_active() == 0);

        // Advance until it falls out; the lagging state then catches up to it.
        p.process(4, None, None);
        p.process(4, None, None);
        check!(p.ringbuffer_tail_state().n_notes_active() == 1);
        check!(p.ringbuffer_tail_state().note_velocity(0, 60) == Some(100));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn shrinking_the_capture_window_is_reflected() {
        let mut p = port();
        p.set_ringbuffer_n_samples(100);
        p.process(10, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.ringbuffer_n_samples() == 100);
        p.set_ringbuffer_n_samples(5);
        check!(p.ringbuffer_n_samples() == 5);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_cycle_with_no_input_still_ages_capture() {
        let mut p = port();
        p.set_ringbuffer_n_samples(8);
        p.process(4, None, None);
        p.process(4, None, None);
        // No events, but time advanced without incident.
        check!(p.n_input_events() == 0);
        check!(p.ringbuffer_n_samples() == 8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn output_event_count_resets() {
        let mut p = port();
        let mut out = MidiSortingBuffer::with_capacity(8);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), Some(&mut out));
        check!(p.n_output_events() == 1);
        p.reset_n_output_events();
        check!(p.n_output_events() == 0);
    }
}
