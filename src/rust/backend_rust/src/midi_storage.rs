//! MIDI storage implementation in Rust.

/// MIDI storage element - a single MIDI message with inline payload.
/// Shared with C++ via CXX bridge.
#[repr(C)]
pub struct MidiStorageElem {
    /// Position: absolute in storage, relative in buffers
    pub time: u32,
    /// Size of the data portion (1..4)
    pub size: u16,
    /// Inline 4-byte payload
    pub bytes: [u8; 4],
}

impl Clone for MidiStorageElem {
    fn clone(&self) -> Self {
        MidiStorageElem {
            time: self.time,
            size: self.size,
            bytes: self.bytes,
        }
    }
}

impl PartialEq for MidiStorageElem {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.size == other.size && self.bytes == other.bytes
    }
}

impl MidiStorageElem {
    pub fn new(time: u32, size: u16, data: &[u8]) -> Option<Self> {
        if size as usize > data.len() || size > 4 {
            return None;
        }
        let mut elem = MidiStorageElem {
            time,
            size,
            bytes: [0; 4],
        };
        elem.bytes[..size as usize].copy_from_slice(&data[..size as usize]);
        Some(elem)
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.size as usize]
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[..self.size as usize]
    }

    /// Compare only the MIDI data bytes (ignores time field)
    pub fn contents_equal(&self, other: &MidiStorageElem) -> bool {
        if self.size != other.size {
            return false;
        }
        self.bytes[..self.size as usize] == other.bytes[..other.size as usize]
    }
}

/// Direction for truncate operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncateSide {
    TruncateHead,
    TruncateTail,
}

/// MidiTimeWindow - Handles time-window logic for MIDI ringbuffer operations.
///
/// This tracks the current buffer's time range and provides operations for:
/// - Setting/configuring the time window size (n_samples)
/// - Advancing time with next_buffer()
/// - Adding messages with put()
/// - Creating snapshots with snapshot()
pub struct MidiTimeWindow {
    /// Number of samples in the time window
    n_samples: u32,
    /// Start time of the current buffer (stored, not computed)
    current_buffer_start_time: u32,
    /// End time of the current buffer
    current_buffer_end_time: u32,
}

impl MidiTimeWindow {
    pub fn new() -> Self {
        MidiTimeWindow {
            n_samples: 0,
            current_buffer_start_time: 0,
            current_buffer_end_time: 0,
        }
    }

    /// Set the number of samples in the time window
    pub fn set_n_samples(&mut self, n: u32) {
        self.n_samples = n;
    }

    /// Get the number of samples in the time window
    pub fn get_n_samples(&self) -> u32 {
        self.n_samples
    }

    /// Get the start time of the current buffer
    /// Returns the stored value, matching C++ behavior where this is set during next_buffer()
    pub fn get_current_start_time(&self) -> u32 {
        self.current_buffer_start_time
    }

    /// Get the end time of the current buffer
    pub fn get_current_end_time(&self) -> u32 {
        self.current_buffer_end_time
    }

    /// Advance time by n_frames, truncate old messages, and invoke callback for any dropped messages.
    /// Callback signature: fn(time: u32, size: u16, data: *const u8, user_data: *mut std::ffi::c_void)
    pub fn next_buffer<C: FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>(
        &mut self,
        storage: &mut MidiStorageCore,
        n_frames: u32,
        mut dropped_cb: Option<C>,
        user_data: *mut std::ffi::c_void,
    ) {
        // Capture n_samples for non-overflow path
        let n_samples = self.n_samples;
        let old_end = self.current_buffer_end_time;
        let new_end = old_end.wrapping_add(n_frames);

        // Check for overflow using wrapping comparison
        let overflow = new_end < old_end;

        let adjusted_new_end: u32;
        let adjusted_old_end: u32;
        if overflow {
            // Overflow detected: re-read n_samples fresh (matching C++ behavior)
            // Use plain i32 arithmetic to compute shift, matching C++'s (int)moved_new_end - (int)new_end
            let n_samples_fresh = self.n_samples;
            let shift = (n_samples_fresh as i32).wrapping_sub(new_end as i32);

            // Shift all messages in storage
            storage.for_each_msg_modify(|t, _s, _d| {
                *t = ((*t as i32).wrapping_add(shift)) as u32;
            });

            // Compute shifted boundary values using wrapping arithmetic (matching C++'s new_end += shift)
            adjusted_new_end = ((new_end as i32).wrapping_add(shift)) as u32;
            adjusted_old_end = ((old_end as i32).wrapping_add(shift)) as u32;
        } else {
            adjusted_new_end = new_end;
            adjusted_old_end = old_end;
        }

        // Truncate old messages (TruncateTail) and report dropped ones
        // Use captured n_samples value for truncation boundary
        let min_time = adjusted_new_end.saturating_sub(n_samples);
        storage.truncate(
            min_time,
            TruncateSide::TruncateTail,
            dropped_cb.as_mut(),
            user_data,
        );

        // Store the shifted start/end times (matching C++ behavior)
        self.current_buffer_start_time = adjusted_old_end;
        self.current_buffer_end_time = adjusted_new_end;
    }

