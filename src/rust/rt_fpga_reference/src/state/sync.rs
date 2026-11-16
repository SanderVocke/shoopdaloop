//! Sync-ordered loop execution and current-quantum render state.
use crate::{
    records::{LoopRecord, Mode as RecordMode},
    transitions::*,
};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SyncInstruction {
    ProcessLoop { loop_id: LoopId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedTransition {
    pub request: RequestId,
    pub command: Command,
    pub effective: u64,
    /// Zero fires on the next eligible source pulse, matching AfterTicks::skip.
    pub remaining_ticks: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopSlot {
    /// Logical state at the next quantum boundary after the sync instruction runs.
    pub state: LoopState,
    /// Settled state for the audio pass, before cursor/capture advancement.
    pub render: LoopRecord,
    pub planned: [Option<PlannedTransition>; 2],
    pub pulse_this_quantum: bool,
    pulse_pending: bool,
    pulse_masked: bool,
    protected: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) struct Resolution {
    pub request: RequestId,
    pub outcome: Outcome,
}

pub(crate) struct LoopMemory {
    pub plan: TransitionPlan,
    pub slots: Vec<Option<LoopSlot>>,
    pub frame: u64,
    pub events: Vec<Event>,
    pub resolutions: Vec<Resolution>,
}
impl LoopMemory {
    pub fn prepare(plan: TransitionPlan) -> Self {
        let slots = plan
            .initial_states()
            .into_iter()
            .map(|state| {
                state.map(|state| LoopSlot {
                    render: render(state),
                    state,
                    planned: [None; 2],
                    pulse_this_quantum: false,
                    pulse_pending: false,
                    pulse_masked: false,
                    protected: None,
                })
            })
            .collect();
        let events = Vec::with_capacity(plan.image.limits.events as usize);
        let resolutions = Vec::with_capacity(plan.image.limits.pending_requests as usize);
        Self {
            plan,
            slots,
            frame: 0,
            events,
            resolutions,
        }
    }
    pub fn slot(&self, id: LoopId) -> Result<&LoopSlot, TransitionError> {
        self.slots
            .get(id.slot as usize)
            .and_then(Option::as_ref)
            .filter(|slot| slot.state.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    pub fn slot_mut(&mut self, id: LoopId) -> Result<&mut LoopSlot, TransitionError> {
        self.slots
            .get_mut(id.slot as usize)
            .and_then(Option::as_mut)
            .filter(|slot| slot.state.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    pub fn run(
        &mut self,
        instructions: &[SyncInstruction],
        quantum: u32,
    ) -> Result<(), TransitionError> {
        if quantum != self.plan.image.description.quantum
            || instructions.len() != self.plan.image.description.loops.len()
            || instructions
                .iter()
                .zip(self.plan.image.components.iter().flatten())
                .any(|(instruction, id)| {
                    *instruction != SyncInstruction::ProcessLoop { loop_id: *id }
                })
        {
            return Err(TransitionError::InvalidConfig);
        }
        let next = self
            .frame
            .checked_add(u64::from(quantum))
            .ok_or(TransitionError::Exhausted)?;
        self.events.clear();
        for &SyncInstruction::ProcessLoop { loop_id } in instructions {
            self.process_loop(loop_id, quantum)?;
        }
        self.frame = next;
        Ok(())
    }
    fn process_loop(&mut self, id: LoopId, quantum: u32) -> Result<(), TransitionError> {
        let slot = self.slot_mut(id)?;
        slot.pulse_this_quantum = false;
        slot.pulse_pending = false;
        slot.pulse_masked = false;
        slot.protected = None;
        let source = self.plan.specs[id.slot as usize]
            .ok_or(TransitionError::InvalidId)?
            .sync_source;
        self.apply_due(id, true)?;
        self.natural_boundary(id)?;
        if let Some(source) = source.filter(|source| *source != id) {
            if self.slot(source)?.pulse_this_quantum {
                if !self.slot(id)?.pulse_masked {
                    self.queue_pulse(id)?;
                }
                self.apply_due(id, false)?;
                self.natural_boundary(id)?;
            }
        }
        if self.slot(id)?.pulse_pending {
            let frame = self.frame;
            let slot = self.slot_mut(id)?;
            slot.pulse_pending = false;
            slot.pulse_this_quantum = true;
            slot.state.last_pulse = Some(frame);
            self.event(id, EventKind::Pulse)?;
            if source == Some(id) {
                self.apply_due(id, false)?;
                self.natural_boundary(id)?;
            }
        }
        let slot = self.slot_mut(id)?;
        slot.render = render(slot.state);
        if slot.state.plays() {
            slot.state.position = slot
                .state
                .position
                .checked_add(u64::from(quantum))
                .filter(|position| *position <= slot.state.content_length)
                .ok_or(TransitionError::InvalidFrame)?;
        }
        if slot.state.mode == Mode::Recording {
            slot.state.capture_frames = slot
                .state
                .capture_frames
                .checked_add(u64::from(quantum))
                .ok_or(TransitionError::Exhausted)?;
        }
        Ok(())
    }
    fn event(&mut self, target: LoopId, kind: EventKind) -> Result<(), TransitionError> {
        if self.events.len() == self.plan.image.limits.events as usize {
            return Err(TransitionError::Capacity);
        }
        self.events.push(Event {
            frame: self.frame,
            target,
            kind,
        });
        Ok(())
    }
    fn queue_pulse(&mut self, id: LoopId) -> Result<(), TransitionError> {
        let slot = self.slot_mut(id)?;
        if !slot.pulse_this_quantum {
            slot.pulse_pending = true;
        }
        Ok(())
    }
    fn terminal(
        &mut self,
        id: LoopId,
        index: usize,
        outcome: Outcome,
    ) -> Result<(), TransitionError> {
        if self.resolutions.len() == self.plan.image.limits.pending_requests as usize {
            return Err(TransitionError::Capacity);
        }
        let plan = self.slot_mut(id)?.planned[index]
            .take()
            .ok_or(TransitionError::InvalidState)?;
        self.resolutions.push(Resolution {
            request: plan.request,
            outcome,
        });
        Ok(())
    }
    fn apply_due(&mut self, id: LoopId, absolute: bool) -> Result<(), TransitionError> {
        let frame = self.frame;
        let mut winner: Option<usize> = None;
        for i in 0..2 {
            let Some(plan) = self.slot(id)?.planned[i] else {
                continue;
            };
            if plan.effective > frame || (plan.command.timing == Timing::At) != absolute {
                continue;
            }
            if !absolute && plan.remaining_ticks != 0 {
                self.slot_mut(id)?.planned[i]
                    .as_mut()
                    .unwrap()
                    .remaining_ticks -= 1;
                continue;
            }
            if let Some(previous) = winner {
                let old = self.slot(id)?.planned[previous].unwrap();
                let loser = if plan.command.sequence > old.command.sequence {
                    previous
                } else {
                    i
                };
                self.terminal(id, loser, Outcome::Superseded { frame })?;
                if loser == i {
                    continue;
                }
            }
            winner = Some(i);
        }
        let Some(index) = winner else {
            return Ok(());
        };
        let plan = self.slot(id)?.planned[index].unwrap();
        if self
            .slot(id)?
            .protected
            .is_some_and(|seq| plan.command.sequence <= seq)
        {
            return self.terminal(id, index, Outcome::Superseded { frame });
        }
        self.slot_mut(id)?.protected = Some(plan.command.sequence);
        let before = self.slot(id)?.state;
        let mut state = before;
        match apply_operation(&mut state, plan.command.operation) {
            Err(reason) => self.terminal(id, index, Outcome::Failed { frame, reason })?,
            Ok(pulse) => {
                self.slot_mut(id)?.state = state;
                self.terminal(id, index, Outcome::Applied { frame })?;
                if state != before {
                    self.event(id, EventKind::Transition(plan.command.operation))?;
                }
                if plan.command.operation == Operation::Stop {
                    let slot = self.slot_mut(id)?;
                    slot.pulse_masked = true;
                    slot.pulse_pending = false;
                    for i in 0..2 {
                        if self.slot(id)?.planned[i]
                            .is_some_and(|old| old.command.sequence < plan.command.sequence)
                        {
                            self.terminal(id, i, Outcome::Cancelled { frame })?;
                        }
                    }
                } else if pulse {
                    self.slot_mut(id)?.pulse_masked = false;
                    self.queue_pulse(id)?;
                }
            }
        }
        Ok(())
    }
    fn natural_boundary(&mut self, id: LoopId) -> Result<(), TransitionError> {
        let state = self.slot(id)?.state;
        if !state.at_end() {
            return Ok(());
        }
        if state.repeat_sync {
            if let Some(source) = self.plan.specs[id.slot as usize]
                .unwrap()
                .sync_source
                .filter(|source| *source != id)
            {
                let source = self.slot(source)?;
                if source.state.mode == Mode::Playing && !source.pulse_this_quantum {
                    return Ok(());
                }
            }
        }
        let state = &mut self.slot_mut(id)?.state;
        state.cycles = state
            .cycles
            .checked_add(1)
            .ok_or(TransitionError::Exhausted)?;
        state.position = 0;
        self.queue_pulse(id)?;
        self.event(id, EventKind::Wrap)
    }
}
fn render(state: LoopState) -> LoopRecord {
    LoopRecord {
        id: state.id,
        mode: match state.mode {
            Mode::Stopped => RecordMode::Stopped,
            Mode::Playing => RecordMode::Playing,
            Mode::Recording => RecordMode::Recording,
        },
        position: state.position,
        content_length: state.content_length,
        begin_capture: state.mode == Mode::Recording && state.capture_frames == 0,
    }
}
fn apply_operation(state: &mut LoopState, operation: Operation) -> Result<bool, TransitionError> {
    match operation {
        Operation::Stop => {
            if state.mode == Mode::Recording {
                state.content_length = state.capture_frames;
                state.content_revision = state.capture_revision;
            }
            state.mode = Mode::Stopped;
            state.position = 0;
            Ok(false)
        }
        Operation::Record => {
            state.capture_revision = state
                .revision_watermark
                .checked_add(1)
                .ok_or(TransitionError::Exhausted)?;
            state.revision_watermark = state.capture_revision;
            state.mode = Mode::Recording;
            state.capture_frames = 0;
            Ok(true)
        }
        Operation::Play {
            position,
            retrigger,
        } => {
            let preserve = state.mode == Mode::Playing && !retrigger;
            if state.mode == Mode::Recording {
                state.content_length = state.capture_frames;
                state.content_revision = state.capture_revision;
            }
            if state.content_length == 0 {
                return Err(TransitionError::EmptyContent);
            }
            if !preserve && position >= state.content_length {
                return Err(TransitionError::InvalidFrame);
            }
            state.mode = Mode::Playing;
            if !preserve {
                state.position = position;
                state.cycles = 0;
            }
            Ok(!preserve)
        }
        Operation::Seek {
            position,
            retrigger,
        } => {
            if state.mode != Mode::Playing {
                return Err(TransitionError::InvalidState);
            }
            if position >= state.content_length {
                return Err(TransitionError::InvalidFrame);
            }
            state.position = position;
            if retrigger {
                state.cycles = 0;
            }
            Ok(retrigger)
        }
    }
}
