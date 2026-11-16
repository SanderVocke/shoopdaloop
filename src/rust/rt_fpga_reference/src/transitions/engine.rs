use super::*;
use crate::state::{sync::PlannedTransition, StateProcessor};
use alloc::{vec, vec::Vec};

#[derive(Clone, Copy)]
struct Record {
    id: RequestId,
    command: Command,
    effective: u64,
    outcome: Option<Outcome>,
}

/// Control-side request admission; loop execution and active plans belong to SP.
pub struct TransitionProcessor {
    records: Vec<Option<Record>>,
    generations: Vec<u32>,
}
impl TransitionProcessor {
    pub fn prepare(plan: &TransitionPlan) -> Self {
        let capacity = plan.image().limits.pending_requests as usize;
        Self {
            records: vec![None; capacity],
            generations: vec![1; capacity],
        }
    }
    /// Collect SP's resolved plans without executing or advancing any loop.
    pub fn harvest(&mut self, state: &mut StateProcessor) {
        for resolution in state.take_resolutions() {
            let record = self.records[resolution.request.slot as usize]
                .as_mut()
                .expect("admitted request");
            assert_eq!(record.id, resolution.request);
            assert!(record.outcome.is_none());
            record.outcome = Some(resolution.outcome);
        }
    }
    fn record(&self, id: RequestId) -> Result<Record, TransitionError> {
        self.records
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|r| r.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    pub fn request_slots<'a>(
        &'a self,
        state: &'a StateProcessor,
    ) -> impl Iterator<Item = Option<RequestSnapshot>> + 'a {
        self.records.iter().map(|record| {
            record.map(|record| RequestSnapshot {
                id: record.id,
                command: record.command,
                effective: record.effective,
                outcome: record.outcome,
                remaining_ticks: if record.outcome.is_none()
                    && matches!(record.command.timing, Timing::AfterTicks { .. })
                {
                    state
                        .loop_state(record.command.target)
                        .ok()
                        .and_then(|slot| {
                            slot.planned
                                .iter()
                                .flatten()
                                .find(|plan| plan.request == record.id)
                                .map(|plan| plan.remaining_ticks)
                        })
                } else {
                    None
                },
            })
        })
    }
    pub fn completion(&self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        let record = self.record(id)?;
        Ok(record.outcome.map(|outcome| Completion {
            id,
            sequence: record.command.sequence,
            requested: record.command.at,
            effective: record.effective,
            outcome,
        }))
    }
    pub fn take_completion(
        &mut self,
        id: RequestId,
    ) -> Result<Option<Completion>, TransitionError> {
        let completion = self.completion(id)?;
        if completion.is_some() {
            self.records[id.slot as usize] = None;
            self.generations[id.slot as usize] = id.generation.checked_add(1).unwrap_or(0);
        }
        Ok(completion)
    }
    pub fn cancel(
        &mut self,
        state: &mut StateProcessor,
        id: RequestId,
    ) -> Result<bool, TransitionError> {
        self.harvest(state);
        let loops = state.loop_memory_mut()?;
        let record = self.record(id)?;
        if record.outcome.is_some() {
            return Ok(false);
        }
        let frame = loops.frame;
        let slot = loops.slot_mut(record.command.target)?;
        let plan = slot
            .planned
            .iter_mut()
            .find(|plan| plan.is_some_and(|plan| plan.request == id))
            .ok_or(TransitionError::InvalidState)?;
        *plan = None;
        self.records[id.slot as usize].as_mut().unwrap().outcome =
            Some(Outcome::Cancelled { frame });
        Ok(true)
    }
    pub fn submit(
        &mut self,
        state: &mut StateProcessor,
        command: Command,
    ) -> Result<RequestId, TransitionError> {
        self.harvest(state);
        let loops = state.loop_memory_mut()?;
        let target = loops.slot(command.target)?;
        let quantum = loops.plan.image().description.quantum;
        if let Operation::Play { position, .. } | Operation::Seek { position, .. } =
            command.operation
        {
            if position % u64::from(quantum) != 0 {
                return Err(TransitionError::InvalidFrame);
            }
        }
        if let Timing::AfterTicks { source, .. } = command.timing {
            loops.slot(source)?;
            if loops.plan.specs[command.target.slot as usize]
                .unwrap()
                .sync_source
                != Some(source)
            {
                return Err(TransitionError::MissingSync);
            }
        }
        if self
            .records
            .iter()
            .flatten()
            .any(|r| r.command.sequence == command.sequence)
        {
            return Err(TransitionError::DuplicateId);
        }
        let plan_index = target
            .planned
            .iter()
            .position(Option::is_none)
            .ok_or(TransitionError::Capacity)?;
        let index = self
            .records
            .iter()
            .enumerate()
            .position(|(i, r)| r.is_none() && self.generations[i] != 0)
            .ok_or(TransitionError::Capacity)?;
        let q = u64::from(quantum);
        let at = command.at.max(loops.frame);
        let remainder = at & (q - 1);
        let effective = at
            .checked_add(if remainder == 0 { 0 } else { q - remainder })
            .ok_or(TransitionError::InvalidFrame)?;
        let id = RequestId {
            slot: index as u32,
            generation: self.generations[index],
        };
        loops.slot_mut(command.target)?.planned[plan_index] = Some(PlannedTransition {
            request: id,
            command,
            effective,
            remaining_ticks: match command.timing {
                Timing::At => 0,
                Timing::AfterTicks { skip, .. } => skip,
            },
        });
        self.records[index] = Some(Record {
            id,
            command,
            effective,
            outcome: None,
        });
        Ok(id)
    }
}
