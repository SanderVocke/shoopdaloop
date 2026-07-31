//! Rolling MIDI capture buffer: the MIDI counterpart to
//! [`crate::buffer_queue::BufferQueue`].
//!
//! Wraps a [`MidiStorage`] and keeps only the most recent `n_samples` worth of
//! messages, trimming the tail as time advances. Timestamps are absolute and
//! monotonically increasing, so they are periodically shifted back down to avoid
//! running out of `u32`.

use crate::midi_storage::{MidiStorage, MidiStorageElem, TruncateSide};

#[derive(Debug)]
pub struct MidiRingbuffer {
    storage: MidiStorage,
    n_samples: u32,
    current_buffer_start_time: u32,
    current_buffer_end_time: u32,
}

impl MidiRingbuffer {
    pub fn with_capacity_elems(capacity: usize) -> Self {
        Self {
            storage: MidiStorage::with_capacity_elems(capacity),
            n_samples: 0,
            current_buffer_start_time: 0,
            current_buffer_end_time: 0,
        }
    }

    pub fn n_samples(&self) -> u32 {
        self.n_samples
    }
    pub fn n_events(&self) -> u32 {
        self.storage.n_events()
    }
    pub fn storage(&self) -> &MidiStorage {
        &self.storage
    }

    /// Time of the oldest message the window can hold.
    ///
    /// Derived from the window length, not from the current buffer's start time,
    /// buffer is shorter than the window.
    pub fn window_start_time(&self) -> u32 {
        self.current_buffer_end_time.saturating_sub(self.n_samples)
    }
    pub fn current_end_time(&self) -> u32 {
        self.current_buffer_end_time
    }
    /// Start of the buffer currently being filled.
    pub fn current_buffer_start_time(&self) -> u32 {
        self.current_buffer_start_time
    }

    /// Sets the window length, discarding anything now out of range.
    pub fn set_n_samples(&mut self, n: u32) {
        self.n_samples = n;
        let end = self.current_buffer_end_time;
        self.storage
            .truncate(end.saturating_sub(n), TruncateSide::Tail, None);
    }

    /// Advances time by `n_frames`, trimming messages that fall out of the window.
    pub fn next_buffer(
        &mut self,
        n_frames: u32,
        dropped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) {
        let mut old_end = self.current_buffer_end_time;
        let mut new_end = old_end.wrapping_add(n_frames);

        if new_end < old_end {
            // Timestamps would wrap. Shift the whole buffer down so everything
            // sits at low time values again, then continue from there.
            let moved_new_end = self.n_samples;
            let shift = moved_new_end.wrapping_sub(new_end);
            self.storage
                .for_each_modify(|e| e.time = e.time.wrapping_add(shift));
            new_end = new_end.wrapping_add(shift);
            old_end = old_end.wrapping_add(shift);
        }

        self.storage.truncate(
            new_end.saturating_sub(self.n_samples.min(new_end)),
            TruncateSide::Tail,
            dropped,
        );
        self.current_buffer_start_time = old_end;
        self.current_buffer_end_time = new_end;
    }

    /// Records a message at `frame_in_current_buffer` within the current buffer.
    ///
    /// Returns false if the frame falls past the end of the current buffer.
    pub fn put(
        &mut self,
        frame_in_current_buffer: u32,
        data: &[u8],
        dropped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) -> bool {
        let time = self.current_buffer_start_time + frame_in_current_buffer;
        if time > self.current_buffer_end_time {
            return false;
        }
        self.storage.append(time, data, true, dropped)
    }

    /// Copies the window into `target`, rebasing times so that
    /// `start_offset_from_end` before the current end becomes time zero.
    ///
    /// `None` uses the full window length. Anything older is dropped.
    pub fn snapshot(&self, target: &mut MidiStorage, start_offset_from_end: Option<u32>) {
        self.storage.copy_into(target);
        let end = self.current_buffer_end_time;
        let start_from_end = start_offset_from_end.unwrap_or(self.n_samples);
        let min_message_time = end.saturating_sub(end.min(start_from_end));
        target.truncate(min_message_time, TruncateSide::Tail, None);
        target.for_each_modify(|e| e.time = e.time.saturating_sub(min_message_time));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    fn rb(cap: usize, n_samples: u32) -> MidiRingbuffer {
        let mut r = MidiRingbuffer::with_capacity_elems(cap);
        r.set_n_samples(n_samples);
        r
    }

    fn times(s: &MidiStorage) -> Vec<u32> {
        s.iter().map(|e| e.time).collect()
    }

    fn snap(r: &MidiRingbuffer, offset: Option<u32>) -> Vec<u32> {
        let mut t = MidiStorage::with_capacity_elems(1);
        r.snapshot(&mut t, offset);
        times(&t)
    }

    #[test]
    fn starts_empty() {
        let r = rb(64, 100);
        check!(r.n_events() == 0);
        check!(r.n_samples() == 100);
        check!(r.current_end_time() == 0);
    }

    #[test]
    fn put_places_messages_at_absolute_times() {
        let mut r = rb(64, 100);
        r.next_buffer(10, None);
        // Buffer now spans [0, 10).
        check!(r.put(3, &midi::note_on(0, 60, 1), None));
        check!(times(r.storage()) == vec![3]);

        r.next_buffer(10, None);
        // Buffer now spans [10, 20); frame 5 is absolute time 15.
        check!(r.put(5, &midi::note_on(0, 61, 1), None));
        check!(times(r.storage()) == vec![3, 15]);
    }

    #[test]
    fn put_past_the_current_buffer_end_is_rejected() {
        let mut r = rb(64, 100);
        r.next_buffer(10, None);
        // Buffer spans [0, 10]; frame 11 is beyond it.
        check!(!r.put(11, &midi::note_on(0, 60, 1), None));
        check!(r.n_events() == 0);
        // Exactly at the end is accepted.
        check!(r.put(10, &midi::note_on(0, 60, 1), None));
    }

    #[test]
    fn advancing_time_trims_messages_out_of_the_window() {
        let mut r = rb(64, 10);
        r.next_buffer(5, None);
        r.put(0, &midi::note_on(0, 60, 1), None); // t=0
        r.next_buffer(5, None);
        r.put(0, &midi::note_on(0, 61, 1), None); // t=5
        check!(times(r.storage()) == vec![0, 5]);

        // End reaches 20, window is 10, so anything before t=10 goes.
        r.next_buffer(10, None);
        check!(times(r.storage()) == Vec::<u32>::new());
    }

    #[test]
    fn trimming_reports_dropped_messages() {
        let mut r = rb(64, 10);
        r.next_buffer(5, None);
        r.put(0, &midi::note_on(0, 60, 1), None);
        let mut dropped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| dropped.push(e.time);
            r.next_buffer(20, Some(&mut cb));
        }
        check!(dropped == vec![0]);
    }

