//! Sync-ordered loop execution and current-quantum render state.
use crate::{
    records::{LoopRecord, Mode as RecordMode},
    transitions::*,
};
use alloc::vec::Vec;
use rt_fpga_compiler::SpanMode;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    #[cfg_attr(feature = "serde", serde(default))]
    composite_previous: Option<u64>,
    #[cfg_attr(feature = "serde", serde(default))]
    composite_snapshot: Option<bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    composite_wrap: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) composite_wait: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) authored_recording: bool,
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
            composite_previous: None,
            composite_snapshot: None,
            composite_wrap: false,
            composite_wait: false,
            authored_recording: false,
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
    capture_checkpoint: Vec<Option<LoopSlot>>,
    pub(crate) capture_refusals: u64,
}
impl LoopMemory {
    pub(crate) fn reserve_loading(&mut self, slots: usize) {
        self.projected_pulses.resize(slots, false);
        self.capture_checkpoint.resize(slots, None);
        self.events
            .reserve((4 * slots).saturating_sub(self.events.len()));
        self.resolutions
            .reserve((2 * slots).saturating_sub(self.resolutions.len()));
    }
    pub fn prepare(plan: TransitionPlan) -> Self {
        let slots = plan
            .initial_states()
            .into_iter()
            .map(|state| state.map(|state| LoopSlot::initial(state, false)))
            .collect();
        let events = Vec::with_capacity(plan.image.limits.events as usize);
        let resolutions = Vec::with_capacity(2 * plan.image.description.loops.len());
        let projected_pulses = alloc::vec![false; plan.image.limits.loop_slots as usize];
        let capture_checkpoint = if plan.authored_recording {
            alloc::vec![None; plan.image.limits.loop_slots as usize]
        } else {
            Vec::new()
        };
        Self {
            projected_pulses,
            plan,
            slots,
            frame: 0,
            events,
            resolutions,
            capture_checkpoint,
            capture_refusals: 0,
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
    pub fn settle(
        &mut self,
        instructions: &[SyncInstruction],
        quantum: u32,
        prepare_captures: impl FnOnce(&Self) -> Result<(), crate::Error>,
    ) -> Result<(), crate::Error> {
        if self.plan.authored_recording {
            let frame = self.frame;
            self.capture_checkpoint.copy_from_slice(&self.slots);
            self.run(instructions, quantum, true)
                .map_err(crate::Error::Transition)?;
            if let Err(error) = prepare_captures(self) {
                self.slots.copy_from_slice(&self.capture_checkpoint);
                self.frame = frame;
                match error {
                    crate::Error::MemoryFull
                    | crate::Error::InvalidView
                    | crate::Error::InvalidState => {
                        self.run(instructions, quantum, false)
                            .map_err(crate::Error::Transition)?;
                        self.capture_refusals = self.capture_refusals.saturating_add(1);
                    }
                    error => return Err(error),
                }
            }
        } else {
            self.run(instructions, quantum, false)
                .map_err(crate::Error::Transition)?;
        }
        Ok(())
    }
    fn run(
        &mut self,
        instructions: &[SyncInstruction],
        quantum: u32,
        allow_authored_recording: bool,
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
                || self.composite_parents(loop_id).iter().any(|parent| {
                    !instructions[..i].iter().any(
                        |&SyncInstruction::ProcessLoop { loop_id: old, .. }| old == parent.parent,
                    )
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
            self.process_loop(loop_id, sync_source, quantum, allow_authored_recording)?;
        }
        self.frame = next;
        Ok(())
    }
    pub fn mode_plan(&self, id: LoopId) -> Result<(ModePlan, u32), TransitionError> {
        let slot = self.slot(id)?;
        Ok((
            ModePlan {
                now: None,
                steps: slot.planned.map(|plan| {
                    plan.filter(|p| p.on_pulse).map(|p| ModeStep {
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
        for i in 0..2 {
            if mask & (1 << i) != 0 && slot.planned[i].is_some_and(|p| p.on_pulse) {
                slot.planned[i] = None;
            }
        }
        Ok(())
    }
    /// Apply an immediate mode and replace the two ordered source-pulse steps.
    pub fn write_mode_plan(&mut self, id: LoopId, plan: ModePlan) -> Result<(), TransitionError> {
        let composite = self.composite_clock(id).is_some();
        let slot = self.slot(id)?;
        let source = self.plan.specs[id.slot as usize].unwrap().sync_source;
        if composite || self.has_composite_parent(id) {
            let planned =
                prepare_composite_mode_plan(slot.state, source, self.frame, plan, composite)?;
            let slot = self.slot_mut(id)?;
            slot.planned = planned;
            slot.mode_plan = false;
            return Ok(());
        }
        if !slot.mode_plan && slot.planned.iter().any(Option::is_some) {
            return Err(TransitionError::InvalidState);
        }
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
        allow_authored_recording: bool,
    ) -> Result<(), TransitionError> {
        let clock = self.composite_clock(id);
        let composite = clock.is_some();
        let slot = self.slot_mut(id)?;
        slot.pulse_this_quantum = false;
        slot.pulse_pending = false;
        slot.pulse_masked = false;
        slot.protected = None;
        slot.composite_previous = slot.render.plays().then_some(slot.render.position);
        slot.composite_snapshot = None;
        slot.composite_wrap = false;
        if !self.slot(id)?.mode_plan {
            self.apply_due(id, true)?;
        }
        self.apply_composite_parents(id, allow_authored_recording)?;
        if !composite {
            self.natural_boundary(id, source)?;
        }
        if let Some(source) = source.filter(|source| *source != id) {
            if self.slot(source)?.pulse_this_quantum {
                if !composite && !self.slot(id)?.pulse_masked {
                    self.queue_pulse(id)?;
                }
                if !self.slot(id)?.mode_plan {
                    self.apply_due(id, false)?;
                }
                if !composite {
                    self.natural_boundary(id, Some(source))?;
                }
            }
        }
        if composite {
            self.natural_boundary(id, source)?;
        }
        if self.slot(id)?.pulse_pending {
            let frame = self.frame;
            let slot = self.slot_mut(id)?;
            slot.pulse_pending = false;
            slot.pulse_this_quantum = true;
            slot.state.last_pulse = Some(frame);
            self.event(id, EventKind::Pulse)?;
            if source == Some(id)
                || (source.is_none() && (composite || self.has_composite_parent(id)))
            {
                if !self.slot(id)?.mode_plan {
                    self.apply_due(id, false)?;
                }
                if !composite {
                    self.natural_boundary(id, source)?;
                }
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
    fn composite_clock(&self, id: LoopId) -> Option<CompositeClock> {
        self.plan
            .composite_clocks
            .get(id.slot as usize)
            .copied()
            .flatten()
    }
    fn composite_parents(&self, id: LoopId) -> &[ParentSpans] {
        self.plan
            .composite_parents
            .get(id.slot as usize)
            .map_or(&[], Vec::as_slice)
    }
    fn has_composite_parent(&self, id: LoopId) -> bool {
        self.composite_parents(id)
            .iter()
            .any(|parent| !parent.spans.is_empty())
    }
    fn apply_composite_parents(
        &mut self,
        id: LoopId,
        allow_authored_recording: bool,
    ) -> Result<(), TransitionError> {
        let slot = self.slot(id)?;
        if slot.protected.is_some() && (allow_authored_recording || !slot.authored_recording) {
            return Ok(());
        }
        let clock = self.composite_clock(id);
        let playable_length = if slot.state.mode == Mode::Recording {
            slot.state.capture_frames
        } else {
            slot.state.content_length
        };
        let mut operation = None;
        for binding in self
            .composite_parents(id)
            .iter()
            .filter(|_| slot.protected.is_none())
        {
            let parent = self.slot(binding.parent)?;
            fn span_at(
                spans: &[rt_fpga_compiler::FrameSpan],
                position: u64,
            ) -> Option<&rt_fpga_compiler::FrameSpan> {
                let index = spans.partition_point(|span| u64::from(span.end) <= position);
                spans
                    .get(index)
                    .filter(|span| u64::from(span.start) <= position)
            }
            let prior = parent
                .composite_previous
                .and_then(|pos| span_at(&binding.spans, pos))
                .map(|span| span.mode);
            let desired = parent
                .render
                .plays()
                .then(|| span_at(&binding.spans, parent.render.position))
                .flatten();
            let wrap = parent.composite_wrap;
            let needs_entry = clock.is_some_and(|c| {
                slot.state.mode != Mode::Playing || (c.explicit && slot.state.at_end())
            });
            if prior == desired.map(|span| span.mode)
                && !(desired.is_some()
                    && (parent.composite_snapshot.is_some() || (wrap && needs_entry)))
            {
                continue;
            }
            operation = Some(match desired {
                Some(span) if span.mode == SpanMode::Record => Operation::Record,
                Some(span) if span.mode == SpanMode::Play && playable_length != 0 => {
                    let mut offset = parent
                        .composite_snapshot
                        .map_or(0, |_| parent.render.position - u64::from(span.start));
                    if let Some(clock) = clock {
                        let unit = u64::from(clock.unit_frames);
                        offset -= offset % unit;
                    }
                    if clock.is_some_and(|c| c.explicit) && offset >= playable_length {
                        Operation::Stop
                    } else if parent.composite_snapshot == Some(false)
                        && slot.state.mode == Mode::Playing
                    {
                        Operation::Seek {
                            position: offset % playable_length,
                            retrigger: false,
                        }
                    } else {
                        Operation::Play {
                            position: offset % playable_length,
                            retrigger: true,
                        }
                    }
                }
                _ => Operation::Stop,
            });
            break;
        }
        let operation = match operation {
            Some(Operation::Record) | None if !allow_authored_recording => {
                slot.authored_recording.then_some(Operation::Stop)
            }
            operation => operation,
        };
        let Some(operation) = operation else {
            return Ok(());
        };
        let before = self.slot(id)?.state;
        let mut state = before;
        let (pulse, snapshot) = apply_loop_operation(&mut state, clock, operation)?;
        let slot = self.slot_mut(id)?;
        slot.state = state;
        slot.composite_snapshot = snapshot;
        slot.authored_recording = operation == Operation::Record;
        if snapshot.is_some() || operation == Operation::Stop {
            slot.composite_wait = false;
        }
        if operation == Operation::Stop {
            slot.pulse_masked = true;
        }
        if before != state {
            self.event(id, EventKind::Transition(operation))?;
        }
        if pulse {
            self.queue_pulse(id)?;
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
        let result = apply_loop_operation(&mut state, self.composite_clock(id), plan.operation);
        match result {
            Err(reason) => self.terminal(id, index, Outcome::Failed { frame, reason })?,
            Ok((pulse, snapshot)) => {
                let slot = self.slot_mut(id)?;
                slot.state = state;
                slot.authored_recording = false;
                if snapshot.is_some() {
                    slot.composite_snapshot = snapshot;
                }
                if snapshot.is_some() || plan.operation == Operation::Stop {
                    slot.composite_wait = false;
                }
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
        let clock = self.composite_clock(id);
        let explicit = clock.is_some_and(|clock| clock.explicit);
        let waiting = if !explicit && state.repeat_sync {
            sync_source
                .filter(|source| *source != id)
                .map(|source| {
                    self.slot(source).map(|source| {
                        source.state.mode == Mode::Playing && !source.pulse_this_quantum
                    })
                })
                .transpose()?
                .unwrap_or(false)
        } else {
            false
        };
        let slot = self.slot_mut(id)?;
        if waiting {
            if clock.is_some() && !slot.composite_wait {
                slot.state.cycles = slot
                    .state
                    .cycles
                    .checked_add(1)
                    .ok_or(TransitionError::Exhausted)?;
                slot.composite_wait = true;
            }
            return Ok(());
        }
        if !slot.composite_wait {
            slot.state.cycles = slot
                .state
                .cycles
                .checked_add(1)
                .ok_or(TransitionError::Exhausted)?;
        }
        slot.state.position = 0;
        if explicit {
            slot.state.mode = Mode::Stopped;
            return Ok(());
        }
        if slot.composite_wait {
            slot.composite_wait = false;
            slot.composite_snapshot = Some(true);
        } else {
            slot.composite_wrap = clock.is_some();
        }
        self.queue_pulse(id)?;
        if clock.is_none() {
            self.event(id, EventKind::Wrap)?;
        }
        Ok(())
    }
}
/// Use the ordinary absolute and pulse slots for loops that a composite can drive.
pub fn prepare_composite_mode_plan(
    state: LoopState,
    source: Option<LoopId>,
    frame: u64,
    mut plan: ModePlan,
    composite: bool,
) -> Result<[Option<PlannedTransition>; 2], TransitionError> {
    let self_sync = source.is_none_or(|source| source == state.id);
    if self_sync && state.mode != Mode::Playing && plan.now.is_none() {
        if plan.steps.iter().all(Option::is_some) {
            return Err(TransitionError::InvalidState);
        }
        plan.now = plan
            .steps
            .into_iter()
            .flatten()
            .next()
            .map(|step| step.mode);
        plan.steps = [None; 2];
    }
    let now = prepare_mode_now(state, frame, plan.now, composite)?;
    let mut result = [None; 2];
    for (index, step) in plan.steps.into_iter().enumerate() {
        if let Some(step) = step {
            if self_sync && state.mode != Mode::Playing && plan.now != Some(Mode::Playing) {
                return Err(TransitionError::InvalidState);
            }
            let operation = match step.mode {
                Mode::Stopped => Operation::Stop,
                Mode::Playing => Operation::Play {
                    position: 0,
                    retrigger: false,
                },
                Mode::Recording if !composite => Operation::Record,
                Mode::Recording => return Err(TransitionError::InvalidState),
            };
            result[index] = Some(PlannedTransition {
                sequence: index as u64 + 1,
                operation,
                on_pulse: true,
                effective: if self_sync && now.is_some() {
                    frame.checked_add(1).ok_or(TransitionError::Exhausted)?
                } else {
                    frame
                },
                remaining_ticks: u64::from(step.skip),
            });
        }
    }
    if let Some(now) = now {
        *result
            .iter_mut()
            .find(|plan| plan.is_none())
            .ok_or(TransitionError::InvalidConfig)? = Some(now);
    }
    Ok(result)
}

fn prepare_mode_now(
    mut state: LoopState,
    frame: u64,
    mode: Option<Mode>,
    composite: bool,
) -> Result<Option<PlannedTransition>, TransitionError> {
    let operation = match mode {
        None => return Ok(None),
        Some(Mode::Stopped) => Operation::Stop,
        Some(Mode::Playing) => Operation::Play {
            position: 0,
            retrigger: true,
        },
        Some(Mode::Recording) if composite => return Err(TransitionError::InvalidState),
        Some(Mode::Recording) if state.mode == Mode::Recording => return Ok(None),
        Some(Mode::Recording) => Operation::Record,
    };
    if !(composite && state.content_length == 0 && mode == Some(Mode::Playing)) {
        apply_operation(&mut state, operation)?;
    }
    Ok(Some(PlannedTransition {
        sequence: 0,
        operation,
        on_pulse: false,
        effective: frame,
        remaining_ticks: 0,
    }))
}

/// Prepare an iteration-based operation against the active composite's geometry.
pub fn prepare_composite_iteration(
    state: LoopState,
    unit_frames: u32,
    frame: u64,
    iteration: u64,
    retrigger: bool,
    seek: bool,
) -> Result<PlannedTransition, TransitionError> {
    if unit_frames == 0 {
        return Err(TransitionError::InvalidId);
    }
    let position = iteration
        .checked_mul(u64::from(unit_frames))
        .ok_or(TransitionError::InvalidFrame)?;
    let empty_start = !seek && iteration == 0 && state.content_length == 0;
    if position >= state.content_length && !empty_start {
        return Err(TransitionError::InvalidFrame);
    }
    if seek && state.mode != Mode::Playing {
        return Err(TransitionError::InvalidState);
    }
    let operation = if seek {
        Operation::Seek {
            position,
            retrigger,
        }
    } else {
        Operation::Play {
            position,
            retrigger,
        }
    };
    Ok(PlannedTransition {
        sequence: 0,
        operation,
        on_pulse: false,
        effective: frame,
        remaining_ticks: 0,
    })
}

fn apply_loop_operation(
    state: &mut LoopState,
    clock: Option<CompositeClock>,
    operation: Operation,
) -> Result<(bool, Option<bool>), TransitionError> {
    let Some(clock) = clock else {
        return apply_operation(state, operation).map(|pulse| (pulse, None));
    };
    let (position, retrigger, seek) = match operation {
        Operation::Stop => return apply_operation(state, operation).map(|pulse| (pulse, None)),
        Operation::Play {
            position,
            retrigger,
        } => (position, retrigger, false),
        Operation::Seek {
            position,
            retrigger,
        } => (position, retrigger, true),
        Operation::Record => return Err(TransitionError::InvalidState),
    };
    if !seek && state.mode == Mode::Playing && !retrigger {
        return Ok((false, None));
    }
    if !seek && position == 0 && state.content_length == 0 {
        return Ok((false, None));
    }
    if position % u64::from(clock.unit_frames) != 0 {
        return Err(TransitionError::InvalidFrame);
    }
    let pulse = apply_operation(state, operation)?;
    Ok((pulse, Some(pulse)))
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