    /// Put a message at a specific frame within the current buffer.
    /// Callback is invoked if the append drops a message (buffer was full).
    ///
    /// Note: Uses the stored current_buffer_start_time for timestamp calculation,
    /// matching C++ behavior where this value is set during next_buffer().
    pub fn put<C: FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>(
        &mut self,
        storage: &mut MidiStorageCore,
        frame_in_current_buffer: u32,
        size: u16,
        data: &[u8],
        mut dropped_cb: Option<C>,
        user_data: *mut std::ffi::c_void,
    ) -> bool {
        // Use stored current_buffer_start_time, matching C++ behavior
        let time = self.current_buffer_start_time + frame_in_current_buffer;

        // Check if time is in range
        if time > self.current_buffer_end_time {
            return false;
        }

        storage.append(time, size, data, true, dropped_cb.as_mut(), user_data)
    }

    /// Create a snapshot of the current state.
    /// Copies messages to the target storage, truncates to the time window,
    /// and adjusts timestamps so the window start is considered zero.
    pub fn snapshot(
        &self,
        storage: &MidiStorageCore,
        target: &mut MidiStorageCore,
        start_offset_from_end: u32,
    ) {
        // Copy all elements to target
        storage.copy_to(target);

        // Calculate the minimum time to keep
        let end = self.current_buffer_end_time;
        let start_from_end = if start_offset_from_end != 0 {
            start_offset_from_end
        } else {
            self.n_samples
        };
        let min_message_time = end.wrapping_sub(std::cmp::min(end, start_from_end));

        // Truncate to the time window (TruncateTail) - no dropped callback needed
        // since this is operating on a copy
        target.truncate_no_callback(min_message_time, TruncateSide::TruncateTail);

        // Adjust timestamps to start from zero
        target.for_each_msg_modify(|t, _s, _d| {
            *t = t.wrapping_sub(min_message_time);
        });
    }
}

/// Core MIDI storage with ringbuffer implementation.
pub struct MidiStorageCore {
    data: Vec<MidiStorageElem>,
    tail: u32,
    head: u32,
    n_events: u32,
}

impl MidiStorageCore {
    pub fn new(data_size_bytes: u32) -> Self {
        let capacity = data_size_bytes as usize / std::mem::size_of::<MidiStorageElem>();
        MidiStorageCore {
            data: vec![
                MidiStorageElem {
                    time: 0,
                    size: 0,
                    bytes: [0; 4],
                };
                capacity
            ],
            tail: 0,
            head: 0,
            n_events: 0,
        }
    }

    pub fn capacity(&self) -> u32 {
        self.data.len() as u32
    }

    pub fn n_events(&self) -> u32 {
        self.n_events
    }

    pub fn full(&self) -> bool {
        self.n_events == self.data.len() as u32
    }

    pub fn empty(&self) -> bool {
        self.n_events == 0
    }

    pub fn raw_tail(&self) -> u32 {
        self.tail
    }

    pub fn raw_head(&self) -> u32 {
        self.head
    }

    pub fn raw_full(&self) -> bool {
        self.n_events == self.data.len() as u32
    }

    /// Get element at a raw physical offset (0 to capacity-1).
    /// This is the actual array index, not related to logical order.
    /// Used by cursor which works with raw physical offsets (tail, head, etc.)
    pub fn get_elem_at_physical_offset(&mut self, idx: u32) -> Option<&mut MidiStorageElem> {
        self.data.get_mut(idx as usize)
    }

    /// Get element at a raw physical offset (const version).
    /// This is the actual array index, not related to logical order.
    pub fn get_elem_at_physical_offset_ref(&self, idx: u32) -> Option<&MidiStorageElem> {
        self.data.get(idx as usize)
    }

