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
        self.bytes[..self.size as usize] == other.bytes[..self.size as usize]
    }
}

/// Direction for truncate operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncateSide {
    TruncateHead,
    TruncateTail,
}

/// Callback for dropped messages
pub type DroppedMsgCallback = Box<dyn Fn(u32, u16, &[u8]) + Send + Sync>;

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
            data: vec![MidiStorageElem {
                time: 0,
                size: 0,
                bytes: [0; 4],
            }; capacity],
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

    pub fn get_elem(&mut self, idx: u32) -> Option<&mut MidiStorageElem> {
        self.data.get_mut(idx as usize)
    }

    pub fn get_elem_ref(&self, idx: u32) -> Option<&MidiStorageElem> {
        self.data.get(idx as usize)
    }

    /// Append a message to the storage.
    /// Returns true if successful, false if the buffer is full.
    pub fn append(
        &mut self,
        time: u32,
        size: u16,
        data: &[u8],
        allow_replace: bool,
        dropped_cb: Option<&DroppedMsgCallback>,
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

        // Handle buffer full condition
        if self.full() {
            if let Some(cb) = dropped_cb {
                if let Some(elem) = self.data.get(self.tail as usize) {
                    cb(elem.time, elem.size, elem.data());
                }
            }
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

        true
    }

    /// Prepend a message to the storage (at the tail).
    /// Returns true if successful, false if the buffer is full.
    pub fn prepend(
        &mut self,
        time: u32,
        size: u16,
        data: &[u8],
        dropped_cb: Option<&DroppedMsgCallback>,
    ) -> bool {
        if self.full() {
            if let Some(cb) = dropped_cb {
                cb(time, size, data);
            }
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

    pub fn clear(&mut self) {
        self.tail = 0;
        self.head = 0;
        self.n_events = 0;
    }

    pub fn truncate(
        &mut self,
        time: u32,
        side: TruncateSide,
        dropped_cb: Option<&DroppedMsgCallback>,
    ) {
        match side {
            TruncateSide::TruncateTail => {
                self.truncate_fn(
                    |t, _, _| t < time,
                    side,
                    dropped_cb,
                );
            }
            TruncateSide::TruncateHead => {
                self.truncate_fn(
                    |t, _, _| t > time,
                    side,
                    dropped_cb,
                );
            }
        }
    }

    pub fn truncate_fn<F>(
        &mut self,
        should_truncate: F,
        side: TruncateSide,
        dropped_cb: Option<&DroppedMsgCallback>,
    ) where
        F: Fn(u32, u16, &[u8]) -> bool,
    {
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
                    if !should_truncate(elem.time, elem.size, elem.data()) {
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut kept = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if should_truncate(elem.time, elem.size, elem.data()) {
                            break;
                        }
                    }
                    kept += 1;
                    idx = (idx + 1) % capacity;
                }

                if let Some(cb) = dropped_cb {
                    let mut drop_idx = idx;
                    for _ in kept..self.n_events {
                        if let Some(elem) = self.data.get(drop_idx as usize) {
                            cb(elem.time, elem.size, elem.data());
                        }
                        drop_idx = (drop_idx + 1) % capacity;
                    }
                }

                self.head = idx;
                self.n_events = kept;
            }
            TruncateSide::TruncateTail => {
                if let Some(elem) = self.data.get(self.tail as usize) {
                    if !should_truncate(elem.time, elem.size, elem.data()) {
                        return;
                    }
                }

                let mut idx = self.tail;
                let mut dropped = 0u32;
                for _ in 0..self.n_events {
                    if let Some(elem) = self.data.get(idx as usize) {
                        if !should_truncate(elem.time, elem.size, elem.data()) {
                            break;
                        }
                    }
                    if let Some(cb) = dropped_cb {
                        if let Some(elem) = self.data.get(idx as usize) {
                            cb(elem.time, elem.size, elem.data());
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
}

/// FindResult for cursor search operations
#[derive(Debug, Clone)]
pub struct FindResult {
    pub n_processed: u32,
    pub found_valid_elem: bool,
}