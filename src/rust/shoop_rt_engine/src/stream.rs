//! The render half of bulk reads: a reader node follows a resource cursor over an
//! exported medium and copies several pages per callback into a byte ring, within
//! the resource copy budget. The control endpoint cuts chunks from the ring.

use alloc::boxed::Box;
use alloc::vec;

use crate::cells::Cells;
use crate::node::EngineOp;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{OperationContext, ProcessBlock, Processor};
use rt_runtime::resource::Cursor;
use rt_runtime::Error;
use shoop_loops::runtime::LoopPlanner;

/// Pages the reader may copy per callback.
pub(crate) const READ_PAGES: usize = 4;
/// Pages the ring holds: several callbacks of reading between two endpoint polls.
pub(crate) const RING_PAGES: usize = 16;

/// Bytes from the reader node to the control endpoint: one producer, one
/// consumer, no locks.
pub(crate) struct ByteRing {
    bytes: Box<[AtomicU8]>,
    written: AtomicU64,
    taken: AtomicU64,
    failed: AtomicBool,
}

impl ByteRing {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            bytes: (0..capacity.max(1)).map(|_| AtomicU8::new(0)).collect(),
            written: AtomicU64::new(0),
            taken: AtomicU64::new(0),
            failed: AtomicBool::new(false),
        }
    }
    fn capacity(&self) -> u64 {
        self.bytes.len() as u64
    }
    /// Room for the producer.
    fn free(&self) -> usize {
        let used = self.written.load(Ordering::Relaxed) - self.taken.load(Ordering::Acquire);
        (self.capacity() - used) as usize
    }
    fn push(&self, data: &[u8]) {
        let written = self.written.load(Ordering::Relaxed);
        for (index, byte) in data.iter().enumerate() {
            let at = (written + index as u64) % self.capacity();
            self.bytes[at as usize].store(*byte, Ordering::Relaxed);
        }
        self.written
            .store(written + data.len() as u64, Ordering::Release);
    }
    fn fail(&self) {
        self.failed.store(true, Ordering::Release);
    }

    /// Bytes ready for the consumer.
    pub(crate) fn available(&self) -> usize {
        (self.written.load(Ordering::Acquire) - self.taken.load(Ordering::Relaxed)) as usize
    }
    /// Copy up to `output.len()` ready bytes out of the ring.
    pub(crate) fn take(&self, output: &mut [u8]) -> usize {
        let taken = self.taken.load(Ordering::Relaxed);
        let count = output.len().min(self.available());
        for (index, byte) in output[..count].iter_mut().enumerate() {
            let at = (taken + index as u64) % self.capacity();
            *byte = self.bytes[at as usize].load(Ordering::Relaxed);
        }
        self.taken.store(taken + count as u64, Ordering::Release);
        count
    }
    /// Drop up to `count` ready bytes.
    pub(crate) fn skip(&self, count: usize) -> usize {
        let count = count.min(self.available());
        self.taken.fetch_add(count as u64, Ordering::Release);
        count
    }
    /// Drop every byte and the failure flag; only while the reader is stopped.
    pub(crate) fn reset(&self) {
        self.taken
            .store(self.written.load(Ordering::Acquire), Ordering::Release);
        self.failed.store(false, Ordering::Release);
    }
    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

/// Streams one medium at a time into the ring; inactive without a cursor.
pub struct BulkReader {
    cells: Arc<Cells>,
    cursor: Option<Cursor>,
    role_cursors: Option<[Cursor; 2]>,
    role_frame_bytes: usize,
    role_filled: usize,
    remaining: u64,
    scratch: Box<[u8]>,
    per_callback: usize,
    callback: Option<rt_runtime::Frame>,
    available: usize,
}

impl BulkReader {
    pub(crate) fn new(cells: Arc<Cells>, page_bytes: usize) -> Self {
        Self {
            cells,
            cursor: None,
            role_cursors: None,
            role_frame_bytes: 0,
            role_filled: 0,
            remaining: 0,
            scratch: vec![0; page_bytes.max(8 * shoop_rt_protocol::MAX_STRIP_CHANNELS)]
                .into_boxed_slice(),
            per_callback: READ_PAGES * page_bytes,
            callback: None,
            available: 0,
        }
    }
}

