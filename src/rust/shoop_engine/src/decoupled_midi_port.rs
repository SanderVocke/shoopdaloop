//! MIDI port with a queue attached, so messages can be handled off the audio
//! thread.
//!
//! Used for controllers: a control surface's messages are queued on arrival and
//! read at leisure, and outgoing messages are queued by the UI and drained during
//! the next cycle. Timing is not meaningful here — these messages are events, not
//! recorded material.
//!
//! The queue is bounded. Producers push into a lock-free-style queue and
//! ignores the result, so a full queue silently loses messages; here the loss is
//! counted. Making it genuinely lock-free across threads belongs with the driver
//! work, where the thread boundary actually exists.

use std::collections::VecDeque;

use thiserror::Error;

use crate::midi_sorting_buffer::MidiSortingBuffer;
use crate::midi_storage::MidiStorageElem;
use crate::port::PortDirection;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot pop incoming messages from an output port")]
pub struct NotAnInputPort;

#[derive(Debug)]
pub struct DecoupledMidiPort {
    name: String,
    direction: PortDirection,
    queue: VecDeque<MidiStorageElem>,
    capacity: usize,
    n_dropped: u32,
}

impl DecoupledMidiPort {
    pub fn new(name: impl Into<String>, direction: PortDirection, capacity: usize) -> Self {
        Self {
            name: name.into(),
            direction,
            queue: VecDeque::with_capacity(capacity),
            capacity,
            n_dropped: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
    pub fn n_queued(&self) -> usize {
        self.queue.len()
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Messages lost to a full queue.
    pub fn n_dropped(&self) -> u32 {
        self.n_dropped
    }

    fn push(&mut self, m: MidiStorageElem) -> bool {
        if self.queue.len() >= self.capacity {
            self.n_dropped += 1;
            return false;
        }
        self.queue.push_back(m);
        true
    }

    // --- process thread ---

    /// Input side: queues this cycle's arriving messages for the control thread.
    pub fn process_incoming(&mut self, events: &[MidiStorageElem]) {
        if self.direction != PortDirection::Input {
            return;
        }
        for e in events {
            self.push(*e);
        }
    }

    /// Output side: drains everything the control thread queued into `sink`.
    ///
    /// Messages keep whatever time they were given; for controller traffic that is
    /// conventionally zero, meaning "as soon as possible".
    pub fn process_outgoing(&mut self, sink: &mut MidiSortingBuffer) {
        if self.direction != PortDirection::Output {
            return;
        }
        while let Some(m) = self.queue.pop_front() {
            sink.write_elem(m);
        }
    }

    // --- control thread ---

    /// Takes the next arrived message, if any.
    pub fn pop_incoming(&mut self) -> Result<Option<MidiStorageElem>, NotAnInputPort> {
        if self.direction != PortDirection::Input {
            return Err(NotAnInputPort);
        }
        Ok(self.queue.pop_front())
    }

    /// Queues a message to go out on the next cycle.
    ///
    /// direction check here.
    pub fn push_outgoing(&mut self, m: MidiStorageElem) -> bool {
        self.push(m)
    }

    pub fn close(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    use PortDirection as D;

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    fn input(cap: usize) -> DecoupledMidiPort {
        DecoupledMidiPort::new("ctrl-in", D::Input, cap)
    }
    fn output(cap: usize) -> DecoupledMidiPort {
        DecoupledMidiPort::new("ctrl-out", D::Output, cap)
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reports_its_identity() {
        let p = input(8);
        check!(p.name() == "ctrl-in");
        check!(p.direction() == D::Input);
        check!(p.capacity() == 8);
        check!(p.n_queued() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn incoming_messages_are_queued_then_popped_in_order() {
        let mut p = input(8);
        p.process_incoming(&[ev(0, &midi::note_on(0, 60, 1)), ev(1, &midi::cc(0, 7, 9))]);
        check!(p.n_queued() == 2);

        assert2::assert!(let Ok(Some(first)) = p.pop_incoming());
        check!(midi::is_note_on(first.data()));
        assert2::assert!(let Ok(Some(second)) = p.pop_incoming());
        check!(midi::is_cc(second.data()));
        assert2::assert!(let Ok(None) = p.pop_incoming());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_output_port_does_not_queue_arrivals() {
        let mut p = output(8);
        p.process_incoming(&[ev(0, &midi::note_on(0, 60, 1))]);
        check!(p.n_queued() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn popping_from_an_output_port_is_refused() {
        let mut p = output(8);
        check!(p.pop_incoming() == Err(NotAnInputPort));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn outgoing_messages_are_drained_into_the_sink() {
        let mut p = output(8);
        check!(p.push_outgoing(ev(0, &midi::note_on(0, 60, 100))));
        check!(p.push_outgoing(ev(0, &midi::cc(0, 7, 5))));
        check!(p.n_queued() == 2);

        let mut sink = MidiSortingBuffer::with_capacity(8);
        p.process_outgoing(&mut sink);
        check!(p.n_queued() == 0);
        sink.sort();
        check!(sink.n_events() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn draining_preserves_queue_order_for_equal_times() {
        let mut p = output(8);
        p.push_outgoing(ev(0, &midi::note_off(0, 60, 0)));
        p.push_outgoing(ev(0, &midi::note_on(0, 60, 100)));
        let mut sink = MidiSortingBuffer::with_capacity(8);
        p.process_outgoing(&mut sink);
        sink.sort();
        let evs = sink.events().unwrap();
        // The sink sorts stably, so the release still precedes the retrigger.
        check!(midi::is_note_off(evs[0].data()));
        check!(midi::is_note_on(evs[1].data()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_input_port_does_not_drain_to_a_sink() {
        let mut p = input(8);
        p.push_outgoing(ev(0, &midi::note_on(0, 60, 1)));
        let mut sink = MidiSortingBuffer::with_capacity(8);
        p.process_outgoing(&mut sink);
        check!(sink.n_events() == 0);
        // The message is still queued; it just has no way out.
        check!(p.n_queued() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_full_queue_drops_and_counts() {
        let mut p = input(2);
        p.process_incoming(&[
            ev(0, &midi::note_on(0, 60, 1)),
            ev(1, &midi::note_on(0, 61, 1)),
            ev(2, &midi::note_on(0, 62, 1)),
        ]);
        check!(p.n_queued() == 2);
        check!(p.n_dropped() == 1);
        // The oldest are kept: this is a queue, not a ring.
        assert2::assert!(let Ok(Some(m)) = p.pop_incoming());
        check!(midi::note(m.data()) == 60);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn push_outgoing_reports_a_full_queue() {
        let mut p = output(1);
        check!(p.push_outgoing(ev(0, &midi::note_on(0, 60, 1))));
        check!(!p.push_outgoing(ev(0, &midi::note_on(0, 61, 1))));
        check!(p.n_dropped() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn draining_makes_room_again() {
        let mut p = output(1);
        check!(p.push_outgoing(ev(0, &midi::note_on(0, 60, 1))));
        let mut sink = MidiSortingBuffer::with_capacity(8);
        p.process_outgoing(&mut sink);
        check!(p.push_outgoing(ev(0, &midi::note_on(0, 61, 1))));
        check!(p.n_dropped() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn close_discards_queued_messages() {
        let mut p = input(8);
        p.process_incoming(&[ev(0, &midi::note_on(0, 60, 1))]);
        p.close();
        check!(p.n_queued() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_cycle_with_no_arrivals_changes_nothing() {
        let mut p = input(8);
        p.process_incoming(&[]);
        check!(p.n_queued() == 0);
        check!(p.n_dropped() == 0);
    }
}
