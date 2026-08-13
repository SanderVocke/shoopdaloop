//! Collects MIDI messages written in any order and hands them back sorted.
//!
//! Several sources can feed one output port within a cycle, so their messages
//! arrive interleaved and must be ordered by time before being emitted. Sorting
//! is stable, so messages written at the same time keep their write order —
//! which matters for note-off/note-on pairs at a loop boundary.
//!
//! Reading before sorting is a programming error and returns `None` rather than
//! silently yielding an unsorted view.

use crate::midi_state::MAX_DIFF_MESSAGES;
use crate::midi_storage::{sort_by_time, MidiStorageElem};

/// Messages reserved up front. Exceeding it means allocating on the audio
/// behaviour of printing a warning to stderr.
///
/// Sized to hold a cycle's own events plus a full playback state restore, which is
/// the largest burst a single cycle can produce.
pub const DEFAULT_CAPACITY: usize = 1024 + MAX_DIFF_MESSAGES;

#[derive(Debug)]
pub struct MidiSortingBuffer {
    messages: Vec<MidiStorageElem>,
    dirty: bool,
    reserved: usize,
    n_overflows: u32,
    n_rejected: u32,
}

impl Default for MidiSortingBuffer {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl MidiSortingBuffer {
    pub fn with_capacity(reserved: usize) -> Self {
        Self {
            messages: Vec::with_capacity(reserved),
            dirty: false,
            reserved,
            n_overflows: 0,
            n_rejected: 0,
        }
    }

    pub fn n_events(&self) -> usize {
        self.messages.len()
    }
    pub fn is_sorted(&self) -> bool {
        !self.dirty
    }
    /// How many writes exceeded the reserved capacity, forcing an allocation.
    pub fn n_overflows(&self) -> u32 {
        self.n_overflows
    }
    /// How many writes were refused for carrying an oversized payload.
    pub fn n_rejected(&self) -> u32 {
        self.n_rejected
    }

    /// Sorted messages, or `None` while unsorted.
    pub fn events(&self) -> Option<&[MidiStorageElem]> {
        if self.dirty {
            None
        } else {
            Some(&self.messages)
        }
    }

    pub fn event(&self, idx: usize) -> Option<MidiStorageElem> {
        if self.dirty {
            return None;
        }
        self.messages.get(idx).copied()
    }

    /// Adds a message. Returns false if the payload is empty or over
    /// four bytes.
    ///
    /// audio thread from unwinding over data it cannot control.
    pub fn write(&mut self, time: u32, data: &[u8]) -> bool {
        let Some(elem) = MidiStorageElem::new(time, data) else {
            self.n_rejected += 1;
            return false;
        };
        self.write_elem(elem)
    }

    pub fn write_elem(&mut self, elem: MidiStorageElem) -> bool {
        if self.messages.len() >= self.reserved {
            self.n_overflows += 1;
        }
        self.messages.push(elem);
        self.dirty = true;
        true
    }

    /// Orders messages by time, stably.
    pub fn sort(&mut self) {
        if self.dirty {
            sort_by_time(&mut self.messages);
            self.dirty = false;
        }
    }

    /// Start of a cycle: discard everything.
    pub fn prepare(&mut self) {
        self.clear();
    }

