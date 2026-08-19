//! Test MIDI port: fed from a queue, and able to capture what was written out.
//!
//! Input side: messages are queued with cycle-relative times and become visible
//! once the cycle reaches them. Anything not reached this cycle stays queued, and
//! its timestamp is shifted down as cycles advance.
//!
//! Output side: a test requests a span of frames, and messages written during it
//! are captured with times rebased to the moment of the request.

use crate::dummy_port::PortId;
use crate::midi_port::MidiPort;
use crate::midi_state::{TrackWhat, MAX_DIFF_MESSAGES};
use crate::midi_storage::{sort_by_time, MidiStorageElem};
use crate::port::{PortConnectability, PortDataType, PortDirection};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("a previous data request has not completed")]
pub struct RequestPending;

/// Messages reserved for each of the port's buffers, so a cycle that emits
/// messages does not allocate. Playback interruption may emit targeted note-offs.
const RESERVE: usize = 256;

/// Output-side reserve: a playback state restore arrives as one burst, and this
/// port stands in for a driver buffer on the audio thread, so it must not grow.
const OUT_RESERVE: usize = RESERVE + MAX_DIFF_MESSAGES;

#[derive(Debug)]
pub struct DummyMidiPort {
    id: PortId,
    name: String,
    direction: PortDirection,
    /// External input, sorted by time.
    queued: Vec<MidiStorageElem>,
    /// This cycle's buffer: written by the engine on an output port.
    buffer: Vec<MidiStorageElem>,
    /// Output captured while a request was active.
    written_requested: Vec<MidiStorageElem>,
    current_buf_frames: u32,
    n_requested_frames: u32,
    n_original_requested_frames: u32,
    n_processed_last_round: u32,
    midi: MidiPort,
}

impl DummyMidiPort {
    pub fn new(id: PortId, name: impl Into<String>, direction: PortDirection) -> Self {
        Self {
            id,
            name: name.into(),
            direction,
            queued: Vec::with_capacity(RESERVE),
            buffer: Vec::with_capacity(OUT_RESERVE),
            written_requested: Vec::with_capacity(OUT_RESERVE),
            current_buf_frames: 0,
            n_requested_frames: 0,
            n_original_requested_frames: 0,
            n_processed_last_round: 0,
            midi: MidiPort::new(TrackWhat::ALL),
        }
    }

