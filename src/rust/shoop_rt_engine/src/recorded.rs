//! Applied settings retained independently for the old and candidate wet takes.
use crate::cells::{Cells, LoopCells};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use rt_buffered::{Backend, SourceObserver};
use rt_runtime::{Error, Frame};
use shoop_loops::runtime::LoopPlanner;
use shoop_rt_protocol::{processors::STATE_SCALARS, ProcessorKind, RecordedProcessorState};

pub(crate) fn validate_state(
    kind: Option<ProcessorKind>,
    recorded: Option<RecordedProcessorState>,
) -> Result<(), shoop_rt_protocol::Refusal> {
    let Some(recorded) = recorded else {
        return Ok(());
    };
    if kind != Some(recorded.kind) {
        return Err(shoop_rt_protocol::Refusal::InvalidValue);
    }
    let mut state = shoop_processor_protocol::state::State::new(recorded.kind)
        .map_err(|_| shoop_rt_protocol::Refusal::InvalidValue)?;
    state.values = recorded.values;
    state
        .validate()
        .map_err(|_| shoop_rt_protocol::Refusal::InvalidValue)
}

impl crate::ControlEndpoint {
    pub(crate) fn validate_recorded(
        &self,
        id: shoop_rt_protocol::LoopSlotId,
        recorded: Option<RecordedProcessorState>,
    ) -> Result<(), shoop_rt_protocol::Refusal> {
        let kind = self
            .model
            .find_loop(id)
            .and_then(|entry| entry.strip.and_then(|id| self.model.strip(id)))
            .and_then(|strip| strip.processor)
            .map(|owner| owner.kind);
        self.validate_recorded_kind(kind, recorded)
    }
    pub(crate) fn validate_recorded_kind(
        &self,
        kind: Option<ProcessorKind>,
        recorded: Option<RecordedProcessorState>,
    ) -> Result<(), shoop_rt_protocol::Refusal> {
        validate_state(kind, recorded)?;
        if let Some(state) = recorded.filter(|s| s.kind == ProcessorKind::OxiSynth) {
            if !self.processor_catalog.presets.iter().any(|preset| {
                (u32::from(preset.bank) * 128 + u32::from(preset.program)) as f32 == state.values[2]
            }) {
                return Err(shoop_rt_protocol::Refusal::InvalidValue);
            }
        }
        Ok(())
    }
}

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
    pub(crate) fn publish(
        &self,
        cells: &LoopCells,
        generation: u32,
        revision: u64,
        state: Option<RecordedProcessorState>,
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
        bank.frame
            .store(state.map_or(0, |s| s.source_frame), Ordering::SeqCst);
        bank.kind
            .store(state.map_or(0, |s| s.kind as u32), Ordering::SeqCst);
        for (cell, value) in bank
            .values
            .iter()
            .zip(state.map_or([0.; STATE_SCALARS], |s| s.values))
        {
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
        (!self.done && self.frame >= start && self.frame < end && self.source.is_some())
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
                .publish(cell, id.generation(), revision.0, Some(state));
        }
        Ok(())
    }
}
