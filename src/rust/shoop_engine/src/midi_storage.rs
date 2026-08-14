//! Fixed-capacity ring buffer of MIDI messages, plus cursors over it.
//!
//! Messages carry an inline 4-byte payload, which covers every channel-voice
//! message; anything longer is rejected rather than heap-allocated, so the
//! storage never allocates after construction.
//!
//! of the storage pushing invalidation to registered cursors, a cursor addresses
//! messages by absolute index and reconciles itself on use via [`Cursor::sync`].
//! Absolute indices are what make that possible: a ring offset cannot tell
//! "still the message I was on" from "that slot was refilled".

pub const MAX_MSG_BYTES: usize = 4;

/// Orders messages by time, stably and without allocating.
///
/// Not `sort_by_key`: the standard stable sort allocates a scratch buffer once the
/// slice exceeds its insertion-sort threshold, which is fatal on the audio thread
/// and only shows up under a large burst such as a playback state restore.
/// Insertion sort is stable, allocates nothing, and is linear on the
/// nearly-sorted and all-equal inputs this actually sees.
///
/// Stability matters: messages sharing a timestamp must keep their write order, or
/// a note-off/note-on pair at a loop boundary silences itself.
pub fn sort_by_time(messages: &mut [MidiStorageElem]) {
    for i in 1..messages.len() {
        let mut j = i;
        while j > 0 && messages[j - 1].time > messages[j].time {
            messages.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// One stored message. `time` is absolute within the storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MidiStorageElem {
    pub time: u32,
    size: u16,
    bytes: [u8; MAX_MSG_BYTES],
}

impl MidiStorageElem {
    /// `None` when the payload is empty or longer than [`MAX_MSG_BYTES`].
    pub fn new(time: u32, data: &[u8]) -> Option<Self> {
        if data.is_empty() || data.len() > MAX_MSG_BYTES {
            return None;
        }
        let mut bytes = [0u8; MAX_MSG_BYTES];
        bytes[..data.len()].copy_from_slice(data);
        Some(Self {
            time,
            size: data.len() as u16,
            bytes,
        })
    }

    /// Same payload, different time.
    pub fn at_time(mut self, time: u32) -> Self {
        self.time = time;
        self
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.size as usize]
    }
    pub fn size(&self) -> usize {
        self.size as usize
    }
    /// Compares payloads only, ignoring time.
    pub fn contents_equal(&self, other: &Self) -> bool {
        self.data() == other.data()
    }
}

/// Which end of the storage a truncation removes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncateSide {
    /// Discard the newest messages.
    Head,
    /// Discard the oldest messages.
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceRangeError {
    InvalidRange,
    OutOfOrder,
    OutOfCapacity { required: usize, capacity: usize },
}

/// Outcome of a cursor search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorFindResult {
    pub n_processed: u32,
    pub found_valid_elem: bool,
}

#[derive(Debug, Clone)]
pub struct MidiStorage {
    data: Vec<MidiStorageElem>,
    /// Ring index of the oldest live message.
    tail: u32,
    /// Absolute index of the oldest live message.
    ///
    /// Cursors address messages by absolute index rather than ring offset. A
    /// ring offset cannot distinguish "still the message I was on" from "that
    /// slot was dropped and immediately refilled", which is exactly what happens
    /// when a full buffer is appended to. Signed because `prepend` walks it down.
    first_index: i64,
    n_events: u32,
    /// Absolute index below which messages were deliberately discarded, by
    /// `clear` or `truncate`.
    ///
    /// Distinguishes the two ways a cursor can fall behind the window. An append
    /// overwrite re-anchors it to the new oldest message; a clear or truncate
    /// invalidates it outright. Both move `first_index` forward identically, so
    /// this watermark is what tells them apart.
    invalidated_below: i64,
}