impl Processor<LoopPlanner, EngineOp> for BulkReader {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(block)
    }
}

impl BulkReader {
    pub(crate) fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        self.render_resources(block.callback_start, &mut block.resources)
    }
    pub(crate) fn render_resources(
        &mut self,
        callback_start: rt_runtime::Frame,
        resources: &mut rt_runtime::resource::ResourceIo<'_>,
    ) -> Result<(), Error> {
        let budget = resources.copy_remaining();
        self.render_external(callback_start, budget, |cursor, bytes| {
            resources.read(cursor, bytes)
        })
    }
    pub(crate) fn render_external(
        &mut self,
        callback_start: rt_runtime::Frame,
        budget: usize,
        mut read: impl FnMut(&mut Cursor, &mut [u8]) -> Result<usize, Error>,
    ) -> Result<(), Error> {
        #[cfg(test)]
        if self.cells.fail_reader_tick.swap(false, Ordering::Relaxed) {
            return Err(Error::InvalidState);
        }
        if self.cursor.is_none() && self.role_cursors.is_none() {
            return Ok(());
        }
        let ring = &self.cells.bulk;
        if Some(callback_start) != self.callback {
            self.callback = Some(callback_start);
            self.available = self.per_callback;
        }
        let mut budget = self.available.min(budget);
        if let Some(mut cursors) = self.role_cursors {
            let frame_bytes = self.role_frame_bytes * 2;
            while self.remaining > 0 {
                if self.role_filled == frame_bytes {
                    if ring.free() < frame_bytes {
                        break;
                    }
                    ring.push(&self.scratch[..frame_bytes]);
                    self.remaining -= frame_bytes as u64;
                    self.role_filled = 0;
                    continue;
                }
                if budget == 0 {
                    break;
                }
                let role = self.role_filled / self.role_frame_bytes;
                let count =
                    (self.role_frame_bytes - self.role_filled % self.role_frame_bytes).min(budget);
                match read(
                    &mut cursors[role],
                    &mut self.scratch[self.role_filled..self.role_filled + count],
                ) {
                    Ok(read) if read > 0 => {
                        self.role_filled += read;
                        budget -= read;
                        self.available -= read;
                    }
                    _ => {
                        ring.fail();
                        self.remaining = 0;
                    }
                }
            }
            self.role_cursors = (self.remaining > 0).then_some(cursors);
            return Ok(());
        }
        let mut cursor = self.cursor.ok_or(Error::InvalidState)?;
        while self.remaining > 0 && budget > 0 {
            let count = self
                .scratch
                .len()
                .min(budget)
                .min(ring.free())
                .min(self.remaining.min(usize::MAX as u64) as usize);
            if count == 0 {
                break;
            }
            match read(&mut cursor, &mut self.scratch[..count]) {
                Ok(read) if read > 0 => {
                    ring.push(&self.scratch[..read]);
                    budget -= read;
                    self.available -= read;
                    self.remaining -= read as u64;
                }
                _ => {
                    ring.fail();
                    self.remaining = 0;
                }
            }
        }
        self.cursor = (self.remaining > 0).then_some(cursor);
        Ok(())
    }
}

impl BulkReader {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::ReadStart { cursor, bytes } => {
                self.cursor = Some(cursor);
                self.role_cursors = None;
                self.role_filled = 0;
                self.remaining = bytes;
            }
            EngineOp::ReadRolesStart {
                cursors,
                bytes,
                channels,
            } => {
                let width = usize::from(channels);
                if width == 0
                    || width > shoop_rt_protocol::MAX_STRIP_CHANNELS
                    || !bytes.is_multiple_of(width as u64 * 8)
                {
                    return Err(Error::InvalidShape);
                }
                self.cursor = None;
                self.role_cursors = Some(cursors);
                self.role_frame_bytes = width * 4;
                self.role_filled = 0;
                self.remaining = bytes;
            }
            EngineOp::ReadStop => {
                self.cursor = None;
                self.role_cursors = None;
                self.role_filled = 0;
                self.remaining = 0;
            }
            _ => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}