    #[test]
    fn set_n_samples_shrinks_the_window_immediately() {
        let mut r = rb(64, 100);
        r.next_buffer(10, None);
        r.put(0, &midi::note_on(0, 60, 1), None); // t=0
        r.put(9, &midi::note_on(0, 61, 1), None); // t=9
        check!(r.n_events() == 2);

        // End is 10; a window of 5 keeps only t >= 5.
        r.set_n_samples(5);
        check!(times(r.storage()) == vec![9]);
    }

    #[test]
    fn window_start_time_is_derived_from_the_window_length() {
        let mut r = rb(64, 10);
        r.next_buffer(4, None);
        // End is 4, window is 10, so the window starts at 0 (saturating).
        check!(r.current_end_time() == 4);
        check!(r.window_start_time() == 0);
        check!(r.current_buffer_start_time() == 0);

        r.next_buffer(20, None);
        // End 24, window 10 -> window starts at 14, while the current buffer
        // started at 4. The two are deliberately different.
        check!(r.current_end_time() == 24);
        check!(r.window_start_time() == 14);
        check!(r.current_buffer_start_time() == 4);
    }

    #[test]
    fn snapshot_rebases_times_to_the_window_start() {
        let mut r = rb(64, 10);
        r.next_buffer(10, None);
        r.put(2, &midi::note_on(0, 60, 1), None); // t=2
        r.put(7, &midi::note_on(0, 61, 1), None); // t=7
        r.next_buffer(5, None); // end=15, window keeps t>=5
        check!(times(r.storage()) == vec![7]);

        // Rebased so that end-10 == 5 becomes zero.
        check!(snap(&r, None) == vec![2]);
    }

    #[test]
    fn snapshot_offset_selects_a_shorter_tail() {
        let mut r = rb(64, 100);
        r.next_buffer(20, None);
        r.put(1, &midi::note_on(0, 60, 1), None); // t=1
        r.put(15, &midi::note_on(0, 61, 1), None); // t=15

        // Only the last 10 samples: t>=10 survives, rebased to end-10 == 10.
        check!(snap(&r, Some(10)) == vec![5]);
        // The whole window keeps both, rebased to 0.
        check!(snap(&r, None) == vec![1, 15]);
    }

    #[test]
    fn snapshot_does_not_disturb_the_ringbuffer() {
        let mut r = rb(64, 100);
        r.next_buffer(20, None);
        r.put(1, &midi::note_on(0, 60, 1), None);
        let before = times(r.storage());
        let _ = snap(&r, Some(5));
        check!(times(r.storage()) == before);
    }

    #[test]
    fn timestamps_are_rebased_low_instead_of_wrapping() {
        let mut r = rb(64, 1000);
        // Park time near the top of the u32 range while empty, then open a
        // buffer up there so a message gets a high timestamp.
        r.next_buffer(u32::MAX - 100, None);
        r.next_buffer(50, None);
        check!(r.current_buffer_start_time() == u32::MAX - 100);
        r.put(10, &midi::note_on(0, 60, 1), None);
        let high = times(r.storage())[0];
        check!(high > u32::MAX - 1000);

        // The message sits this far before the current end, which becomes the
        // start of the next buffer.
        let distance_before_end = r.current_end_time() - high;
        check!(distance_before_end == 40);

        // This advance would wrap, so the whole buffer is rebased low, preserving
        // every message's offset relative to the timeline.
        r.next_buffer(100, None);
        check!(r.current_end_time() == 1000);
        check!(r.current_buffer_start_time() == 900);
        let low = times(r.storage())[0];
        check!(low < 1000);
        check!(r.current_buffer_start_time() - low == distance_before_end);
    }

    #[test]
    fn full_buffer_drops_oldest_on_put() {
        let mut r = rb(2, 1000);
        r.next_buffer(100, None);
        r.put(0, &midi::note_on(0, 60, 1), None);
        r.put(1, &midi::note_on(0, 61, 1), None);
        // Capacity 2 is reached; a third message evicts the oldest.
        let mut dropped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| dropped.push(e.time);
            check!(r.put(2, &midi::note_on(0, 62, 1), Some(&mut cb)));
        }
        check!(dropped == vec![0]);
        check!(times(r.storage()) == vec![1, 2]);
    }
}
