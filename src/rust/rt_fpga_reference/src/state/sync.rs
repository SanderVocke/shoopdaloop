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
    ProcessLoop {
        loop_id: LoopId,
        sync_source: Option<LoopId>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlannedTransition {
    pub sequence: u64,
    pub operation: Operation,
    pub on_pulse: bool,
    pub effective: u64,
    /// Zero fires on the next eligible source pulse.
    pub remaining_ticks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModeStep {
    pub mode: Mode,
    pub skip: u32,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModePlan {
    pub now: Option<Mode>,
    pub steps: [Option<ModeStep>; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    pub(crate) mode_plan: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) midi_capture_open: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) midi_playing: bool,
    pub(crate) mode_failures: u32,
}
impl LoopSlot {
    pub(crate) fn initial(state: LoopState, mode_plan: bool) -> Self {
        Self {
            render: render(state),
            state,
            planned: [None; 2],
            pulse_this_quantum: false,
            pulse_pending: false,
            pulse_masked: false,
            protected: None,
            mode_plan,
            midi_capture_open: false,
            midi_playing: false,
            mode_failures: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Resolution {
    pub target: LoopId,
    pub plan_index: usize,
    pub outcome: Outcome,
}

pub(crate) struct LoopMemory {
    pub plan: TransitionPlan,
    pub slots: Vec<Option<LoopSlot>>,
    pub frame: u64,
    pub events: Vec<Event>,
    pub resolutions: Vec<Resolution>,
    projected_pulses: Vec<bool>,
}
impl LoopMemory {
    pub(crate) fn reserve_loading(&mut self, slots: usize) {
        self.projected_pulses.resize(slots, false);
        self.events
            .reserve((4 * slots).saturating_sub(self.events.len()));
        self.resolutions
            .reserve((2 * slots).saturating_sub(self.resolutions.len()));
    }
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
                    mode_plan: false,
                    midi_capture_open: false,
                    midi_playing: false,
                    mode_failures: 0,
                })
            })
            .collect();
        let events = Vec::with_capacity(plan.image.limits.events as usize);
        let resolutions = Vec::with_capacity(2 * plan.image.description.loops.len());
        let projected_pulses = alloc::vec![false; plan.image.limits.loop_slots as usize];
        Self {
            projected_pulses,
            plan,
            slots,
            frame: 0,
            events,
            resolutions,
        }
    }
    pub fn slot(&self, id: LoopId) -> Result<&LoopSlot, TransitionError> {
        if self
            .plan
            .specs
            .get(id.slot as usize)
            .and_then(Option::as_ref)
            .is_none_or(|s| s.id != id)
        {
            return Err(TransitionError::InvalidId);
        }
        self.slots
            .get(id.slot as usize)
            .and_then(Option::as_ref)
            .filter(|slot| slot.state.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    pub fn slot_mut(&mut self, id: LoopId) -> Result<&mut LoopSlot, TransitionError> {
        if self
            .plan
            .specs
            .get(id.slot as usize)
            .and_then(Option::as_ref)
            .is_none_or(|s| s.id != id)
        {
            return Err(TransitionError::InvalidId);
        }
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
        {
            return Err(TransitionError::InvalidConfig);
        }
        for (
            i,
            &SyncInstruction::ProcessLoop {
                loop_id,
                sync_source,
            },
        ) in instructions.iter().enumerate()
        {
            self.slot(loop_id)?;
            if instructions[..i]
                .iter()
                .any(|&SyncInstruction::ProcessLoop { loop_id: old, .. }| old == loop_id)
                || sync_source.filter(|&s| s != loop_id).is_some_and(|s| {
                    !instructions[..i]
                        .iter()
                        .any(|&SyncInstruction::ProcessLoop { loop_id: old, .. }| old == s)
                })
            {
                return Err(TransitionError::InvalidConfig);
            }
        }
        let next = self
            .frame
            .checked_add(u64::from(quantum))
            .ok_or(TransitionError::Exhausted)?;
        self.events.clear();
        self.resolutions.clear();
        self.advance_mode_plans(instructions)?;
        for &SyncInstruction::ProcessLoop {
            loop_id,
            sync_source,
        } in instructions
        {
            self.process_loop(loop_id, sync_source, quantum)?;
        }
        self.frame = next;
        Ok(())
    }
    pub fn mode_plan(&self, id: LoopId) -> Result<(ModePlan, u32), TransitionError> {
        let slot = self.slot(id)?;
        if !slot.mode_plan {
            return Err(TransitionError::InvalidState);
        }
        Ok((
            ModePlan {
                now: None,
                steps: slot.planned.map(|plan| {
                    plan.map(|p| ModeStep {
                        mode: match p.operation {
                            Operation::Stop => Mode::Stopped,
                            Operation::Record => Mode::Recording,
                            _ => Mode::Playing,
                        },
                        skip: p.remaining_ticks as u32,
                    })
                }),
            },
            slot.mode_failures,
        ))
    }
    pub fn clear_mode_plan(&mut self, id: LoopId, mask: u8) -> Result<(), TransitionError> {
        if !(1..=3).contains(&mask) {
            return Err(TransitionError::InvalidConfig);
        }
        let slot = self.slot_mut(id)?;
        if !slot.mode_plan {
            return Err(TransitionError::InvalidState);
        }
        for i in 0..2 {
            if mask & (1 << i) != 0 {
                slot.planned[i] = None;
            }
        }
        Ok(())
    }
    /// Apply an immediate mode and replace the two ordered source-pulse steps.
    pub fn write_mode_plan(&mut self, id: LoopId, plan: ModePlan) -> Result<(), TransitionError> {
        let slot = self.slot(id)?;
        if !slot.mode_plan && slot.planned.iter().any(Option::is_some) {
            return Err(TransitionError::InvalidState);
        }
        let source = self.plan.specs[id.slot as usize].unwrap().sync_source;
        let frame = self.frame;
        let slot = self.slot_mut(id)?;
        slot.planned = apply_mode_command(&mut slot.state, source, frame, plan)?;
        slot.mode_plan = true;
        Ok(())
    }
    fn apply_mode(
        &mut self,
        id: LoopId,
        mode: Mode,
        scheduled: bool,
    ) -> Result<(), TransitionError> {
        let mut state = self.slot(id)?.state;
        if (mode == Mode::Recording && state.mode == Mode::Recording)
            || (mode == Mode::Stopped && state.mode == Mode::Stopped)
            || (scheduled && mode == Mode::Playing && state.mode == Mode::Playing)
        {
            return Ok(());
        }
        apply_operation(
            &mut state,
            match mode {
                Mode::Stopped => Operation::Stop,
                Mode::Recording => Operation::Record,
                Mode::Playing => Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            },
        )?;
        self.slot_mut(id)?.state = state;
        Ok(())
    }
    fn advance_mode_plans(
        &mut self,
        instructions: &[SyncInstruction],
    ) -> Result<(), TransitionError> {
        // Every step observes the same projected pulse cut, before mode changes.
        for &SyncInstruction::ProcessLoop {
            loop_id: id,
            sync_source,
        } in instructions
        {
            let state = self.slot(id)?.state;
            let source = sync_source.filter(|s| *s != id);
            let source_pulse = source.is_some_and(|s| self.projected_pulses[s.slot as usize]);
            let wraps = state.at_end()
                && (!state.repeat_sync
                    || source.is_none_or(|s| self.slot(s).unwrap().state.mode != Mode::Playing));
            self.projected_pulses[id.slot as usize] = wraps || source_pulse;
        }
        for &SyncInstruction::ProcessLoop {
            loop_id: id,
            sync_source,
        } in instructions
        {
            let Some(slot) = self.slots[id.slot as usize]
                .as_ref()
                .filter(|s| s.mode_plan)
            else {
                continue;
            };
            let source = sync_source.unwrap_or(slot.state.id);
            if !self.projected_pulses[source.slot as usize] {
                continue;
            }
            for index in 0..2 {
                let Some(mut plan) = self.slot(id)?.planned[index] else {
                    continue;
                };
                if plan.remaining_ticks != 0 {
                    plan.remaining_ticks -= 1;
                    self.slot_mut(id)?.planned[index] = Some(plan);
                    continue;
                }
                self.slot_mut(id)?.planned[index] = None;
                let mode = match plan.operation {
                    Operation::Stop => Mode::Stopped,
                    Operation::Record => Mode::Recording,
                    _ => Mode::Playing,
                };
                if self.apply_mode(id, mode, true).is_err() {
                    let slot = self.slot_mut(id)?;
                    slot.planned = [None; 2];
                    slot.mode_failures = slot.mode_failures.saturating_add(1);
                    break;
                }
            }
        }
        Ok(())
    }
    fn process_loop(
        &mut self,
        id: LoopId,
        source: Option<LoopId>,
        quantum: u32,
    ) -> Result<(), TransitionError> {
        let slot = self.slot_mut(id)?;
        slot.pulse_this_quantum = false;
        slot.pulse_pending = false;
        slot.pulse_masked = false;
        slot.protected = None;
        if !self.slot(id)?.mode_plan {
            self.apply_due(id, true)?;
        }
        self.natural_boundary(id, source)?;
        if let Some(source) = source.filter(|source| *source != id) {
            if self.slot(source)?.pulse_this_quantum {
                if !self.slot(id)?.pulse_masked {
                    self.queue_pulse(id)?;
                }
                if !self.slot(id)?.mode_plan {
                    self.apply_due(id, false)?;
                }
                self.natural_boundary(id, Some(source))?;
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
                if !self.slot(id)?.mode_plan {
                    self.apply_due(id, false)?;
                }
                self.natural_boundary(id, source)?;
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
        if self.resolutions.len() == self.resolutions.capacity() {
            return Err(TransitionError::Capacity);
        }
        self.slot_mut(id)?.planned[index]
            .take()
            .ok_or(TransitionError::InvalidState)?;
        self.resolutions.push(Resolution {
            target: id,
            plan_index: index,
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
            if plan.effective > frame || plan.on_pulse == absolute {
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
                let loser = if plan.sequence > old.sequence {
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
            .is_some_and(|seq| plan.sequence <= seq)
        {
            return self.terminal(id, index, Outcome::Superseded { frame });
        }
        self.slot_mut(id)?.protected = Some(plan.sequence);
        let before = self.slot(id)?.state;
        let mut state = before;
        match apply_operation(&mut state, plan.operation) {
            Err(reason) => self.terminal(id, index, Outcome::Failed { frame, reason })?,
            Ok(pulse) => {
                self.slot_mut(id)?.state = state;
                self.terminal(id, index, Outcome::Applied { frame })?;
                if state != before {
                    self.event(id, EventKind::Transition(plan.operation))?;
                }
                if plan.operation == Operation::Stop {
                    let slot = self.slot_mut(id)?;
                    slot.pulse_masked = true;
                    slot.pulse_pending = false;
                    for i in 0..2 {
                        if self.slot(id)?.planned[i].is_some_and(|old| old.sequence < plan.sequence)
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
    fn natural_boundary(
        &mut self,
        id: LoopId,
        sync_source: Option<LoopId>,
    ) -> Result<(), TransitionError> {
        let state = self.slot(id)?.state;
        if !state.at_end() {
            return Ok(());
        }
        if state.repeat_sync {
            if let Some(source) = sync_source.filter(|source| *source != id) {
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
/// Apply a command to actual boundary state; sync execution remains in the SP.
pub fn apply_mode_command(
    state: &mut LoopState,
    source: Option<LoopId>,
    frame: u64,
    mut plan: ModePlan,
) -> Result<[Option<PlannedTransition>; 2], TransitionError> {
    let source = source.unwrap_or(state.id);
    if source == state.id && state.mode != Mode::Playing && plan.now.is_none() {
        if plan.steps.iter().flatten().count() == 2 {
            return Err(TransitionError::InvalidState);
        }
        if let Some(step) = plan.steps.into_iter().flatten().next() {
            plan.now = Some(step.mode);
            plan.steps = [None; 2];
        }
    }
    if plan.steps.iter().any(Option::is_some)
        && source == state.id
        && state.mode != Mode::Playing
        && plan.now != Some(Mode::Playing)
    {
        return Err(TransitionError::InvalidState);
    }
    let mut next = *state;
    if let Some(mode) = plan.now {
        if !((mode == Mode::Recording && next.mode == Mode::Recording)
            || (mode == Mode::Stopped && next.mode == Mode::Stopped))
        {
            apply_operation(
                &mut next,
                match mode {
                    Mode::Stopped => Operation::Stop,
                    Mode::Recording => Operation::Record,
                    Mode::Playing => Operation::Play {
                        position: 0,
                        retrigger: true,
                    },
                },
            )?;
        }
    }
    *state = next;
    Ok(plan.steps.map(|s| {
        s.map(|s| PlannedTransition {
            sequence: 0,
            effective: frame,
            on_pulse: true,
            remaining_ticks: u64::from(s.skip),
            operation: match s.mode {
                Mode::Stopped => Operation::Stop,
                Mode::Recording => Operation::Record,
                Mode::Playing => Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            },
        })
    }))
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