    /// Get element at a raw physical offset (mutable version).
    /// This is the actual array index, not related to logical order.
    pub fn get_elem_at_physical_offset_mut(&mut self, idx: u32) -> Option<&mut MidiStorageElem> {
        self.data.get_mut(idx as usize)
    }

    /// Get element at logical index (0 = oldest message, increasing toward newest).
    /// Logical index 0 corresponds to physical offset `tail`, index 1 to `(tail+1) % capacity`, etc.
    pub fn get_elem_logical(&mut self, idx: u32) -> Option<&mut MidiStorageElem> {
        if idx >= self.n_events {
            return None;
        }
        let phys_idx = (self.tail + idx) % self.data.len() as u32;
        self.data.get_mut(phys_idx as usize)
    }

    /// Get element at logical index (const version).
    /// Logical index 0 corresponds to physical offset `tail`, index 1 to `(tail+1) % capacity`, etc.
    pub fn get_elem_logical_ref(&self, idx: u32) -> Option<&MidiStorageElem> {
        if idx >= self.n_events {
            return None;
        }
        let phys_idx = (self.tail + idx) % self.data.len() as u32;
        self.data.get(phys_idx as usize)
    }

    /// Append a message to the storage.
    /// If allow_replace is true and the buffer is full, the oldest message is dropped.
    /// The dropped_cb callback (if provided) is invoked once for each dropped message.
    pub fn append<C: FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>(
        &mut self,
        time: u32,
        size: u16,
        data: &[u8],
        allow_replace: bool,
        mut dropped_cb: Option<&mut C>,
        user_data: *mut std::ffi::c_void,
    ) -> bool {
        if self.full() && !allow_replace {
            return false;
        }

        // Check for out-of-order messages
        if self.n_events > 0 {
            let newest_idx = if self.head == 0 {
                self.data.len() - 1
            } else {
                self.head as usize - 1
            };
            if let Some(elem) = self.data.get(newest_idx) {
                if elem.time > time {
                    return false;
                }
            }
        }

        // Handle buffer full condition - capture and report dropped element
        if self.full() {
            let dropped_elem = self.data[self.tail as usize].clone();
            self.tail = (self.tail + 1) % self.data.len() as u32;

            if let Some(ref mut cb) = dropped_cb {
                cb(
                    dropped_elem.time,
                    dropped_elem.size,
                    dropped_elem.data().as_ptr(),
                    user_data,
                );
            }
        }

        // Write the new element
        if let Some(elem) = self.data.get_mut(self.head as usize) {
            elem.time = time;
            elem.size = size;
            elem.bytes[..size as usize].copy_from_slice(&data[..size as usize]);
        }

        self.head = (self.head + 1) % self.data.len() as u32;
        if self.n_events < self.data.len() as u32 {
            self.n_events += 1;
        }

        true
    }

    /// Prepend a message to the storage (at the tail).
    pub fn prepend(&mut self, time: u32, size: u16, data: &[u8]) -> bool {
        if self.full() {
            return false;
        }

        // Check for out-of-order messages (for prepend, expect earlier times)
        if self.n_events > 0 {
            if let Some(elem) = self.data.get(self.tail as usize) {
                if elem.time < time {
                    return false;
                }
            }
        }

        let new_tail = if self.tail == 0 {
            self.data.len() - 1
        } else {
            self.tail as usize - 1
        };
        self.tail = new_tail as u32;
        self.n_events += 1;

        if let Some(elem) = self.data.get_mut(self.tail as usize) {
            elem.time = time;
            elem.size = size;
            elem.bytes[..size as usize].copy_from_slice(&data[..size as usize]);
        }

        true
    }

    /// Clear all elements from the storage.
    pub fn clear(&mut self) {
        self.tail = 0;
        self.head = 0;
        self.n_events = 0;
    }

