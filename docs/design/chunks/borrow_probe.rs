//! Executable borrowing experiment, not a production allocator or ownership API.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Full,
    Invalid,
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkRef {
    Silence,
    Persistent { slot: usize, generation: u64 },
    Transient { slot: usize, epoch: u64 },
}

#[derive(Clone, Copy)]
struct Meta {
    generation: u64,
    live: bool,
}

struct Persistent<const S: usize> {
    samples: Box<[[f32; S]]>,
    meta: Box<[Meta]>,
    free: Vec<usize>,
}

struct Arena<const S: usize> {
    samples: Box<[[f32; S]]>,
    used: usize,
    epoch: u64,
}

pub struct AudioMemory<const S: usize> {
    persistent: Persistent<S>,
    arena: Arena<S>,
    silence: [f32; S],
}

pub struct TransientBatch {
    first: usize,
    count: usize,
    epoch: u64,
}

impl TransientBatch {
    pub fn chunk(&self, index: usize) -> Result<ChunkRef, Error> {
        if index >= self.count {
            return Err(Error::Invalid);
        }
        Ok(ChunkRef::Transient {
            slot: self.first + index,
            epoch: self.epoch,
        })
    }
}

pub struct ReadView<'a, const S: usize> {
    before: &'a [[f32; S]],
    after: &'a [[f32; S]],
    excluded: Option<usize>,
    meta: &'a [Meta],
    transient: &'a [[f32; S]],
    epoch: u64,
    silence: &'a [f32; S],
}

impl<'a, const S: usize> ReadView<'a, S> {
    pub fn get(&self, chunk: ChunkRef) -> Result<&'a [f32; S], Error> {
        match chunk {
            ChunkRef::Silence => Ok(self.silence),
            ChunkRef::Transient { slot, epoch } => {
                if epoch != self.epoch {
                    return Err(Error::Invalid);
                }
                self.transient.get(slot).ok_or(Error::Invalid)
            }
            ChunkRef::Persistent { slot, generation } => {
                let meta = self.meta.get(slot).ok_or(Error::Invalid)?;
                if !meta.live || meta.generation != generation {
                    return Err(Error::Invalid);
                }
                match self.excluded {
                    None => self.before.get(slot),
                    Some(excluded) if slot < excluded => self.before.get(slot),
                    Some(excluded) if slot > excluded => self.after.get(slot - excluded - 1),
                    Some(_) => None,
                }
                .ok_or(Error::Invalid)
            }
        }
    }
}

impl<const S: usize> AudioMemory<S> {
    pub fn prepare(persistent_slots: usize, arena_slots: usize) -> Result<Self, Error> {
        if S == 0 || persistent_slots == 0 || arena_slots == 0 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            persistent: Persistent {
                samples: vec![[0.; S]; persistent_slots].into_boxed_slice(),
                meta: vec![
                    Meta {
                        generation: 1,
                        live: false
                    };
                    persistent_slots
                ]
                .into_boxed_slice(),
                free: (0..persistent_slots).rev().collect(),
            },
            arena: Arena {
                samples: vec![[0.; S]; arena_slots].into_boxed_slice(),
                used: 0,
                epoch: 1,
            },
            silence: [0.; S],
        })
    }

    pub fn produce_transient<F>(&mut self, render: F) -> Result<ChunkRef, Error>
    where
        F: for<'a> FnOnce(ReadView<'a, S>, &'a mut [f32; S]) -> Result<(), Error>,
    {
        let slot = self.arena.used;
        if slot == self.arena.samples.len() {
            return Err(Error::Full);
        }
        let (previous, destination) = self.arena.samples.split_at_mut(slot);
        let input = ReadView {
            before: &self.persistent.samples,
            after: &[],
            excluded: None,
            meta: &self.persistent.meta,
            transient: previous,
            epoch: self.arena.epoch,
            silence: &self.silence,
        };
        render(input, &mut destination[0])?;
        self.arena.used += 1;
        Ok(ChunkRef::Transient {
            slot,
            epoch: self.arena.epoch,
        })
    }

    pub fn produce_persistent<F>(&mut self, render: F) -> Result<ChunkRef, Error>
    where
        F: for<'a> FnOnce(ReadView<'a, S>, &'a mut [f32; S]) -> Result<(), Error>,
    {
        let slot = self.persistent.free.pop().ok_or(Error::Full)?;
        let (before, destination) = self.persistent.samples.split_at_mut(slot);
        let (destination, after) = destination.split_at_mut(1);
        let input = ReadView {
            before,
            after,
            excluded: Some(slot),
            meta: &self.persistent.meta,
            transient: &self.arena.samples[..self.arena.used],
            epoch: self.arena.epoch,
            silence: &self.silence,
        };
        if let Err(error) = render(input, &mut destination[0]) {
            self.persistent.free.push(slot);
            return Err(error);
        }
        self.persistent.meta[slot].live = true;
        Ok(ChunkRef::Persistent {
            slot,
            generation: self.persistent.meta[slot].generation,
        })
    }

    pub fn produce_transient_batch<F>(
        &mut self,
        count: usize,
        render: F,
    ) -> Result<TransientBatch, Error>
    where
        F: for<'a> FnOnce(ReadView<'a, S>, &'a mut [[f32; S]]) -> Result<(), Error>,
    {
        if count == 0 {
            return Err(Error::Invalid);
        }
        let first = self.arena.used;
        let end = first.checked_add(count).ok_or(Error::Full)?;
        if end > self.arena.samples.len() {
            return Err(Error::Full);
        }
        let (previous, destinations) = self.arena.samples.split_at_mut(first);
        let input = ReadView {
            before: &self.persistent.samples,
            after: &[],
            excluded: None,
            meta: &self.persistent.meta,
            transient: previous,
            epoch: self.arena.epoch,
            silence: &self.silence,
        };
        render(input, &mut destinations[..count])?;
        self.arena.used = end;
        Ok(TransientBatch {
            first,
            count,
            epoch: self.arena.epoch,
        })
    }

    /// The experiment has one owner per persistent chunk. A production index
    /// supplies ownership and queues release only after the final owner leaves.
    pub fn release(&mut self, chunk: ChunkRef) -> Result<(), Error> {
        let ChunkRef::Persistent { slot, generation } = chunk else {
            return Err(Error::Invalid);
        };
        let meta = self.persistent.meta.get_mut(slot).ok_or(Error::Invalid)?;
        if !meta.live || meta.generation != generation {
            return Err(Error::Invalid);
        }
        let next = generation.checked_add(1).ok_or(Error::Exhausted)?;
        meta.live = false;
        meta.generation = next;
        self.persistent.free.push(slot);
        Ok(())
    }

    pub fn finish_quantum(&mut self) -> Result<(), Error> {
        let epoch = self.arena.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        self.arena.used = 0;
        self.arena.epoch = epoch;
        Ok(())
    }
}

#[repr(C, align(64))]
pub struct QuadNode {
    pub generation: u32,
    pub references: u32,
    pub kind: u32,
    pub reserved: u32,
    pub entries: [u32; 4],
}

const _: [(); 64] = [(); core::mem::size_of::<QuadNode>()];

#[repr(C, align(64))]
pub struct OctNode {
    pub generation: u32,
    pub references: u32,
    pub kind: u32,
    pub reserved: u32,
    pub entries: [u32; 8],
}

const _: [(); 64] = [(); core::mem::size_of::<OctNode>()];