    pub fn id(&self) -> PortId {
        self.id
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

    /// Unlike the dummy audio port, a dummy MIDI port is readable and writable in
    pub fn has_internal_read_access(&self) -> bool {
        true
    }
    pub fn has_internal_write_access(&self) -> bool {
        true
    }
    pub fn has_implicit_input_source(&self) -> bool {
        true
    }
    pub fn has_implicit_output_sink(&self) -> bool {
        true
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

    // --- test-facing controls ---

    /// Queues an external input message at a cycle-relative time.
    pub fn queue_msg(&mut self, time: u32, data: &[u8]) -> bool {
        let Some(elem) = MidiStorageElem::new(time, data) else {
            return false;
        };
        self.queued.push(elem);
        self.queued.sort_by_key(|m| m.time);
        true
    }

    pub fn queue_msg_next_cycle(&mut self, time: u32, data: &[u8]) -> bool {
        let pending_progress = self
            .n_processed_last_round
            .saturating_sub(self.n_requested_frames);
        let Some(time) = time.checked_add(pending_progress) else {
            return false;
        };
        self.queue_msg(time, data)
    }

    pub fn queue_empty(&self) -> bool {
        self.queued.is_empty()
    }

    pub fn clear_queues(&mut self) {
        self.queued.clear();
        self.written_requested.clear();
        self.n_original_requested_frames = 0;
        self.n_requested_frames = 0;
    }

    /// Asks for the next `n_frames` of output to be captured.
    ///
    /// Refuses while a previous request is still outstanding, since the capture
    /// times are rebased against a single request.
    pub fn request_data(&mut self, n_frames: u32) -> Result<(), RequestPending> {
        if self.n_requested_frames > 0 {
            return Err(RequestPending);
        }
        self.n_requested_frames = n_frames;
        self.n_original_requested_frames = n_frames;
        Ok(())
    }

    pub fn n_requested_frames(&self) -> u32 {
        self.n_requested_frames
    }

    /// Takes the captured output, with times relative to the request.
    pub fn take_written_requested_msgs(&mut self) -> Vec<MidiStorageElem> {
        std::mem::take(&mut self.written_requested)
    }

    // --- buffer interface ---

    /// Messages readable this cycle: the queued prefix that the cycle has reached,
    /// or the written buffer when nothing is queued.
    pub fn visible_events(&self) -> &[MidiStorageElem] {
        if self.queued.is_empty() {
            return &self.buffer;
        }
        let n = self
            .queued
            .iter()
            .position(|m| m.time >= self.current_buf_frames)
            .unwrap_or(self.queued.len());
        &self.queued[..n]
    }

    pub fn n_events(&self) -> usize {
        self.visible_events().len()
    }

    /// Adds a message to this cycle's buffer.
    pub fn write_event(&mut self, event: MidiStorageElem) {
        self.buffer.push(event);
    }

    pub fn buffer(&self) -> &[MidiStorageElem] {
        &self.buffer
    }

    // --- processing ---

    /// Start of cycle: clear the buffer and advance the input queue.
    ///
    /// The queue only advances by frames processed *beyond* an outstanding
    /// request, so a request holds the input queue in place while it completes.
    pub fn prepare(&mut self, n_frames: u32) {
        self.buffer.clear();
        let progress_by = self
            .n_processed_last_round
            .saturating_sub(self.n_requested_frames);
        if progress_by > 0 && !self.queued.is_empty() {
            // Messages now in the past are dropped; the rest shift down.
            self.queued.retain(|m| m.time >= progress_by);
            for m in self.queued.iter_mut() {
                m.time -= progress_by;
            }
        }
        self.n_processed_last_round = 0;
        self.current_buf_frames = n_frames;
    }

    /// End of cycle: capture requested output, then run the MIDI port core.
    pub fn process(&mut self, n_frames: u32) {
        if self.direction == PortDirection::Output {
            sort_by_time(&mut self.buffer);
            if !self.midi.muted() {
                let base = self.n_original_requested_frames - self.n_requested_frames;
                for m in &self.buffer {
                    if m.time < self.n_requested_frames {
                        self.written_requested.push(m.at_time(m.time + base));
                    }
                }
            }
        }
        self.n_processed_last_round = n_frames;
        self.n_requested_frames = self.n_requested_frames.saturating_sub(n_frames);

        // The dummy port is its own readable buffer and has no separate internal
        // output sink, so the core only tracks state and feeds capture.
        //
        // The visible range is computed first, then the fields are borrowed
        // separately, so nothing is copied on the process path.
        let n_visible = if self.queued.is_empty() {
            self.buffer.len()
        } else {
            self.queued
                .iter()
                .position(|m| m.time >= self.current_buf_frames)
                .unwrap_or(self.queued.len())
        };
        let DummyMidiPort {
            queued,
            buffer,
            midi,
            ..
        } = self;
        let events: &[MidiStorageElem] = if queued.is_empty() {
            &buffer[..n_visible]
        } else {
            &queued[..n_visible]
        };
        midi.process(n_frames, Some(events), None);
    }

    pub fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    use PortDirection as D;

    fn input_port() -> DummyMidiPort {
        DummyMidiPort::new(PortId(1), "min", D::Input)
    }
    fn output_port() -> DummyMidiPort {
        DummyMidiPort::new(PortId(2), "mout", D::Output)
    }

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    fn times(msgs: &[MidiStorageElem]) -> Vec<u32> {
        msgs.iter().map(|m| m.time).collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reports_its_identity() {
        let p = input_port();
        check!(p.id() == PortId(1));
        check!(p.name() == "min");
        check!(p.direction() == D::Input);
        check!(p.data_type() == PortDataType::Midi);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn is_readable_and_writable_in_both_directions() {
        for p in [input_port(), output_port()] {
            check!(p.has_internal_read_access());
            check!(p.has_internal_write_access());
            check!(p.has_implicit_input_source());
            check!(p.has_implicit_output_sink());
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connectability_follows_direction() {
        check!(input_port().input_connectability() == PortConnectability::EXTERNAL);
        check!(input_port().output_connectability() == PortConnectability::INTERNAL);
        check!(output_port().input_connectability() == PortConnectability::INTERNAL);
        check!(output_port().output_connectability() == PortConnectability::EXTERNAL);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn queued_messages_are_kept_in_time_order() {
        let mut p = input_port();
        check!(p.queue_empty());
        check!(p.queue_msg(5, &midi::note_on(0, 60, 1)));
        check!(p.queue_msg(1, &midi::note_on(0, 61, 1)));
        check!(!p.queue_empty());
        p.prepare(8);
        check!(times(p.visible_events()) == vec![1, 5]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn oversized_queued_messages_are_refused() {
        let mut p = input_port();
        check!(!p.queue_msg(0, &[1, 2, 3, 4, 5]));
        check!(p.queue_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn only_messages_within_the_cycle_are_visible() {
        let mut p = input_port();
        p.queue_msg(1, &midi::note_on(0, 60, 1));
        p.queue_msg(9, &midi::note_on(0, 61, 1));
        p.prepare(4);
        // Frame 9 is beyond this 4-frame cycle.
        check!(times(p.visible_events()) == vec![1]);
        check!(p.n_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn the_queue_advances_across_cycles() {
        let mut p = input_port();
        p.queue_msg(1, &midi::note_on(0, 60, 1));
        p.queue_msg(6, &midi::note_on(0, 61, 1));

        p.prepare(4);
        check!(times(p.visible_events()) == vec![1]);
        p.process(4);

        // Next cycle: times shift down by the four frames just processed, so the
        // frame-6 message is now at frame 2 and visible.
        p.prepare(4);
        check!(times(p.visible_events()) == vec![2]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn messages_falling_behind_the_queue_are_dropped() {
        let mut p = input_port();
        p.queue_msg(1, &midi::note_on(0, 60, 1));
        p.prepare(4);
        p.process(4);
        // Frame 1 is now in the past and is discarded rather than going negative.
        p.prepare(4);
        check!(p.queue_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn clear_queues_resets_everything() {
        let mut p = output_port();
        p.queue_msg(1, &midi::note_on(0, 60, 1));
        assert2::assert!(let Ok(()) = p.request_data(4));
        p.clear_queues();
        check!(p.queue_empty());
        check!(p.n_requested_frames() == 0);
        check!(p.take_written_requested_msgs().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_second_request_is_refused_while_one_is_outstanding() {
        let mut p = output_port();
        assert2::assert!(let Ok(()) = p.request_data(8));
        check!(p.request_data(4) == Err(RequestPending));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn written_output_is_captured_during_a_request() {
        let mut p = output_port();
        assert2::assert!(let Ok(()) = p.request_data(4));
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 100)));
        p.write_event(ev(3, &midi::note_off(0, 60, 0)));
        p.process(4);

        let got = p.take_written_requested_msgs();
        check!(times(&got) == vec![1, 3]);
        // And taking them empties the capture.
        check!(p.take_written_requested_msgs().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn captured_times_are_relative_to_the_request() {
        let mut p = output_port();
        assert2::assert!(let Ok(()) = p.request_data(8));
        // First half of the request.
        p.prepare(4);
        p.write_event(ev(2, &midi::note_on(0, 60, 1)));
        p.process(4);
        // Second half: a message at cycle-frame 1 is frame 5 of the request.
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 61, 1)));
        p.process(4);

        check!(times(&p.take_written_requested_msgs()) == vec![2, 5]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn output_beyond_the_request_is_not_captured() {
        let mut p = output_port();
        assert2::assert!(let Ok(()) = p.request_data(2));
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 1)));
        p.write_event(ev(3, &midi::note_on(0, 61, 1)));
        p.process(4);
        // Only the message inside the requested 2 frames.
        check!(times(&p.take_written_requested_msgs()) == vec![1]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn nothing_is_captured_without_a_request() {
        let mut p = output_port();
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 1)));
        p.process(4);
        check!(p.take_written_requested_msgs().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_muted_port_captures_nothing() {
        let mut p = output_port();
        p.midi_mut().set_muted(true);
        assert2::assert!(let Ok(()) = p.request_data(4));
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 1)));
        p.process(4);
        check!(p.take_written_requested_msgs().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_input_port_does_not_capture_written_output() {
        let mut p = input_port();
        assert2::assert!(let Ok(()) = p.request_data(4));
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 1)));
        p.process(4);
        // Capture is an output-port behaviour only.
        check!(p.take_written_requested_msgs().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn written_output_is_sorted_before_capture() {
        let mut p = output_port();
        assert2::assert!(let Ok(()) = p.request_data(8));
        p.prepare(8);
        p.write_event(ev(5, &midi::note_on(0, 60, 1)));
        p.write_event(ev(2, &midi::note_on(0, 61, 1)));
        p.process(8);
        check!(times(&p.take_written_requested_msgs()) == vec![2, 5]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prepare_clears_the_written_buffer() {
        let mut p = output_port();
        p.prepare(4);
        p.write_event(ev(1, &midi::note_on(0, 60, 1)));
        check!(p.buffer().len() == 1);
        p.prepare(4);
        check!(p.buffer().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn incoming_messages_update_port_state() {
        let mut p = input_port();
        p.queue_msg(1, &midi::note_on(0, 60, 100));
        p.prepare(4);
        p.process(4);
        assert2::assert!(let Some(s) = p.midi().midi_state());
        check!(s.note_velocity(0, 60) == Some(100));
        check!(p.midi().n_input_events() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_request_holds_the_input_queue_in_place() {
        let mut p = output_port();
        p.queue_msg(6, &midi::note_on(0, 60, 1));
        assert2::assert!(let Ok(()) = p.request_data(8));
        p.prepare(4);
        p.process(4);
        // Four frames were processed but all of them were inside the request, so
        // the input queue has not advanced yet.
        p.prepare(4);
        check!(times(p.visible_events()) == Vec::<u32>::new());
        check!(!p.queue_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn close_is_harmless() {
        let mut p = input_port();
        p.close();
        check!(p.name() == "min");
    }
}
