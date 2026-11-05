//! Applied settings retained independently for the old and candidate wet takes.
use crate::cells::{Cells, LoopCells};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use rt_buffered::{Backend, SourceObserver};
use rt_runtime::{Error, Frame};
use shoop_loops::runtime::LoopPlanner;
use shoop_rt_protocol::{processors::STATE_SCALARS, ProcessorKind, RecordedProcessorState};

struct Bank {
    sequence: AtomicU64,
    generation: AtomicU32,
    revision: AtomicU64,
    frame: AtomicU64,
    kind: AtomicU32,
    values: [AtomicU32; STATE_SCALARS],
}
impl Default for Bank {
    fn default() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            generation: AtomicU32::new(0),
            revision: AtomicU64::new(0),
            frame: AtomicU64::new(0),
            kind: AtomicU32::new(0),
            values: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}
#[derive(Default)]
pub(crate) struct TakeCells {
    banks: [Bank; 2],
}
impl TakeCells {
    fn publish(
        &self,
        cells: &LoopCells,
        generation: u32,
        revision: u64,
        state: RecordedProcessorState,
    ) {
        let committed = cells.content_revision.load(Ordering::Relaxed);
        let previous = &self.banks[0];
        let bank = &self.banks[usize::from(
            previous.generation.load(Ordering::Relaxed) == generation
                && previous.revision.load(Ordering::Relaxed) == committed,
        )];
        bank.sequence.fetch_add(1, Ordering::SeqCst);
        bank.generation.store(generation, Ordering::SeqCst);
        bank.revision.store(revision, Ordering::SeqCst);
        bank.frame.store(state.source_frame, Ordering::SeqCst);
        bank.kind.store(state.kind as u32, Ordering::SeqCst);
        for (cell, value) in bank.values.iter().zip(state.values) {
            cell.store(value.to_bits(), Ordering::SeqCst);
        }
        bank.sequence.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn read(
        &self,
        generation: u32,
        revision: u64,
    ) -> Result<Option<RecordedProcessorState>, ()> {
        for bank in &self.banks {
            let sequence = bank.sequence.load(Ordering::SeqCst);
            if sequence % 2 != 0 {
                return Err(());
            }
            let matching = bank.generation.load(Ordering::SeqCst) == generation
                && bank.revision.load(Ordering::SeqCst) == revision;
            let kind = match bank.kind.load(Ordering::SeqCst) {
                1 => Some(ProcessorKind::BuiltInFx),
                2 => Some(ProcessorKind::OxiSynth),
                _ => None,
            };
            let state = RecordedProcessorState {
                source_frame: bank.frame.load(Ordering::SeqCst),
                kind: kind.unwrap_or(ProcessorKind::External),
                values: core::array::from_fn(|index| {
                    f32::from_bits(bank.values[index].load(Ordering::SeqCst))
                }),
            };
            if sequence != bank.sequence.load(Ordering::SeqCst) {
                return Err(());
            }
            if matching && kind.is_some() {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }
}

pub(crate) struct TakeObserver<'a> {
    pub domain: &'a LoopPlanner,
    pub cells: Option<&'a Cells>,
    pub source: Option<u32>,
    pub kind: ProcessorKind,
    pub frame: Frame,
    pub done: bool,
}
impl SourceObserver for TakeObserver<'_> {
    fn next(&self, start: Frame, end: Frame) -> Option<Frame> {
        (!self.done
            && self.frame >= start
            && self.frame < end
            && self
                .source
                .is_some_and(|source| self.domain.recording_starts(source).next().is_some()))
        .then_some(self.frame)
    }
    fn observe(&mut self, frame: Frame, backend: &dyn Backend) -> Result<(), Error> {
        self.done = true;
        let Some(cells) = self.cells else {
            return Ok(());
        };
        let Some(source) = self.source else {
            return Ok(());
        };
        let mut state = RecordedProcessorState {
            source_frame: frame.0,
            kind: self.kind,
            values: [0.; STATE_SCALARS],
        };
        for index in 0..self.kind.scalar_count() {
            state.values[index] = backend
                .parameter(self.kind.scalar_id(index).ok_or(Error::InvalidParameter)?)
                .ok_or(Error::InvalidParameter)?;
        }
        for (id, revision) in self.domain.recording_starts(source) {
            let cell = cells
                .loops
                .get(id.slot() as usize)
                .ok_or(Error::InvalidId)?;
            cell.recorded
                .publish(cell, id.generation(), revision.0, state);
        }
        Ok(())
    }
}
