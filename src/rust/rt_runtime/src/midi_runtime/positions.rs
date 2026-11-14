use super::{Pool, Store, Version, NEXT_SCOPE};
use crate::midi_state::{BasicUpdate, MidiState, BASIC_STATE_CELLS};
use crate::{Error, Frame};
use alloc::{boxed::Box, vec};
use core::sync::atomic::Ordering;

#[derive(Clone, Copy)]
enum Phase {
    Copy(usize),
    Read,
    Apply(BasicUpdate),
    Ready,
}
#[derive(Clone, Copy)]
struct Job {
    expected: Version,
    version: Version,
    target: Frame,
    after: Option<(Frame, u16)>,
    phase: Phase,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PositionId {
    scope: u32,
    slot: u32,
    generation: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionStatus {
    Building,
    Ready,
}
pub struct PreparedPosition {
    pub expected: Version,
    pub version: Version,
    pub target: Frame,
    pub after: Option<(Frame, u16)>,
    pub state: MidiState,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PositionWork {
    pub seed_copies: usize,
    pub state_cells: usize,
    pub lookups: usize,
    pub update_steps: usize,
}
pub struct PositionJobs {
    scope: u32,
    jobs: Pool<Job>,
    generations: Box<[u64]>,
    seeds: Box<[MidiState]>,
    states: Box<[MidiState]>,
    work: PositionWork,
    exhausted: bool,
}
impl PositionJobs {
    pub fn prepare(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > u32::MAX as usize {
            return Err(Error::InvalidConfig);
        }
        capacity
            .checked_mul(2 * size_of::<MidiState>() + size_of::<Job>() + 32)
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(Self {
            scope,
            jobs: Pool::new(capacity),
            generations: vec![1; capacity].into_boxed_slice(),
            seeds: vec![MidiState::default(); capacity].into_boxed_slice(),
            states: vec![MidiState::default(); capacity].into_boxed_slice(),
            work: PositionWork::default(),
            exhausted: false,
        })
    }
    pub fn allocated_bytes(&self) -> usize {
        self.jobs.allocated_bytes()
            + core::mem::size_of_val(&*self.generations)
            + core::mem::size_of_val(&*self.seeds)
            + core::mem::size_of_val(&*self.states)
    }
    pub fn work(&self) -> PositionWork {
        self.work
    }
    fn job(&self, id: PositionId) -> Result<Job, Error> {
        if id.scope != self.scope
            || self.generations.get(id.slot.wrapping_sub(1) as usize) != Some(&id.generation)
        {
            return Err(Error::InvalidId);
        }
        self.jobs.live(id.slot)
    }
    pub fn begin(
        &mut self,
        store: &mut Store,
        version: Version,
        seed: &MidiState,
        target: Frame,
    ) -> Result<PositionId, Error> {
        let view = store.view(version)?;
        if view
            .visible
            .is_some_and(|(start, end)| target.0 < start || target.0 > end)
        {
            return Err(Error::InvalidFrame);
        }
        if !seed.is_basic() {
            return Err(Error::InvalidParameter);
        }
        if self.jobs.available() == 0 {
            return Err(if self.exhausted {
                Error::Exhausted
            } else {
                Error::Capacity
            });
        }
        let pinned = store.retain(version)?;
        let slot = self.jobs.insert(Job {
            expected: version,
            version: pinned,
            target,
            after: None,
            phase: Phase::Copy(0),
        });
        self.seeds[slot as usize - 1] = *seed;
        self.work.seed_copies = self.work.seed_copies.saturating_add(1);
        Ok(PositionId {
            scope: self.scope,
            slot,
            generation: self.generations[slot as usize - 1],
        })
    }
    pub fn progress(
        &mut self,
        store: &mut Store,
        id: PositionId,
        steps: usize,
    ) -> Result<PositionStatus, Error> {
        let mut job = self.job(id)?;
        for _ in 0..steps {
            match job.phase {
                Phase::Ready => break,
                Phase::Copy(cell) => {
                    let index = id.slot as usize - 1;
                    self.states[index].copy_basic_cell(Some(&self.seeds[index]), cell);
                    self.work.state_cells = self.work.state_cells.saturating_add(1);
                    job.phase = if cell + 1 == BASIC_STATE_CELLS {
                        Phase::Read
                    } else {
                        Phase::Copy(cell + 1)
                    };
                }
                Phase::Read => {
                    self.work.lookups = self.work.lookups.saturating_add(1);
                    job.phase = match store.first(job.version, job.after)? {
                        Some(event) if event.frame < job.target => {
                            job.after = Some((event.frame, event.order));
                            Phase::Apply(BasicUpdate::begin(event.message())?)
                        }
                        _ => Phase::Ready,
                    };
                }
                Phase::Apply(mut update) => {
                    self.work.update_steps = self.work.update_steps.saturating_add(1);
                    job.phase = if update.step(&mut self.states[id.slot as usize - 1]) {
                        Phase::Read
                    } else {
                        Phase::Apply(update)
                    };
                }
            }
        }
        self.jobs.replace(id.slot, job);
        Ok(if matches!(job.phase, Phase::Ready) {
            PositionStatus::Ready
        } else {
            PositionStatus::Building
        })
    }
    fn release_slot(&mut self, id: PositionId) {
        assert!(self.jobs.release(id.slot));
        let generation = &mut self.generations[id.slot as usize - 1];
        if let Some(next) = generation.checked_add(1) {
            *generation = next;
            self.jobs.reclaim(id.slot);
        } else {
            self.exhausted = true;
        }
    }
    pub fn finish(&mut self, id: PositionId) -> Result<PreparedPosition, Error> {
        let job = self.job(id)?;
        if !matches!(job.phase, Phase::Ready) {
            return Err(Error::Busy);
        }
        let state = self.states[id.slot as usize - 1];
        self.release_slot(id);
        Ok(PreparedPosition {
            expected: job.expected,
            version: job.version,
            target: job.target,
            after: job.after,
            state,
        })
    }
    pub fn cancel(&mut self, store: &mut Store, id: PositionId) -> Result<(), Error> {
        let job = self.job(id)?;
        store.release(job.version)?;
        self.release_slot(id);
        Ok(())
    }
}