impl MidiStorage {
    /// divided by the platform's `sizeof(Elem)`; taking elements directly avoids
    pub fn with_capacity_elems(capacity: usize) -> Self {
        Self {
            data: vec![MidiStorageElem::default(); capacity],
            tail: 0,
            first_index: 0,
            n_events: 0,
            invalidated_below: 0,
        }
    }

    pub fn capacity_elems(&self) -> usize {
        self.data.len()
    }
    pub fn n_events(&self) -> u32 {
        self.n_events
    }
    pub fn is_full(&self) -> bool {
        self.n_events as usize == self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.n_events == 0
    }
    /// Ring index of the oldest message. Exposed mainly so compaction is testable.
    pub fn tail_offset(&self) -> u32 {
        self.tail
    }
    /// Absolute index of the oldest live message.
    pub fn first_index(&self) -> i64 {
        self.first_index
    }
    /// One past the absolute index of the newest live message.
    pub fn end_index(&self) -> i64 {
        self.first_index + self.n_events as i64
    }

    pub fn clear(&mut self) {
        // Advance past the discarded messages before zeroing the count, so
        // indices keep moving forward and stale cursors cannot look live.
        self.first_index = self.end_index();
        self.tail = 0;
        self.n_events = 0;
        self.invalidated_below = self.first_index;
    }

    /// Absolute index below which content was deliberately discarded.
    pub fn invalidated_below(&self) -> i64 {
        self.invalidated_below
    }

    /// Drops messages for which `should_truncate` holds, from one end.
    ///
    /// The predicate is assumed monotone over time, which holds for the
    /// time-based truncation in [`MidiStorage::truncate`]: scanning stops at the
    /// first message that flips it.
    pub fn truncate_fn(
        &mut self,
        should_truncate: impl Fn(&MidiStorageElem) -> bool,
        side: TruncateSide,
        mut dropped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) {
        if self.data.is_empty() || self.n_events == 0 {
            return;
        }
        let cap = self.data.len() as u32;

        match side {
            TruncateSide::Head => {
                let newest = self.ring_of(self.end_index() - 1).unwrap();
                if !should_truncate(&self.data[newest as usize]) {
                    return;
                }
                let mut kept = 0u32;
                let mut idx = self.tail;
                for _ in 0..self.n_events {
                    if should_truncate(&self.data[idx as usize]) {
                        break;
                    }
                    kept += 1;
                    idx = (idx + 1) % cap;
                }
                if let Some(cb) = dropped.as_mut() {
                    let mut drop_idx = idx;
                    for _ in kept..self.n_events {
                        cb(&self.data[drop_idx as usize]);
                        drop_idx = (drop_idx + 1) % cap;
                    }
                }
                self.n_events = kept;
            }
            TruncateSide::Tail => {
                if !should_truncate(&self.data[self.tail as usize]) {
                    return;
                }
                let mut n_dropped = 0u32;
                let mut idx = self.tail;
                for _ in 0..self.n_events {
                    if !should_truncate(&self.data[idx as usize]) {
                        break;
                    }
                    if let Some(cb) = dropped.as_mut() {
                        cb(&self.data[idx as usize]);
                    }
                    n_dropped += 1;
                    idx = (idx + 1) % cap;
                }
                self.tail = idx;
                self.first_index += n_dropped as i64;
                self.n_events -= n_dropped;
                self.invalidated_below = self.first_index;
            }
        }
    }

    /// Drops messages after `time` (head) or before it (tail).
    pub fn truncate(
        &mut self,
        time: u32,
        side: TruncateSide,
        dropped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) {
        match side {
            TruncateSide::Head => self.truncate_fn(|e| e.time > time, side, dropped),
            TruncateSide::Tail => self.truncate_fn(|e| e.time < time, side, dropped),
        }
    }

