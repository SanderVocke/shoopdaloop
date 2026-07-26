//! MIDI port: state tracking, event metering, muting and always-on capture.
//!
//! The C++ `MidiPort` reached its four possible buffers through virtual getters,
//! which tied the base class to each driver. Here the source events and the
//! optional output sink are passed into `process`, so this holds only the
//! driver-independent behaviour.
//!
//! Two state trackers are kept: one for the port's current state, and one lagging
//! behind at the tail of the capture window, fed by messages as they age out. The
//! lagging one is what lets a retroactive recording know the state it began in.

use crate::midi_ringbuffer::MidiRingbuffer;
use crate::midi_sorting_buffer::MidiSortingBuffer;
use crate::midi_state::{MidiStateTracker, TrackWhat};
use crate::midi_storage::{MidiStorage, MidiStorageElem};
use crate::port::PortDataType;

/// Capacity of a port's capture storage, in messages.
///
/// The C++ `shoop_globals::midi_storage_size` is a byte budget (`65536 * 8`)
/// divided by the element size; expressed directly in elements here.
pub const DEFAULT_RINGBUFFER_CAPACITY_ELEMS: usize = 65536 * 8 / 12;

#[derive(Debug)]
pub struct MidiPort {
    muted: bool,
    /// `None` when nothing is being tracked.
    midi_state: Option<MidiStateTracker>,
    /// State as of the oldest message still in the capture window.
    ringbuffer_tail_state: MidiStateTracker,
    /// Created lazily, only once capture is asked for.
    ringbuffer: Option<MidiRingbuffer>,
    ringbuffer_capacity: usize,
    n_input_events: u32,
    n_output_events: u32,
}

impl MidiPort {
    pub fn new(track: TrackWhat) -> Self {
        Self {
            muted: false,
            midi_state: track.anything().then(|| MidiStateTracker::new(track)),
            ringbuffer_tail_state: MidiStateTracker::new(track),
            ringbuffer: None,
            ringbuffer_capacity: DEFAULT_RINGBUFFER_CAPACITY_ELEMS,
            n_input_events: 0,
            n_output_events: 0,
        }
    }

    /// Smaller capture storage, for tests.
    pub fn with_ringbuffer_capacity(track: TrackWhat, capacity_elems: usize) -> Self {
        let mut p = Self::new(track);
        p.ringbuffer_capacity = capacity_elems;
        p
    }

    pub fn data_type(&self) -> PortDataType {
        PortDataType::Midi
    }

    pub fn muted(&self) -> bool {
        self.muted
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
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

        let Some(events) = input else { return };
        self.n_input_events += events.len() as u32;

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
            return;
        }
        if let Some(out) = output {
            self.n_output_events += events.len() as u32;
            for e in events {
                out.write_elem(*e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::{check, let_assert};

    fn port() -> MidiPort {
        MidiPort::with_ringbuffer_capacity(TrackWhat::ALL, 64)
    }

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    fn out_times(b: &MidiSortingBuffer) -> Vec<u32> {
        b.events().unwrap().iter().map(|e| e.time).collect()
    }

    #[test]
    fn reports_its_data_type() {
        check!(port().data_type() == PortDataType::Midi);
    }

    #[test]
    fn tracks_state_of_incoming_messages() {
        let mut p = port();
        let input = [
            ev(0, &midi::note_on(0, 60, 100)),
            ev(1, &midi::cc(0, 7, 42)),
        ];
        p.process(4, Some(&input), None);
        let_assert!(Some(s) = p.midi_state());
        check!(s.note_velocity(0, 60) == Some(100));
        check!(s.cc_value(0, 7) == Some(42));
        check!(p.n_notes_active() == 1);
    }

    #[test]
    fn counts_input_events() {
        let mut p = port();
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.n_input_events() == 1);
        p.process(4, Some(&[ev(0, &midi::note_off(0, 60, 0))]), None);
        check!(p.n_input_events() == 2);
        p.reset_n_input_events();
        check!(p.n_input_events() == 0);
    }

    #[test]
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

    #[test]
    fn an_input_only_port_counts_no_output() {
        let mut p = port();
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.n_input_events() == 1);
        // No sink, so nothing was emitted -- an input port must not report its
        // own arrivals as output.
        check!(p.n_output_events() == 0);
    }

    #[test]
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

    #[test]
    fn untracked_port_has_no_state() {
        let mut p = MidiPort::with_ringbuffer_capacity(TrackWhat::NOTHING, 64);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.midi_state().is_none());
        check!(p.n_notes_active() == 0);
        // Counting still works.
        check!(p.n_input_events() == 1);
    }

    #[test]
    fn capture_is_off_until_asked_for() {
        let mut p = port();
        check!(p.ringbuffer_n_samples() == 0);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);

        let mut snap = MidiStorage::with_capacity_elems(8);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 0);
    }

    #[test]
    fn capture_retains_recent_messages() {
        let mut p = port();
        p.set_ringbuffer_n_samples(100);
        p.process(4, Some(&[ev(1, &midi::note_on(0, 60, 100))]), None);
        p.process(4, Some(&[ev(2, &midi::note_off(0, 60, 0))]), None);

        let mut snap = MidiStorage::with_capacity_elems(64);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 2);
    }

    #[test]
    fn snapshot_clears_the_target_when_capture_is_inactive() {
        let p = port();
        let mut snap = MidiStorage::with_capacity_elems(8);
        snap.append(0, &midi::note_on(0, 60, 1), false, None);
        check!(snap.n_events() == 1);
        p.snapshot_ringbuffer_into(&mut snap);
        check!(snap.n_events() == 0);
    }

    #[test]
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

    #[test]
    fn shrinking_the_capture_window_is_reflected() {
        let mut p = port();
        p.set_ringbuffer_n_samples(100);
        p.process(10, Some(&[ev(0, &midi::note_on(0, 60, 1))]), None);
        check!(p.ringbuffer_n_samples() == 100);
        p.set_ringbuffer_n_samples(5);
        check!(p.ringbuffer_n_samples() == 5);
    }

    #[test]
    fn a_cycle_with_no_input_still_ages_capture() {
        let mut p = port();
        p.set_ringbuffer_n_samples(8);
        p.process(4, None, None);
        p.process(4, None, None);
        // No events, but time advanced without incident.
        check!(p.n_input_events() == 0);
        check!(p.ringbuffer_n_samples() == 8);
    }

    #[test]
    fn output_event_count_resets() {
        let mut p = port();
        let mut out = MidiSortingBuffer::with_capacity(8);
        p.process(4, Some(&[ev(0, &midi::note_on(0, 60, 1))]), Some(&mut out));
        check!(p.n_output_events() == 1);
        p.reset_n_output_events();
        check!(p.n_output_events() == 0);
    }
}