    /// End of a cycle: make the contents readable.
    pub fn process(&mut self) {
        self.sort();
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::{check, let_assert};

    fn buf() -> MidiSortingBuffer {
        MidiSortingBuffer::with_capacity(8)
    }

    fn times(b: &MidiSortingBuffer) -> Vec<u32> {
        b.events().unwrap().iter().map(|e| e.time).collect()
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn starts_empty_and_sorted() {
        let b = buf();
        check!(b.n_events() == 0);
        check!(b.is_sorted());
        check!(b.events() == Some(&[][..]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn writing_marks_unsorted_until_sorted() {
        let mut b = buf();
        b.write(5, &midi::note_on(0, 60, 1));
        check!(!b.is_sorted());
        check!(b.events() == None);
        check!(b.event(0) == None);

        b.sort();
        check!(b.is_sorted());
        check!(times(&b) == vec![5]);
        let_assert!(Some(e) = b.event(0));
        check!(e.time == 5);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn sorts_by_time() {
        let mut b = buf();
        for t in [9u32, 2, 7, 0] {
            b.write(t, &midi::note_on(0, 60, 1));
        }
        b.sort();
        check!(times(&b) == vec![0, 2, 7, 9]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    /// A burst well past the standard stable sort's insertion-sort threshold: this
    /// is the size at which `sort_by_key` would have allocated a scratch buffer.
    fn sorting_a_large_burst_stays_ordered_and_stable() {
        let mut b = buf();
        // Descending times, so nothing is already in place.
        for t in (0..200u32).rev() {
            b.write(t, &midi::note_off(0, 60, 0));
            b.write(t, &midi::note_on(0, 60, 100));
        }
        b.sort();
        let events = b.events().expect("sorted");
        check!(events.len() == 400);
        for (i, e) in events.iter().enumerate() {
            check!(e.time == (i / 2) as u32);
            // Within a timestamp the note-off written first stays first.
            let expected_off = i % 2 == 0;
            check!(midi::is_note_off(e.data()) == expected_off);
        }
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn sorting_is_stable_for_equal_times() {
        let mut b = buf();
        // Same timestamp: a note-off must stay ahead of the note-on written after
        // it, or a retrigger at a loop boundary silences itself.
        b.write(4, &midi::note_off(0, 60, 0));
        b.write(4, &midi::note_on(0, 60, 100));
        b.write(4, &midi::cc(0, 7, 3));
        b.sort();
        let evs = b.events().unwrap();
        check!(midi::is_note_off(evs[0].data()));
        check!(midi::is_note_on(evs[1].data()));
        check!(midi::is_cc(evs[2].data()));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn sorting_twice_is_a_no_op() {
        let mut b = buf();
        b.write(3, &midi::note_on(0, 60, 1));
        b.write(1, &midi::note_on(0, 61, 1));
        b.sort();
        let first = times(&b);
        b.sort();
        check!(times(&b) == first);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn oversized_messages_are_refused() {
        let mut b = buf();
        check!(!b.write(0, &[1, 2, 3, 4, 5]));
        check!(!b.write(0, &[]));
        check!(b.n_events() == 0);
        check!(b.n_rejected() == 2);
        // A maximum-size payload is fine.
        check!(b.write(0, &[1, 2, 3, 4]));
        check!(b.n_events() == 1);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn exceeding_the_reservation_is_counted() {
        let mut b = MidiSortingBuffer::with_capacity(2);
        b.write(0, &midi::note_on(0, 60, 1));
        b.write(1, &midi::note_on(0, 61, 1));
        check!(b.n_overflows() == 0);
        // Past the reservation the message is still kept, but the allocation on
        // the audio thread is recorded.
        b.write(2, &midi::note_on(0, 62, 1));
        check!(b.n_overflows() == 1);
        check!(b.n_events() == 3);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn prepare_clears_for_the_next_cycle() {
        let mut b = buf();
        b.write(1, &midi::note_on(0, 60, 1));
        b.prepare();
        check!(b.n_events() == 0);
        check!(b.is_sorted());
        check!(b.events() == Some(&[][..]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn process_sorts_at_end_of_cycle() {
        let mut b = buf();
        b.write(8, &midi::note_on(0, 60, 1));
        b.write(2, &midi::note_on(0, 61, 1));
        b.process();
        check!(b.is_sorted());
        check!(times(&b) == vec![2, 8]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_full_cycle_round_trip() {
        let mut b = buf();
        b.prepare();
        b.write(5, &midi::note_on(0, 60, 1));
        b.write(0, &midi::cc(0, 7, 9));
        b.process();
        check!(times(&b) == vec![0, 5]);
        // Next cycle starts clean.
        b.prepare();
        check!(b.n_events() == 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn write_elem_accepts_a_prebuilt_message() {
        let mut b = buf();
        let e = MidiStorageElem::new(7, &midi::note_on(0, 60, 1)).unwrap();
        check!(b.write_elem(e));
        b.sort();
        check!(times(&b) == vec![7]);
    }
}