    /// Truncate the storage, invoking callback for each dropped message.
    pub fn truncate<C: FnMut(u32, u16, *const u8, *mut std::ffi::c_void)>(
        &mut self,
        time: u32,
        side: TruncateSide,
        mut dropped_cb: Option<&mut C>,
        user_data: *mut std::ffi::c_void,
    ) {
        let capacity = self.data.len() as u32;
        if capacity == 0 || self.n_events == 0 {
            return;
        }

        match side {
            TruncateSide::TruncateHead => {
                let newest_idx = if self.head == 0 {
                    (capacity - 1) as usize
                } else {
                    self.head as usize - 1
                };

                if let Some(elem) = self.data.get(newest_idx as usize) {
                    if elem.time <= time {
                        // Nothing to truncate
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut kept = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time > time {
                            break;
                        }
                    }
                    kept += 1;
                    idx = (idx + 1) % capacity;
                }

                // Report dropped messages
                if let Some(ref mut cb) = dropped_cb {
                    for i in 0..kept {
                        let drop_idx = (self.tail + i) % capacity;
                        let elem = &self.data[drop_idx as usize];
                        cb(elem.time, elem.size, elem.data().as_ptr(), user_data);
                    }
                }

                self.head = idx;
                self.n_events = kept;
            }
            TruncateSide::TruncateTail => {
                if let Some(elem) = self.data.get(self.tail as usize) {
                    if elem.time >= time {
                        // Nothing to truncate
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut dropped = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time >= time {
                            break;
                        }
                    }
                    dropped += 1;
                    idx = (idx + 1) % capacity;
                }

                // Report dropped messages
                if let Some(ref mut cb) = dropped_cb {
                    for i in 0..dropped {
                        let drop_idx = (self.tail + i) % capacity;
                        let elem = &self.data[drop_idx as usize];
                        cb(elem.time, elem.size, elem.data().as_ptr(), user_data);
                    }
                }

                self.tail = idx;
                self.n_events -= dropped;
                if self.n_events == 0 {
                    self.head = self.tail;
                }
            }
        }
    }

    /// Truncate without callback (for snapshot target which doesn't need to report drops)
    pub fn truncate_no_callback(&mut self, time: u32, side: TruncateSide) {
        let capacity = self.data.len() as u32;
        if capacity == 0 || self.n_events == 0 {
            return;
        }

        match side {
            TruncateSide::TruncateHead => {
                let newest_idx = if self.head == 0 {
                    (capacity - 1) as usize
                } else {
                    self.head as usize - 1
                };

                if let Some(elem) = self.data.get(newest_idx as usize) {
                    if elem.time <= time {
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut kept = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time > time {
                            break;
                        }
                    }
                    kept += 1;
                    idx = (idx + 1) % capacity;
                }

                self.head = idx;
                self.n_events = kept;
            }
            TruncateSide::TruncateTail => {
                if let Some(elem) = self.data.get(self.tail as usize) {
                    if elem.time >= time {
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut dropped = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time >= time {
                            break;
                        }
                    }
                    dropped += 1;
                    idx = (idx + 1) % capacity;
                }

                self.tail = idx;
                self.n_events -= dropped;
                if self.n_events == 0 {
                    self.head = self.tail;
                }
            }
        }
    }

    /// Copy all elements to another storage
    pub fn copy_to(&self, target: &mut MidiStorageCore) {
        target.data.resize(
            self.data.len(),
            MidiStorageElem {
                time: 0,
                size: 0,
                bytes: [0; 4],
            },
        );
        target.tail = 0;
        target.n_events = self.n_events;

        if self.n_events == 0 {
            target.head = 0;
            return;
        }

        let mut count = 0u32;
        let mut idx = self.tail;
        while count < self.n_events {
            target.data[count as usize] = self.data[idx as usize].clone();
            idx = (idx + 1) % self.data.len() as u32;
            count += 1;
        }
        target.head = count % self.data.len() as u32;
    }

    /// Copy all elements from another storage
    pub fn copy_from(&mut self, source: &MidiStorageCore) {
        self.data.resize(
            source.data.len(),
            MidiStorageElem {
                time: 0,
                size: 0,
                bytes: [0; 4],
            },
        );
        self.tail = source.tail;
        self.head = source.head;
        self.n_events = source.n_events;

        if self.n_events == 0 {
            return;
        }

        let mut count = 0u32;
        let mut idx = source.tail;
        while count < self.n_events {
            self.data[idx as usize] = source.data[idx as usize].clone();
            idx = (idx + 1) % source.data.len() as u32;
            count += 1;
        }
    }

    /// Iterate over all elements with mutable access
    pub fn for_each_msg_modify<F>(&mut self, mut cb: F)
    where
        F: FnMut(&mut u32, &mut u16, &mut [u8]),
    {
        if self.data.is_empty() {
            return;
        }

        let mut idx = self.tail as usize;
        let capacity = self.data.len();
        for _ in 0..self.n_events {
            if let Some(elem) = self.data.get_mut(idx) {
                let size = elem.size as usize;
                let time_ref = &mut elem.time;
                let size_ref = &mut elem.size;
                let bytes_ref = &mut elem.bytes[..size];
                cb(time_ref, size_ref, bytes_ref);
            }
            idx = (idx + 1) % capacity;
        }
    }