    /// Replaces every event in `[start, end)` with an ordered replacement.
    ///
    /// The backing allocation is reused. Existing cursors are invalidated because
    /// a middle splice changes message identities on both sides of the insertion.
    pub fn replace_range(
        &mut self,
        start: u32,
        end: u32,
        replacement: &[MidiStorageElem],
    ) -> Result<(), ReplaceRangeError> {
        if start >= end
            || replacement
                .iter()
                .any(|event| event.time < start || event.time >= end)
        {
            return Err(ReplaceRangeError::InvalidRange);
        }
        if replacement
            .windows(2)
            .any(|events| events[0].time > events[1].time)
        {
            return Err(ReplaceRangeError::OutOfOrder);
        }

        let live = self.n_events as usize;
        let mut before = 0;
        let mut after = live;
        for (index, event) in self.iter().enumerate() {
            if event.time < start {
                before = index + 1;
            }
            if event.time >= end {
                after = after.min(index);
            }
        }
        let required = before + replacement.len() + live.saturating_sub(after);
        if required > self.data.len() {
            return Err(ReplaceRangeError::OutOfCapacity {
                required,
                capacity: self.data.len(),
            });
        }

        let old_end = self.end_index();
        if !self.data.is_empty() {
            self.data.rotate_left(self.tail as usize);
            self.data
                .copy_within(after..live, before + replacement.len());
            self.data[before..before + replacement.len()].copy_from_slice(replacement);
        }
        self.tail = 0;
        self.first_index = old_end;
        self.n_events = required as u32;
        self.invalidated_below = self.first_index;
        Ok(())
    }

    /// Ring slot holding absolute index `i`, if it is live.
    fn ring_of(&self, i: i64) -> Option<u32> {
        if self.data.is_empty() || i < self.first_index || i >= self.end_index() {
            return None;
        }
        let cap = self.data.len() as i64;
        Some(((self.tail as i64 + (i - self.first_index)) % cap) as u32)
    }

    /// Appends a message. Messages must arrive in non-decreasing time order.
    ///
    /// With `allow_replace`, a full buffer drops its oldest message to make
    /// room and reports it through `dropped`. Without it, a full buffer refuses.
    /// Returns whether the message was stored.
    pub fn append(
        &mut self,
        time: u32,
        data: &[u8],
        allow_replace: bool,
        mut dropped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) -> bool {
        if data.is_empty() || data.len() > MAX_MSG_BYTES || self.data.is_empty() {
            return false;
        }
        if self.is_full() && !allow_replace {
            return false;
        }
        if self.n_events > 0 {
            let newest = self.ring_of(self.end_index() - 1).unwrap_or(0);
            if self.data[newest as usize].time > time {
                // Out-of-order messages are refused rather than reordered.
                return false;
            }
        }

        let cap = self.data.len() as u32;
        if self.is_full() {
            if let Some(cb) = dropped.as_mut() {
                cb(&self.data[self.tail as usize]);
            }
            self.tail = (self.tail + 1) % cap;
            self.first_index += 1;
            self.n_events -= 1;
        }

        // Write one past the newest.
        let slot = ((self.tail as i64 + self.n_events as i64) % cap as i64) as usize;
        let elem = &mut self.data[slot];
        elem.time = time;
        elem.size = data.len() as u16;
        elem.bytes = [0; MAX_MSG_BYTES];
        elem.bytes[..data.len()].copy_from_slice(data);
        self.n_events += 1;
        true
    }

    /// Inserts before the oldest message. Refuses when full, or when the message
    /// is newer than the current oldest.
    pub fn prepend(&mut self, time: u32, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > MAX_MSG_BYTES || self.data.is_empty() {
            return false;
        }
        if self.is_full() {
            return false;
        }
        if self.n_events > 0 && self.data[self.tail as usize].time < time {
            return false;
        }

        let cap = self.data.len() as u32;
        self.tail = (self.tail + cap - 1) % cap;
        self.first_index -= 1;
        self.n_events += 1;

        let elem = &mut self.data[self.tail as usize];
        elem.time = time;
        elem.size = data.len() as u16;
        elem.bytes = [0; MAX_MSG_BYTES];
        elem.bytes[..data.len()].copy_from_slice(data);
        true
    }

