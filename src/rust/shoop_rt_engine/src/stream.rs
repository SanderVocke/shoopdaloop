//! The render half of bulk reads: a reader node follows a resource cursor over an
//! exported medium and copies several pages per callback into a byte ring, within
//! the resource copy budget. The control endpoint cuts chunks from the ring.

use crate::cells::Cells;
use crate::node::EngineOp;
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{OperationContext, ProcessBlock, Processor};
use rt_runtime::resource::Cursor;
use rt_runtime::Error;
use shoop_loops::runtime::LoopPlanner;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

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
    remaining: u64,
    scratch: Box<[u8]>,
    per_callback: usize,
}

impl BulkReader {
    pub(crate) fn new(cells: Arc<Cells>, page_bytes: usize) -> Self {
        Self {
            cells,
            cursor: None,
            remaining: 0,
            scratch: vec![0; page_bytes].into_boxed_slice(),
            per_callback: READ_PAGES * page_bytes,
        }
    }
}

impl Processor<LoopPlanner, EngineOp> for BulkReader {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        match operation {
            EngineOp::ReadStart { cursor, bytes } => {
                self.cursor = Some(cursor);
                self.remaining = bytes;
            }
            EngineOp::ReadStop => {
                self.cursor = None;
                self.remaining = 0;
            }
            _ => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let Some(mut cursor) = self.cursor else {
            return Ok(());
        };
        let ring = &self.cells.bulk;
        let mut budget = self.per_callback.min(block.resources.copy_remaining());
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
            match block
                .resources
                .read(&mut cursor, &mut self.scratch[..count])
            {
                Ok(read) if read > 0 => {
                    ring.push(&self.scratch[..read]);
                    budget -= read;
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