    /// Iterate over all elements with immutable access
    pub fn for_each_msg<F>(&self, cb: F)
    where
        F: Fn(u32, u16, &[u8]),
    {
        if self.data.is_empty() {
            return;
        }

        let mut idx = self.tail as usize;
        let capacity = self.data.len();
        for _ in 0..self.n_events {
            if let Some(elem) = self.data.get(idx) {
                cb(elem.time, elem.size, elem.data());
            }
            idx = (idx + 1) % capacity;
        }
    }
}

/// Cursor for iterating over MIDI storage
pub struct MidiCursor {
    offset: Option<u32>,
    prev_offset: Option<u32>,
}

impl MidiCursor {
    pub fn new() -> Self {
        MidiCursor {
            offset: None,
            prev_offset: None,
        }
    }

    pub fn valid(&self) -> bool {
        self.offset.is_some()
    }

    pub fn offset(&self) -> Option<u32> {
        self.offset
    }

    pub fn prev_offset(&self) -> Option<u32> {
        self.prev_offset
    }

    pub fn invalidate(&mut self) {
        self.offset = None;
        self.prev_offset = None;
    }

    pub fn wrapped(&self) -> bool {
        match (self.prev_offset, self.offset) {
            (Some(prev), Some(curr)) => prev > curr,
            _ => false,
        }
    }

    /// Reset cursor to the beginning of storage
    pub fn reset(&mut self, storage: &MidiStorageCore) {
        let n_events = storage.n_events();
        if n_events == 0 {
            self.invalidate();
        } else {
            self.offset = Some(storage.raw_tail());
            self.prev_offset = None;
        }
    }

    /// Move to the next element
    pub fn next(&mut self, storage: &MidiStorageCore) {
        if !self.offset.is_some() {
            return;
        }

        let cap = storage.capacity();
        if cap == 0 {
            self.invalidate();
            return;
        }

        let curr = self.offset.unwrap();
        let next = (curr + 1) % cap;

        let raw_full = storage.raw_full();
        let head = storage.raw_head();
        let tail = storage.raw_tail();

        // Edge case: single element capacity
        if cap == 1 {
            self.invalidate();
            return;
        }

        // Check if we've reached the end
        if !raw_full && next == head {
            self.invalidate();
            return;
        }

        if raw_full && next == tail && self.prev_offset.is_some() {
            self.invalidate();
            return;
        }

        self.prev_offset = self.offset;
        self.offset = Some(next);
    }

    /// Overwrite cursor position
    pub fn overwrite(&mut self, offset: u32, prev_offset: u32) {
        self.offset = Some(offset);
        self.prev_offset = Some(prev_offset);
    }

    /// Find first element where time >= target
    pub fn find_time_forward(&mut self, storage: &MidiStorageCore, time: u32) -> FindResult {
        self.find_fn_forward(storage, |elem| elem.time >= time)
    }

    /// Find first element matching predicate function
    pub fn find_fn_forward<F>(&mut self, storage: &MidiStorageCore, mut pred: F) -> FindResult
    where
        F: FnMut(&MidiStorageElem) -> bool,
    {
        let mut rval = FindResult {
            n_processed: 0,
            found_valid_elem: false,
        };

        if !self.valid() {
            return rval;
        }

        let cap = storage.capacity();
        let n_events = storage.n_events();
        if cap == 0 {
            self.invalidate();
            return rval;
        }

        let head = storage.raw_head();
        let tail = storage.raw_tail();
        let raw_full = storage.raw_full();

        let mut idx = self.offset.unwrap();
        let mut prev_idx = idx;

        for step in 0..n_events {
            // Check if we've gone past the end (except for first iteration from current pos)
            if step > 0 {
                if !raw_full && idx == head {
                    break;
                }
                if raw_full && idx == tail {
                    break;
                }
            }

            if let Some(elem) = storage.get_elem_at_physical_offset_ref(idx) {
                if pred(elem) {
                    if step > 0 {
                        self.prev_offset = Some(prev_idx);
                        self.offset = Some(idx);
                    }
                    rval.found_valid_elem = true;
                    return rval;
                }
            }

            prev_idx = idx;
            idx = (idx + 1) % cap;
            rval.n_processed += 1;
        }

        self.invalidate();
        rval
    }
}

/// FindResult for cursor search operations
#[derive(Debug, Clone)]
pub struct FindResult {
    pub n_processed: u32,
    pub found_valid_elem: bool,
}
