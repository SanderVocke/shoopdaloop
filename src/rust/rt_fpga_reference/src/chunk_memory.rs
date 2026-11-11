//! Synchronous chunk-memory reference. Buffering and transfer timing are implementation details.

use alloc::vec::Vec;
use core::ops::Range;

use crate::{fixed::Fixed, storage::ChunkId, Error};

pub struct ChunkMemory {
    chunk_frames: usize,
    samples: Vec<Fixed>,
}

impl ChunkMemory {
    pub fn new(chunk_frames: usize) -> Result<Self, Error> {
        if chunk_frames == 0 {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            chunk_frames,
            samples: Vec::new(),
        })
    }

    pub fn from_samples(chunk_frames: usize, samples: Vec<Fixed>) -> Result<Self, Error> {
        let mut memory = Self::new(chunk_frames)?;
        if samples.len() % chunk_frames != 0 {
            return Err(Error::InvalidShape);
        }
        memory.samples = samples;
        Ok(memory)
    }

    pub fn chunk_frames(&self) -> usize {
        self.chunk_frames
    }
    pub fn slot_count(&self) -> usize {
        self.samples.len() / self.chunk_frames
    }
    pub fn slot_capacity(&self) -> usize {
        self.samples.capacity() / self.chunk_frames
    }
    pub fn samples(&self) -> &[Fixed] {
        &self.samples
    }

    pub fn reserve_slots(&mut self, slots: usize) -> Result<(), Error> {
        let words = slots
            .checked_mul(self.chunk_frames)
            .ok_or(Error::MemoryFull)?;
        self.samples
            .try_reserve(words.saturating_sub(self.samples.len()))
            .map_err(|_| Error::MemoryFull)
    }

    /// Allocation policy remains with the caller; new physical slots start zeroed.
    pub fn grow_zeroed(&mut self, slots: usize) -> Result<(), Error> {
        if slots < self.slot_count() {
            return Err(Error::InvalidShape);
        }
        self.reserve_slots(slots)?;
        self.samples.resize(slots * self.chunk_frames, Fixed::ZERO);
        Ok(())
    }

    fn span(&self, chunk: ChunkId, offset: usize, count: usize) -> Result<Range<usize>, Error> {
        if chunk.0 >= self.slot_count() {
            return Err(Error::InvalidView);
        }
        if offset > self.chunk_frames || count > self.chunk_frames - offset {
            return Err(Error::InvalidShape);
        }
        let start = chunk.0 * self.chunk_frames + offset;
        Ok(start..start + count)
    }

    pub fn read(&self, chunk: ChunkId, offset: usize, output: &mut [Fixed]) -> Result<(), Error> {
        let span = self.span(chunk, offset, output.len())?;
        output.copy_from_slice(&self.samples[span]);
        Ok(())
    }

    pub fn write(&mut self, chunk: ChunkId, offset: usize, samples: &[Fixed]) -> Result<(), Error> {
        let span = self.span(chunk, offset, samples.len())?;
        self.samples[span].copy_from_slice(samples);
        Ok(())
    }

    pub fn prefetch(&self, chunk: ChunkId) -> Result<(), Error> {
        self.span(chunk, 0, 0).map(|_| ())
    }

    pub fn fence(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// The caller has removed all logical references. Payload is retained for slot reuse.
    pub fn retire(&mut self, chunk: ChunkId) -> Result<(), Error> {
        self.span(chunk, 0, 0).map(|_| ())
    }
}