    /// Copies contents into `to`, compacted so the oldest message sits at 0.
    pub fn copy_into(&self, to: &mut MidiStorage) {
        to.data.resize(self.data.len(), MidiStorageElem::default());
        to.tail = 0;
        to.first_index = 0;
        to.n_events = self.n_events;
        for (count, e) in self.iter().enumerate() {
            to.data[count] = *e;
        }
    }

    /// Messages oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &MidiStorageElem> + '_ {
        let cap = self.data.len().max(1);
        (0..self.n_events as usize).map(move |i| &self.data[(self.tail as usize + i) % cap])
    }

    pub fn for_each_modify(&mut self, mut cb: impl FnMut(&mut MidiStorageElem)) {
        let cap = self.data.len().max(1);
        for i in 0..self.n_events as usize {
            cb(&mut self.data[(self.tail as usize + i) % cap]);
        }
    }

    pub fn create_cursor(&self) -> Cursor {
        let mut c = Cursor::default();
        c.reset(self);
        c
    }
}

/// A position within a [`MidiStorage`], held as an absolute message index.
///
/// Holds no reference to the storage, so every operation takes it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    index: Option<i64>,
    prev_index: Option<i64>,
}

impl Cursor {
    pub fn valid(&self) -> bool {
        self.index.is_some()
    }
    /// Ring offset currently addressed, if any.
    pub fn offset(&self, storage: &MidiStorage) -> Option<u32> {
        self.index.and_then(|i| storage.ring_of(i))
    }
    pub fn prev_offset(&self, storage: &MidiStorage) -> Option<u32> {
        self.prev_index.and_then(|i| storage.ring_of(i))
    }
    pub fn index(&self) -> Option<i64> {
        self.index
    }
    pub fn invalidate(&mut self) {
        self.index = None;
        self.prev_index = None;
    }
    pub fn is_at_start(&self, storage: &MidiStorage) -> bool {
        self.index == Some(storage.first_index) && storage.n_events > 0
    }
    pub fn overwrite(&mut self, index: i64, prev_index: i64) {
        self.index = Some(index);
        self.prev_index = Some(prev_index);
    }

    /// Moves to the oldest message, or invalidates when there is none.
    pub fn reset(&mut self, storage: &MidiStorage) {
        if storage.n_events == 0 {
            self.index = None;
            self.prev_index = None;
        } else {
            self.index = Some(storage.first_index);
            self.prev_index = None;
        }
    }

    /// Re-anchors a cursor whose message has since been dropped.
    ///
    /// overwrite consumed the element they pointed at.
    pub fn sync(&mut self, storage: &MidiStorage) {
        let Some(i) = self.index else { return };
        if i < storage.invalidated_below || i >= storage.end_index() {
            // Deliberately discarded, or truncated off the head.
            self.invalidate();
        } else if i < storage.first_index {
            // Merely overwritten to make room; carry on from the new oldest.
            self.reset(storage);
        }
    }

