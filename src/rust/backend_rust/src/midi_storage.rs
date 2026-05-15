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

// Implement Clone manually since we can't derive on repr(C) with non-C types
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

/// Result of an append operation, including any dropped message
#[derive(Clone)]
pub struct AppendResult {
    pub success: bool,
    pub dropped: bool,
    pub dropped_elem: Option<MidiStorageElem>,
}

impl Default for AppendResult {
    fn default() -> Self {
        AppendResult {
            success: false,
            dropped: false,
            dropped_elem: None,
        }
    }
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
    /// Start time of the current buffer
    current_buffer_start_time: u32,
    /// End time of the current buffer
    current_buffer_end_time: u32,
    /// Preview buffer for truncate - stores elements that would be dropped
    preview_dropped: Vec<MidiStorageElem>,
}

impl MidiTimeWindow {
    pub fn new() -> Self {
        MidiTimeWindow {
            n_samples: 0,
            current_buffer_start_time: 0,
            current_buffer_end_time: 0,
            preview_dropped: Vec::new(),
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
    pub fn get_current_start_time(&self) -> u32 {
        self.current_buffer_end_time.saturating_sub(self.n_samples)
    }

    /// Get the end time of the current buffer
    pub fn get_current_end_time(&self) -> u32 {
        self.current_buffer_end_time
    }

    /// Clear the preview buffer
    pub fn clear_preview(&mut self) {
        self.preview_dropped.clear();
    }

    /// Get the number of elements in the preview buffer
    pub fn preview_count(&self) -> u32 {
        self.preview_dropped.len() as u32
    }

    /// Get the nth element from the preview buffer
    pub fn get_preview_elem(&self, idx: u32) -> Option<&MidiStorageElem> {
        self.preview_dropped.get(idx as usize)
    }

    /// Preview what would be dropped when advancing to a new end time.
    /// Returns the number of messages that would be dropped.
    /// Call get_preview_elem(0..n) to retrieve each dropped message.
    /// 
    /// This is used to allow the caller to handle dropped messages before
    /// the actual truncation is performed.
    pub fn truncate_preview(&mut self, storage: &mut MidiStorageCore, new_end: u32) -> u32 {
        self.clear_preview();
        
        if self.n_samples == 0 {
            return 0;
        }

        let min_time = new_end.saturating_sub(self.n_samples);
        
        // Walk through storage to find elements before min_time (TruncateTail)
        // This mirrors the C++ truncate_fn behavior
        let capacity = storage.capacity();
        let n_events = storage.n_events();
        
        if capacity == 0 || n_events == 0 {
            return 0;
        }

        let mut idx = storage.raw_tail();
        for _ in 0..n_events {
            if let Some(elem) = storage.get_elem_ref(idx) {
                if elem.time >= min_time {
                    break; // found element to keep
                }
                // This element would be truncated - add to preview
                self.preview_dropped.push(elem.clone());
            }
            idx = (idx + 1) % capacity;
        }
        
        self.preview_dropped.len() as u32
    }

    /// Perform the actual truncate operation.
    /// Should be called after truncate_preview() and after handling dropped messages.
    pub fn truncate_doit(&mut self, storage: &mut MidiStorageCore, new_end: u32) {
        if self.n_samples == 0 {
            return;
        }

        let min_time = new_end.saturating_sub(self.n_samples);
        storage.truncate_doit(min_time, TruncateSide::TruncateTail);
    }

    /// Advance time by n_frames and truncate old messages.
    /// Returns the number of messages that were dropped (for preview).
    pub fn next_buffer_preview(&mut self, storage: &mut MidiStorageCore, n_frames: u32) -> u32 {
        let old_end = self.current_buffer_end_time;
        let (new_end, _overflow) = old_end.overflowing_add(n_frames);

        // Check for overflow and handle timestamp shift
        let adjusted_new_end = if new_end < old_end {
            // Overflow - need to shift all timestamps
            let shifted_new_end = self.n_samples;
            let shift = shifted_new_end as i32 - new_end as i32;
            
            // Shift all messages in storage
            storage.for_each_msg_modify(|t, _s, _d| {
                *t = (*t as i32 + shift) as u32;
            });
            shifted_new_end
        } else {
            new_end
        };

        // Preview truncate
        self.truncate_preview(storage, adjusted_new_end)
    }

    /// Perform the actual next_buffer operation.
    /// Should be called after next_buffer_preview() and handling dropped messages.
    pub fn next_buffer_doit(&mut self, storage: &mut MidiStorageCore, n_frames: u32) {
        let old_end = self.current_buffer_end_time;
        let (new_end, _overflow) = old_end.overflowing_add(n_frames);

        // Check for overflow and handle timestamp shift
        let adjusted_new_end = if new_end < old_end {
            let shifted_new_end = self.n_samples;
            let shift = shifted_new_end as i32 - new_end as i32;
            
            storage.for_each_msg_modify(|t, _s, _d| {
                *t = (*t as i32 + shift) as u32;
            });
            shifted_new_end
        } else {
            new_end
        };

        self.truncate_doit(storage, adjusted_new_end);
        self.current_buffer_start_time = old_end;
        self.current_buffer_end_time = adjusted_new_end;
    }

    /// Put a message at a specific frame within the current buffer.
    /// Returns (success, dropped_flag).
    pub fn put(&mut self, storage: &mut MidiStorageCore, frame_in_current_buffer: u32, 
               size: u16, data: &[u8]) -> AppendResult {
        let time = self.current_buffer_start_time + frame_in_current_buffer;
        
        // Check if time is in range
        if time > self.current_buffer_end_time {
            return AppendResult::default();
        }

        storage.append(time, size, data, true)
    }

    /// Create a snapshot of the current state.
    /// Copies messages to the target storage, truncates to the time window,
    /// and adjusts timestamps so the window start is considered zero.
    pub fn snapshot(&self, storage: &MidiStorageCore, target: &mut MidiStorageCore,
                    start_offset_from_end: u32) {
        // Copy all elements to target
        storage.copy_to(target);

        // Calculate the minimum time to keep
        let end = self.current_buffer_end_time;
        let start_from_end = if start_offset_from_end != 0 {
            start_offset_from_end
        } else {
            self.n_samples
        };
        let min_message_time = end.saturating_sub(start_from_end);

        // Truncate to the time window (TruncateTail)
        // Walk through and remove elements before min_message_time
        let capacity = target.capacity();
        let n_events = target.n_events();
        
        if capacity == 0 || n_events == 0 {
            return;
        }

        let mut idx = target.raw_tail();
        let mut _drop_count = 0u32;
        
        for _ in 0..n_events {
            if let Some(elem) = target.get_elem_ref(idx) {
                if elem.time >= min_message_time {
                    break;
                }
            }
            _drop_count += 1;
            idx = (idx + 1) % capacity;
        }

        // Actually truncate
        target.truncate_doit(min_message_time, TruncateSide::TruncateTail);

        // Adjust timestamps to start from zero
        target.for_each_msg_modify(|t, _s, _d| {
            *t = t.saturating_sub(min_message_time);
        });
    }
}

/// Core MIDI storage with ringbuffer implementation.
pub struct MidiStorageCore {
    data: Vec<MidiStorageElem>,
    tail: u32,
    head: u32,
    n_events: u32,
    // Preview buffer for truncate - stores elements that would be dropped
    preview_dropped: Vec<MidiStorageElem>,
    // Index of last dropped element from append (INVALID_OFFSET if none)
    last_dropped_idx: u32,
}

impl MidiStorageCore {
    pub fn new(data_size_bytes: u32) -> Self {
        let capacity = data_size_bytes as usize / std::mem::size_of::<MidiStorageElem>();
        const INVALID: u32 = 0xFFFFFFFF;
        MidiStorageCore {
            data: vec![MidiStorageElem {
                time: 0,
                size: 0,
                bytes: [0; 4],
            }; capacity],
            tail: 0,
            head: 0,
            n_events: 0,
            preview_dropped: Vec::new(),
            last_dropped_idx: INVALID,
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

    pub fn get_elem(&mut self, idx: u32) -> Option<&mut MidiStorageElem> {
        self.data.get_mut(idx as usize)
    }

    pub fn get_elem_ref(&self, idx: u32) -> Option<&MidiStorageElem> {
        self.data.get(idx as usize)
    }

    /// Append a message to the storage.
    /// Returns (success, dropped_message_time, dropped_message_size, dropped_message_data).
    /// Only one message can be dropped from append.
    pub fn append(
        &mut self,
        time: u32,
        size: u16,
        data: &[u8],
        allow_replace: bool,
    ) -> AppendResult {
        const INVALID: u32 = 0xFFFFFFFF;
        
        if self.full() && !allow_replace {
            return AppendResult::default();
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
                    return AppendResult::default();
                }
            }
        }

        // Handle buffer full condition - capture dropped element
        self.last_dropped_idx = INVALID;
        if self.full() {
            self.last_dropped_idx = self.tail;
            self.tail = (self.tail + 1) % self.data.len() as u32;
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

        let dropped_elem = if self.last_dropped_idx != INVALID {
            self.data.get(self.last_dropped_idx as usize).map(|e| e.clone())
        } else {
            None
        };

        AppendResult {
            success: true,
            dropped: self.last_dropped_idx != INVALID,
            dropped_elem,
        }
    }
    
    /// Get the index of the last dropped element (INVALID if none)
    pub fn get_last_dropped_idx(&self) -> u32 {
        self.last_dropped_idx
    }

    /// Prepend a message to the storage (at the tail).
    /// Returns (success, dropped_message_time, dropped_message_size, dropped_message_data).
    pub fn prepend(
        &mut self,
        time: u32,
        size: u16,
        data: &[u8],
    ) -> AppendResult {
        if self.full() {
            return AppendResult {
                success: false,
                dropped: true,  // prepend fails if full
                dropped_elem: Some(MidiStorageElem {
                    time,
                    size,
                    bytes: {
                        let mut arr = [0u8; 4];
                        arr[..size as usize].copy_from_slice(&data[..size as usize]);
                        arr
                    },
                }),
            };
        }

        // Check for out-of-order messages (for prepend, expect earlier times)
        if self.n_events > 0 {
            if let Some(elem) = self.data.get(self.tail as usize) {
                if elem.time < time {
                    return AppendResult::default();
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

        AppendResult {
            success: true,
            dropped: false,
            dropped_elem: None,
        }
    }

    /// Clear all elements from the storage.
    pub fn clear(&mut self) {
        self.tail = 0;
        self.head = 0;
        self.n_events = 0;
        self.preview_dropped.clear();
    }

    /// Preview what would be dropped if we truncate to a specific time.
    /// Returns the number of messages that would be dropped.
    /// Call get_preview_elem(0..n) to retrieve each dropped message.
    pub fn truncate_preview(&mut self, time: u32, side: TruncateSide) -> u32 {
        self.preview_dropped.clear();
        
        let capacity = self.data.len() as u32;
        if capacity == 0 || self.n_events == 0 {
            return 0;
        }

        match side {
            TruncateSide::TruncateHead => {
                // TruncateHead: remove elements where time > target
                // Walk from tail forward, stop when we find an element to keep
                let mut idx = self.tail;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time > time {
                            break;  // found element to keep
                        }
                        // This element would be truncated - add to preview
                        self.preview_dropped.push(elem.clone());
                    }
                    idx = (idx + 1) % capacity;
                }
                self.preview_dropped.len() as u32
            }
            TruncateSide::TruncateTail => {
                // TruncateTail: remove elements where time < target
                // Walk from tail forward, count elements to drop
                let mut idx = self.tail;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if elem.time >= time {
                            break;  // found element to keep
                        }
                        // This element would be truncated - add to preview
                        self.preview_dropped.push(elem.clone());
                    }
                    idx = (idx + 1) % capacity;
                }
                self.preview_dropped.len() as u32
            }
        }
    }

    /// Get the nth element from the truncate preview buffer.
    /// Returns None if idx is out of range.
    pub fn get_preview_elem(&self, idx: u32) -> Option<&MidiStorageElem> {
        self.preview_dropped.get(idx as usize)
    }

    /// Get the number of elements in the truncate preview buffer.
    pub fn preview_count(&self) -> u32 {
        self.preview_dropped.len() as u32
    }

    /// Perform the actual truncate operation.
    /// Should be called after truncate_preview() and after handling dropped messages.
    pub fn truncate_doit(&mut self, time: u32, side: TruncateSide) {
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

                self.tail = idx;
                self.n_events -= dropped;
                if self.n_events == 0 {
                    self.head = self.tail;
                }
            }
        }
        
        // Clear preview after truncate
        self.preview_dropped.clear();
    }

    /// Clear the preview buffer without performing truncate.
    pub fn clear_preview(&mut self) {
        self.preview_dropped.clear();
    }

    /// Copy all elements to another storage
    pub fn copy_to(&self, target: &mut MidiStorageCore) {
        target.data.resize(self.data.len(), MidiStorageElem {
            time: 0,
            size: 0,
            bytes: [0; 4],
        });
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
        self.data.resize(source.data.len(), MidiStorageElem {
            time: 0,
            size: 0,
            bytes: [0; 4],
        });
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

            if let Some(elem) = storage.get_elem_ref(idx) {
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