    pub fn get<'s>(&self, storage: &'s MidiStorage) -> Option<&'s MidiStorageElem> {
        let ring = self.offset(storage)?;
        storage.data.get(ring as usize)
    }
    pub fn get_prev<'s>(&self, storage: &'s MidiStorage) -> Option<&'s MidiStorageElem> {
        let ring = self.prev_offset(storage)?;
        storage.data.get(ring as usize)
    }

    /// True when stepping from the previous message to the current one crossed
    /// the ring boundary.
    pub fn wrapped(&self, storage: &MidiStorage) -> bool {
        match (self.prev_offset(storage), self.offset(storage)) {
            (Some(p), Some(o)) => p > o,
            _ => false,
        }
    }

    pub fn next(&mut self, storage: &MidiStorage) {
        let Some(i) = self.index else { return };
        if i + 1 >= storage.end_index() {
            self.invalidate();
            return;
        }
        self.prev_index = Some(i);
        self.index = Some(i + 1);
    }

    /// Advances to the first message at or after `time`.
    pub fn find_time_forward(
        &mut self,
        storage: &MidiStorage,
        time: u32,
        skipped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) -> CursorFindResult {
        self.find_fn_forward(storage, |e| e.time >= time, skipped)
    }

    /// Advances to the first message satisfying `pred`, invalidating if none do.
    pub fn find_fn_forward(
        &mut self,
        storage: &MidiStorage,
        pred: impl Fn(&MidiStorageElem) -> bool,
        mut skipped: Option<&mut dyn FnMut(&MidiStorageElem)>,
    ) -> CursorFindResult {
        let mut rval = CursorFindResult::default();
        let Some(start) = self.index else {
            return rval;
        };

        let mut i = start;
        while i < storage.end_index() {
            let Some(ring) = storage.ring_of(i) else {
                break;
            };
            let elem = &storage.data[ring as usize];
            if pred(elem) {
                if i != start {
                    self.prev_index = Some(i - 1);
                    self.index = Some(i);
                }
                rval.found_valid_elem = true;
                return rval;
            }
            if let Some(cb) = skipped.as_mut() {
                cb(elem);
            }
            i += 1;
            rval.n_processed += 1;
        }

        self.invalidate();
        rval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    fn storage(cap: usize) -> MidiStorage {
        MidiStorage::with_capacity_elems(cap)
    }

    fn note(n: u8) -> [u8; 3] {
        [0x90, n, 0x7f]
    }

    fn times(s: &MidiStorage) -> Vec<u32> {
        s.iter().map(|e| e.time).collect()
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn starts_empty() {
        let s = storage(4);
        check!(s.n_events() == 0);
        check!(s.is_empty());
        check!(!s.is_full());
        check!(s.capacity_elems() == 4);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn appends_in_order() {
        let mut s = storage(4);
        check!(s.append(0, &note(1), false, None));
        check!(s.append(5, &note(2), false, None));
        check!(s.n_events() == 2);
        check!(times(&s) == vec![0, 5]);
        let_assert!(Some(first) = s.iter().next());
        check!(first.data() == &note(1));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn refuses_out_of_order() {
        let mut s = storage(4);
        check!(s.append(10, &note(1), false, None));
        check!(!s.append(9, &note(2), false, None));
        check!(s.n_events() == 1);
        // Equal times are allowed.
        check!(s.append(10, &note(3), false, None));
        check!(s.n_events() == 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn refuses_when_full_without_replace() {
        let mut s = storage(2);
        check!(s.append(0, &note(1), false, None));
        check!(s.append(1, &note(2), false, None));
        check!(s.is_full());
        check!(!s.append(2, &note(3), false, None));
        check!(times(&s) == vec![0, 1]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn replaces_a_middle_interval_without_disturbing_outer_events() {
        let mut s = storage(6);
        s.append(0, &note(1), false, None);
        s.append(2, &note(2), false, None);
        s.append(4, &note(3), false, None);
        s.append(6, &note(4), false, None);
        let replacement = [
            MidiStorageElem::new(2, &note(8)).unwrap(),
            MidiStorageElem::new(3, &note(9)).unwrap(),
        ];

        check!(s.replace_range(2, 5, &replacement).is_ok());
        check!(times(&s) == vec![0, 2, 3, 6]);
        check!(s.iter().map(|event| event.data()[1]).collect::<Vec<_>>() == vec![1, 8, 9, 4]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn replacement_is_atomic_when_capacity_is_insufficient() {
        let mut s = storage(3);
        s.append(0, &note(1), false, None);
        s.append(2, &note(2), false, None);
        s.append(6, &note(3), false, None);
        let replacement = [
            MidiStorageElem::new(2, &note(8)).unwrap(),
            MidiStorageElem::new(3, &note(9)).unwrap(),
        ];

        check!(
            s.replace_range(2, 5, &replacement)
                == Err(ReplaceRangeError::OutOfCapacity {
                    required: 4,
                    capacity: 3
                })
        );
        check!(times(&s) == vec![0, 2, 6]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn replacement_linearizes_wrapped_storage_and_invalidates_cursors() {
        let mut s = storage(4);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        s.append(2, &note(3), false, None);
        s.append(3, &note(4), false, None);
        s.append(4, &note(5), true, None);
        let mut cursor = s.create_cursor();
        let replacement = [MidiStorageElem::new(2, &note(8)).unwrap()];

        check!(s.replace_range(2, 4, &replacement).is_ok());
        cursor.sync(&s);
        check!(!cursor.valid());
        check!(s.tail_offset() == 0);
        check!(times(&s) == vec![1, 2, 4]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn replace_drops_oldest_and_reports_it() {
        let mut s = storage(2);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        let mut dropped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| dropped.push(e.time);
            check!(s.append(2, &note(3), true, Some(&mut cb)));
        }
        check!(dropped == vec![0]);
        check!(s.n_events() == 2);
        check!(times(&s) == vec![1, 2]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn rejects_oversized_and_empty_payloads() {
        let mut s = storage(4);
        check!(!s.append(0, &[], false, None));
        check!(!s.append(0, &[1, 2, 3, 4, 5], false, None));
        check!(s.append(0, &[1, 2, 3, 4], false, None));
        check!(s.n_events() == 1);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn prepend_inserts_before_oldest() {
        let mut s = storage(4);
        s.append(10, &note(1), false, None);
        check!(s.prepend(5, &note(2)));
        check!(times(&s) == vec![5, 10]);
        // Newer than the oldest: refused.
        check!(!s.prepend(7, &note(3)));
        check!(s.n_events() == 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn prepend_refuses_when_full() {
        let mut s = storage(1);
        s.append(5, &note(1), false, None);
        check!(!s.prepend(1, &note(2)));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn clear_empties() {
        let mut s = storage(4);
        s.append(0, &note(1), false, None);
        s.clear();
        check!(s.n_events() == 0);
        check!(times(&s) == Vec::<u32>::new());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn copy_into_compacts_from_zero() {
        let mut s = storage(3);
        // Wrap the ring so tail is not 0.
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        s.append(2, &note(3), false, None);
        s.append(3, &note(4), true, None);
        check!(times(&s) == vec![1, 2, 3]);

        let mut dst = storage(1);
        s.copy_into(&mut dst);
        check!(dst.n_events() == 3);
        check!(times(&dst) == vec![1, 2, 3]);
        check!(dst.tail_offset() == 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn for_each_modify_visits_in_order() {
        let mut s = storage(4);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        s.for_each_modify(|e| e.time += 100);
        check!(times(&s) == vec![100, 101]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cursor_walks_all_messages_then_invalidates() {
        let mut s = storage(4);
        for i in 0..3 {
            s.append(i, &note(i as u8), false, None);
        }
        let mut c = s.create_cursor();
        check!(c.valid());
        check!(c.is_at_start(&s));

        let mut seen = Vec::new();
        while let Some(e) = c.get(&s) {
            seen.push(e.time);
            c.next(&s);
        }
        check!(seen == vec![0, 1, 2]);
        check!(!c.valid());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cursor_on_empty_storage_is_invalid() {
        let s = storage(4);
        let c = s.create_cursor();
        check!(!c.valid());
        check!(c.get(&s) == None);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cursor_tracks_previous_element() {
        let mut s = storage(4);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        let mut c = s.create_cursor();
        check!(c.get_prev(&s) == None);
        c.next(&s);
        let_assert!(Some(prev) = c.get_prev(&s));
        check!(prev.time == 0);
        check!(!c.wrapped(&s));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn find_time_forward_lands_on_first_at_or_after() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut c = s.create_cursor();
        let r = c.find_time_forward(&s, 20, None);
        check!(r.found_valid_elem);
        check!(r.n_processed == 2);
        let_assert!(Some(e) = c.get(&s));
        check!(e.time == 20);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn find_time_forward_reports_skipped_messages() {
        let mut s = storage(8);
        for t in [0u32, 10, 20] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut c = s.create_cursor();
        let mut skipped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| skipped.push(e.time);
            c.find_time_forward(&s, 20, Some(&mut cb));
        }
        check!(skipped == vec![0, 10]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn find_time_forward_past_the_end_invalidates() {
        let mut s = storage(8);
        s.append(0, &note(1), false, None);
        let mut c = s.create_cursor();
        let r = c.find_time_forward(&s, 999, None);
        check!(!r.found_valid_elem);
        check!(!c.valid());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn find_time_forward_stays_put_when_already_there() {
        let mut s = storage(8);
        s.append(5, &note(1), false, None);
        s.append(6, &note(2), false, None);
        let mut c = s.create_cursor();
        let r = c.find_time_forward(&s, 0, None);
        check!(r.found_valid_elem);
        check!(r.n_processed == 0);
        check!(c.is_at_start(&s));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn find_fn_forward_uses_the_predicate() {
        let mut s = storage(8);
        s.append(0, &[0x90, 1, 1], false, None);
        s.append(1, &[0x80, 2, 2], false, None);
        let mut c = s.create_cursor();
        let r = c.find_fn_forward(&s, |e| e.data()[0] == 0x80, None);
        check!(r.found_valid_elem);
        let_assert!(Some(e) = c.get(&s));
        check!(e.time == 1);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cursor_re_anchors_after_its_element_is_dropped() {
        let mut s = storage(2);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        let mut c = s.create_cursor();
        check!(c.get(&s).map(|e| e.time) == Some(0));

        // Overwrite drops the element the cursor pointed at.
        s.append(2, &note(3), true, None);
        c.sync(&s);
        // Re-anchored to the new oldest message rather than reading a dropped one.
        check!(c.get(&s).map(|e| e.time) == Some(1));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn cursor_untouched_by_drops_it_does_not_point_at() {
        let mut s = storage(3);
        s.append(0, &note(1), false, None);
        s.append(1, &note(2), false, None);
        s.append(2, &note(3), false, None);
        let mut c = s.create_cursor();
        c.next(&s); // now on time 1
        s.append(3, &note(4), true, None); // drops time 0
        c.sync(&s);
        check!(c.get(&s).map(|e| e.time) == Some(1));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn clear_invalidates_pre_existing_cursors() {
        let mut s = storage(4);
        for i in 0..3 {
            s.append(i, &note(i as u8), false, None);
        }
        let mut c = s.create_cursor();
        c.next(&s);
        check!(c.valid());

        s.clear();
        // Enough new content that the stale cursor's index lands *inside* the
        // post-clear window. Only the watermark tells it apart from a cursor that
        // merely had its message overwritten, which would re-anchor instead.
        for i in 0..3 {
            s.append(100 + i, &note(i as u8), false, None);
        }
        check!(c.index() == Some(1));
        check!(s.first_index() == 3);
        c.sync(&s);
        check!(!c.valid());

        // A cursor made after the clear works normally.
        let fresh = s.create_cursor();
        check!(fresh.get(&s).map(|e| e.time) == Some(100));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_head_drops_newer_messages() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut dropped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| dropped.push(e.time);
            s.truncate(15, TruncateSide::Head, Some(&mut cb));
        }
        check!(dropped == vec![20, 30]);
        check!(times(&s) == vec![0, 10]);
        check!(s.n_events() == 2);
        // first_index is untouched: nothing was dropped from the old end.
        check!(s.first_index() == 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_tail_drops_older_messages() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut dropped = Vec::new();
        {
            let mut cb = |e: &MidiStorageElem| dropped.push(e.time);
            s.truncate(15, TruncateSide::Tail, Some(&mut cb));
        }
        check!(dropped == vec![0, 10]);
        check!(times(&s) == vec![20, 30]);
        check!(s.first_index() == 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_leaves_untouched_storage_alone() {
        let mut s = storage(8);
        for t in [10u32, 20] {
            s.append(t, &note(t as u8), false, None);
        }
        // Newest is not past the cut: head truncation is a no-op.
        s.truncate(99, TruncateSide::Head, None);
        check!(times(&s) == vec![10, 20]);
        // Oldest is not before the cut: tail truncation is a no-op.
        s.truncate(0, TruncateSide::Tail, None);
        check!(times(&s) == vec![10, 20]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_can_empty_the_storage() {
        let mut s = storage(8);
        s.append(10, &note(1), false, None);
        s.truncate(0, TruncateSide::Head, None);
        check!(s.n_events() == 0);
        check!(s.is_empty());
        // Still usable afterwards.
        check!(s.append(50, &note(2), false, None));
        check!(times(&s) == vec![50]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_on_empty_storage_is_a_no_op() {
        let mut s = storage(4);
        s.truncate(5, TruncateSide::Head, None);
        s.truncate(5, TruncateSide::Tail, None);
        check!(s.n_events() == 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_fn_uses_the_predicate() {
        let mut s = storage(8);
        s.append(0, &[0x90, 1, 1], false, None);
        s.append(1, &[0x90, 2, 2], false, None);
        s.append(2, &[0x80, 3, 3], false, None);
        s.truncate_fn(|e| e.data()[0] == 0x80, TruncateSide::Head, None);
        check!(times(&s) == vec![0, 1]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_head_checks_the_newest_message_first() {
        let mut s = storage(8);
        s.append(0, &[0x90, 1, 1], false, None);
        s.append(1, &[0x80, 2, 2], false, None);
        s.append(2, &[0x90, 3, 3], false, None);
        // Non-monotone predicate: only the middle message matches. Because the
        // newest does not, nothing is dropped at all -- head truncation decides
        // on the newest message and never scans for an interior match.
        s.truncate_fn(|e| e.data()[0] == 0x80, TruncateSide::Head, None);
        check!(times(&s) == vec![0, 1, 2]);

        // Once the newest matches, the scan runs and cuts from the first match.
        s.append(3, &[0x80, 4, 4], false, None);
        s.truncate_fn(|e| e.data()[0] == 0x80, TruncateSide::Head, None);
        check!(times(&s) == vec![0]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_tail_invalidates_cursors_rather_than_re_anchoring() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut c = s.create_cursor(); // on time 0
        s.truncate(15, TruncateSide::Tail, None);
        c.sync(&s);
        // The message it pointed at was deliberately discarded, so unlike an
        // append overwrite this does not slide forward to the new oldest.
        check!(!c.valid());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_keeps_cursors_still_inside_the_window() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut c = s.create_cursor();
        c.next(&s);
        c.next(&s); // on time 20
        s.truncate(15, TruncateSide::Tail, None);
        c.sync(&s);
        check!(c.get(&s).map(|e| e.time) == Some(20));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn truncate_head_invalidates_cursors_past_the_new_end() {
        let mut s = storage(8);
        for t in [0u32, 10, 20, 30] {
            s.append(t, &note(t as u8), false, None);
        }
        let mut c = s.create_cursor();
        c.next(&s);
        c.next(&s); // on time 20
        s.truncate(15, TruncateSide::Head, None);
        c.sync(&s);
        check!(!c.valid());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn contents_equal_ignores_time() {
        let mut s = storage(4);
        s.append(0, &note(60), false, None);
        s.append(9, &note(60), false, None);
        let v: Vec<_> = s.iter().copied().collect();
        check!(v[0].contents_equal(&v[1]));
        check!(v[0] != v[1]); // times differ
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn zero_capacity_storage_accepts_nothing() {
        let mut s = storage(0);
        check!(!s.append(0, &note(1), false, None));
        let c = s.create_cursor();
        check!(!c.valid());
    }
}
