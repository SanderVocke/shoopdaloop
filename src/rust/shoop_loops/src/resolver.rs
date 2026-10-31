//! Fixed-storage boundary resolution with unpublished transactional previews.

use crate::command::Record;
use crate::config::checked_storage;
use crate::*;

mod program_change;
pub use program_change::ProgramChangeResult;
use program_change::{ProgramChange, Replacement};

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Priority {
    Direct(Sequence),
    Action {
        source: LoopId,
        ordinal: usize,
        inherited: bool,
    },
}

impl Priority {
    fn beats(self, other: Self) -> bool {
        match (self, other) {
            (Self::Direct(a), Self::Direct(b)) => a > b,
            (Self::Direct(_), _) => true,
            (_, Self::Direct(_)) => false,
            (
                Self::Action {
                    source: a,
                    ordinal: x,
                    inherited: inherited_a,
                },
                Self::Action {
                    source: b,
                    ordinal: y,
                    inherited: inherited_b,
                },
            ) => {
                if inherited_a != inherited_b {
                    !inherited_a
                } else {
                    a < b || (a == b && x > y)
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Intent {
    transition: Transition,
    priority: Priority,
    request: Option<usize>,
    origin: Option<RequestId>,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum ProgramAction {
    #[default]
    None,
    Snapshot {
        restart: bool,
    },
    Stop,
}

struct Scratch {
    states: Vec<Option<State>>,
    program_actions: Vec<ProgramAction>,
    records: Vec<Option<Record>>,
    intents: Vec<Option<Intent>>,
    protected: Vec<Option<Priority>>,
    origins: Vec<Option<RequestId>>,
    playback_discontinuities: Vec<bool>,
    pulses: Vec<bool>,
    delivered: Vec<bool>,
    masked: Vec<bool>,
    eligible: Vec<bool>,
    decisions: Vec<Decision>,
    events: Vec<Event>,
    dropped: u64,
}

/// Unpublished state at a composite's action-emission cut, not activation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramBoundary {
    pub frame: Frame,
    pub topology_epoch: Epoch,
    pub state: State,
}

impl ProgramBoundary {
    /// True only on entry to zero, not while an idle source leaves the cursor there.
    pub fn enters_zero(self) -> bool {
        self.state.mode == Mode::Playing
            && self.state.composite.is_some_and(|cursor| {
                cursor.iteration == 0 && cursor.entered_at == Some(self.frame)
            })
    }
}

pub struct Resolver {
    topology: PreparedTopology,
    epoch: Epoch,
    frame: Frame,
    states: Vec<Option<State>>,
    records: Vec<Option<Record>>,
    loop_generations: Vec<u32>,
    request_generations: Vec<u32>,
    scratch: Scratch,
    preview: Option<Interval>,
    preview_program_epoch: Option<Epoch>,
    program_change: Option<ProgramChange>,
    quiescent: bool,
    committed: bool,
    events: Vec<Event>,
    dropped_events: u64,
}

pub(crate) fn validate_storage(limits: Limits) -> Result<(), Error> {
    let mut bytes = 0usize;
    for (count, size) in [
        (
            limits.loop_slots,
            2 * size_of::<Option<State>>()
                + size_of::<Option<Intent>>()
                + size_of::<Option<Priority>>()
                + size_of::<Option<RequestId>>()
                + size_of::<Decision>()
                + size_of::<u32>()
                + size_of::<ProgramAction>()
                + 4,
        ),
        (
            limits.pending_requests,
            2 * size_of::<Option<Record>>() + size_of::<u32>() + 1,
        ),
        (limits.journal_events, 2 * size_of::<Event>()),
    ] {
        bytes = bytes
            .checked_add(checked_storage(count, size)?)
            .ok_or(Error::Capacity)?;
    }
    checked_storage(bytes, 1)?;
    Ok(())
}

impl Resolver {
    /// Preparation may allocate; all execution storage is fixed afterwards.
    pub fn prepare(topology: PreparedTopology) -> Result<Self, Error> {
        let limits = topology.limits().validate()?;
        let mut states = prepared(limits.loop_slots, None);
        let mut loop_generations = prepared(limits.loop_slots, 0);
        let dummy = LoopId::from_parts(0, 1)?;
        let mut decisions = prepared(
            limits.loop_slots,
            Decision {
                state: State::new(dummy, 0),
                playback_discontinuity: false,
                origin: None,
            },
        );
        decisions.clear();
        let mut preview_events = prepared(
            limits.journal_events,
            Event {
                frame: Frame(0),
                target: dummy,
                kind: EventKind::Pulse,
            },
        );
        let mut events = preview_events.clone();
        preview_events.clear();
        events.clear();
        for node in topology.nodes() {
            states[node.id.slot() as usize] = Some(initial_state(*node));
            loop_generations[node.id.slot() as usize] = node.id.generation();
        }
        Ok(Self {
            topology,
            epoch: Epoch(0),
            frame: Frame(0),
            scratch: Scratch {
                states: states.clone(),
                program_actions: prepared(limits.loop_slots, ProgramAction::None),
                records: prepared(limits.pending_requests, None),
                intents: prepared(limits.loop_slots, None),
                protected: prepared(limits.loop_slots, None),
                origins: prepared(limits.loop_slots, None),
                playback_discontinuities: prepared(limits.loop_slots, false),
                pulses: prepared(limits.loop_slots, false),
                delivered: prepared(limits.loop_slots, false),
                masked: prepared(limits.loop_slots, false),
                eligible: prepared(limits.pending_requests, false),
                decisions,
                events: preview_events,
                dropped: 0,
            },
            states,
            records: prepared(limits.pending_requests, None),
            loop_generations,
            request_generations: prepared(limits.pending_requests, 1),
            preview: None,
            preview_program_epoch: None,
            program_change: None,
            quiescent: false,
            committed: false,
            events,
            dropped_events: 0,
        })
    }

    /// Permanently end execution before reclaiming owners after stopped/faulted extraction.
    /// Does not advance time, undo published state, or invent completion of a rendered prefix.
    pub fn quiesce(&mut self) {
        self.quiescent = true;
        self.committed = false;
        self.preview = None;
        self.preview_program_epoch = None;
        for record in self.records.iter_mut().flatten() {
            if record.terminal.is_none() {
                record.terminal = Some(Outcome::Cancelled { frame: self.frame });
            }
        }
    }

    pub fn is_quiescent(&self) -> bool {
        self.quiescent
    }

    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn topology(&self) -> &PreparedTopology {
        &self.topology
    }
    pub fn state(&self, id: LoopId) -> Result<State, Error> {
        self.states
            .get(id.slot() as usize)
            .copied()
            .flatten()
            .filter(|state| state.id == id)
            .ok_or(Error::InvalidId)
    }
    pub(crate) fn reserve_media_revision(
        &mut self,
        id: LoopId,
        revision: Revision,
    ) -> Result<(), Error> {
        if self.quiescent
            || self.frame != Frame(0)
            || self.committed
            || self.preview.is_some()
            || self.records.iter().any(Option::is_some)
        {
            return Err(Error::InvalidState);
        }
        if self.topology.node(id)?.kind != LoopKind::Primitive {
            return Err(Error::InvalidState);
        }
        let state = self.states[id.slot() as usize]
            .as_mut()
            .ok_or(Error::InvalidId)?;
        state.revision_watermark = state.revision_watermark.max(revision);
        Ok(())
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn dropped_events(&self) -> u64 {
        self.dropped_events
    }
    /// Candidate decisions before commit; immutable execution decisions after it.
    pub fn decisions(&self) -> Result<&[Decision], Error> {
        self.preview.ok_or(Error::InvalidState)?;
        Ok(&self.scratch.decisions)
    }
    pub fn retained_requests(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.is_some())
            .count()
    }

    pub fn submit(&mut self, request: Request) -> Result<RequestId, Error> {
        if request.transition == Transition::Program {
            return Err(Error::InvalidProgram);
        }
        self.submit_record(request)
    }

    fn submit_record(&mut self, request: Request) -> Result<RequestId, Error> {
        if self.quiescent || self.committed {
            return Err(Error::InvalidState);
        }
        let target = self.topology.node(request.target)?;
        if let Timing::AfterTicks { source, .. } = request.timing {
            self.topology.node(source)?;
            if target.sync_source != Some(source) {
                return Err(Error::MissingSync);
            }
        }
        if self
            .records
            .iter()
            .flatten()
            .any(|record| record.request.sequence == request.sequence)
        {
            return Err(Error::DuplicateId);
        }
        let slot = self
            .records
            .iter()
            .enumerate()
            .position(|(index, record)| record.is_none() && self.request_generations[index] != 0)
            .ok_or(Error::Capacity)?;
        let id = RequestId::from_parts(slot as u32, self.request_generations[slot])?;
        self.records[slot] = Some(Record {
            id,
            request,
            effective: request.at.max(self.frame),
            remaining: match request.timing {
                Timing::At => 0,
                Timing::AfterTicks { skip, .. } => skip,
            },
            terminal: None,
        });
        self.preview = None;
        Ok(id)
    }

    fn record(&self, id: RequestId) -> Result<Record, Error> {
        self.records
            .get(id.slot() as usize)
            .copied()
            .flatten()
            .filter(|record| record.id == id)
            .ok_or(Error::InvalidId)
    }

    pub fn completion(&self, id: RequestId) -> Result<Option<Completion>, Error> {
        let record = self.record(id)?;
        Ok(record.terminal.map(|outcome| Completion {
            id,
            sequence: record.request.sequence,
            requested: record.request.at,
            effective: record.effective,
            outcome,
        }))
    }

    pub fn take_completion(&mut self, id: RequestId) -> Result<Option<Completion>, Error> {
        if self.committed
            || self
                .program_change
                .as_ref()
                .is_some_and(|change| change.id == id)
        {
            return Err(Error::InvalidState);
        }
        let completion = self.completion(id)?;
        if completion.is_some() {
            let slot = id.slot() as usize;
            self.records[slot] = None;
            self.request_generations[slot] = id.generation().checked_add(1).unwrap_or(0);
            self.preview = None;
        }
        Ok(completion)
    }

    pub fn cancel(&mut self, id: RequestId) -> Result<CancelResult, Error> {
        if self.committed {
            return Err(Error::InvalidState);
        }
        let record = self.record(id)?;
        if record.terminal.is_some() {
            return Ok(CancelResult::AlreadyTerminal);
        }
        self.records[id.slot() as usize]
            .as_mut()
            .expect("validated receipt")
            .terminal = Some(Outcome::Cancelled { frame: self.frame });
        self.preview = None;
        Ok(CancelResult::Cancelled)
    }

    /// Resolve the target's controls and clock without emitting its child actions.
    ///
    /// Invalidates any earlier uncommitted preview. Committed state, receipts, journals and
    /// epochs remain unchanged; this does not create a committable interval or
    /// certify a replacement candidate. Reprobe after changing retained inputs.
    pub fn probe_program_boundary(
        &mut self,
        target: LoopId,
        observations: &[Observation],
    ) -> Result<ProgramBoundary, Error> {
        self.begin_preview(Frame(self.frame.0.saturating_add(1)), observations)?;
        if self.topology.node(target)?.kind != LoopKind::Composite {
            return Err(Error::InvalidProgram);
        }
        Boundary {
            topology: &self.topology,
            replacement: None,
            replaced: None,
            scratch: &mut self.scratch,
            frame: self.frame,
            observations,
        }
        .settle(Some(target))?;
        Ok(ProgramBoundary {
            frame: self.frame,
            topology_epoch: self.epoch,
            state: self.scratch.states[target.slot() as usize].expect("prepared composite"),
        })
    }

    fn begin_preview(&mut self, ceiling: Frame, observations: &[Observation]) -> Result<(), Error> {
        if self.quiescent || self.committed {
            return Err(Error::InvalidState);
        }
        self.preview = None;
        self.preview_program_epoch = None;
        if self.frame.0 == u64::MAX {
            return Err(Error::Exhausted);
        }
        if ceiling <= self.frame {
            return Err(Error::InvalidFrame);
        }
        let limits = self.topology.limits();
        let observation_capacity = limits
            .loop_slots
            .checked_mul(2)
            .and_then(|count| count.checked_add(limits.pending_requests))
            .ok_or(Error::Capacity)?;
        if observations.len() > observation_capacity {
            return Err(Error::Capacity);
        }
        for (index, observation) in observations.iter().enumerate() {
            if observations[..index]
                .iter()
                .any(|previous| match (*previous, *observation) {
                    (
                        Observation::CaptureExhausted { target: a },
                        Observation::CaptureExhausted { target: b },
                    ) => a == b,
                    (
                        Observation::CaptureUnavailable { target: a, .. },
                        Observation::CaptureUnavailable { target: b, .. },
                    ) => a == b,
                    (
                        Observation::Refused { request: a, .. },
                        Observation::Refused { request: b, .. },
                    ) => a == b,
                    _ => false,
                })
            {
                return Err(Error::DuplicateId);
            }
            match *observation {
                Observation::CaptureExhausted { target } => {
                    if self.state(target)?.mode != Mode::Recording {
                        return Err(Error::InvalidState);
                    }
                }
                Observation::CaptureUnavailable { target, .. } => {
                    if self.state(target)?.composite.is_some() {
                        return Err(Error::InvalidState);
                    }
                }
                Observation::Refused { request, .. } => {
                    if self.record(request)?.terminal.is_some() {
                        return Err(Error::InvalidState);
                    }
                }
            }
        }
        let scratch = &mut self.scratch;
        scratch.states.copy_from_slice(&self.states);
        scratch.records.copy_from_slice(&self.records);
        scratch.intents.fill(None);
        scratch.program_actions.fill(ProgramAction::None);
        scratch.protected.fill(None);
        scratch.origins.fill(None);
        scratch.playback_discontinuities.fill(false);
        scratch.pulses.fill(false);
        scratch.delivered.fill(false);
        scratch.masked.fill(false);
        scratch.eligible.fill(false);
        scratch.events.clear();
        scratch.decisions.clear();
        scratch.dropped = 0;
        Ok(())
    }

    /// All preflight retries start from committed state and the same retained inputs.
    pub fn preview(
        &mut self,
        ceiling: Frame,
        observations: &[Observation],
    ) -> Result<Interval, Error> {
        self.begin_preview(ceiling, observations)?;
        let replacement = self.select_program(ceiling, observations)?;
        let replaced = replacement.map(|change| {
            self.topology
                .program(change.target)
                .expect("validated old program")
        });
        let topology = if replacement.is_some() {
            &self
                .program_change
                .as_ref()
                .expect("selected owner")
                .candidate
        } else {
            &self.topology
        };
        let mut boundary = Boundary {
            topology,
            replacement,
            replaced,
            scratch: &mut self.scratch,
            frame: self.frame,
            observations,
        };
        boundary.settle(None)?;
        let mut end = ceiling.0;
        for record in boundary.scratch.records.iter().flatten() {
            if record.terminal.is_none() && record.effective > self.frame {
                end = end.min(record.effective.0);
            }
        }
        for node in self.topology.nodes() {
            let slot = node.id.slot() as usize;
            let state = boundary.scratch.states[slot].expect("prepared state");
            if state.plays() {
                let remaining = state
                    .content_length
                    .checked_sub(state.position)
                    .ok_or(Error::InvalidState)?;
                if remaining == 0 {
                    return Err(Error::InvalidState);
                }
                end = end.min(self.frame.0.saturating_add(remaining));
            }
            if state.mode == Mode::Recording {
                let remaining = u64::MAX - state.capture_frames;
                if remaining == 0 {
                    return Err(Error::Exhausted);
                }
                end = end.min(self.frame.0.saturating_add(remaining));
            }
            boundary.scratch.decisions.push(Decision {
                state,
                playback_discontinuity: state.plays()
                    && boundary.scratch.playback_discontinuities[slot],
                origin: boundary.scratch.origins[slot],
            });
        }
        let interval = Interval {
            start: self.frame,
            end: Frame(end),
        };
        self.preview = Some(interval);
        Ok(interval)
    }

    pub fn commit(&mut self) -> Result<Interval, Error> {
        if self.committed {
            return Err(Error::InvalidState);
        }
        let interval = self.preview.ok_or(Error::InvalidState)?;
        if let Some(epoch) = self.preview_program_epoch {
            let change = self
                .program_change
                .as_mut()
                .expect("selected owner retained");
            std::mem::swap(&mut self.topology, &mut change.candidate);
            change.activated_epoch = Some(epoch);
            self.epoch = epoch;
        }
        std::mem::swap(&mut self.states, &mut self.scratch.states);
        std::mem::swap(&mut self.records, &mut self.scratch.records);
        std::mem::swap(&mut self.events, &mut self.scratch.events);
        self.dropped_events = self.dropped_events.saturating_add(self.scratch.dropped);
        self.committed = true;
        Ok(interval)
    }

    pub fn finish(&mut self, end: Frame) -> Result<(), Error> {
        if !self.committed || self.preview.map(|interval| interval.end) != Some(end) {
            return Err(Error::InvalidState);
        }
        let frames = end.0 - self.frame.0;
        for state in self.states.iter_mut().flatten() {
            if state.plays() {
                state.position += frames;
            }
            if state.mode == Mode::Recording {
                state.capture_frames += frames;
            }
        }
        self.frame = end;
        self.preview = None;
        self.committed = false;
        Ok(())
    }

    /// Displaced owners are returned, never destroyed on this path.
    pub fn replace_topology(
        &mut self,
        expected: Epoch,
        candidate: PreparedTopology,
    ) -> Result<PreparedTopology, (Error, PreparedTopology)> {
        let validation = (|| {
            if self.quiescent || self.committed || expected != self.epoch {
                return Err(Error::InvalidState);
            }
            if candidate.limits() != self.topology.limits() {
                return Err(Error::InvalidProgram);
            }
            for state in self.states.iter().flatten() {
                if candidate.node(state.id).is_err() && state.mode != Mode::Stopped {
                    return Err(Error::InvalidState);
                }
            }
            for node in candidate.nodes() {
                if let Ok(state) = self.state(node.id) {
                    if state.composite.is_some() != (node.kind == LoopKind::Composite) {
                        return Err(Error::InvalidProgram);
                    }
                    if state.composite.is_some()
                        && state.mode != Mode::Stopped
                        && candidate.program(node.id) != self.topology.program(node.id)
                    {
                        return Err(Error::InvalidState);
                    }
                }
                let slot = node.id.slot() as usize;
                if self.state(node.id).is_err()
                    && node.id.generation() <= self.loop_generations[slot]
                {
                    return Err(Error::InvalidId);
                }
            }
            self.epoch.checked_next().map_err(|_| Error::Exhausted)
        })();
        let next = match validation {
            Ok(next) => next,
            Err(error) => return Err((error, candidate)),
        };
        self.scratch.states.copy_from_slice(&self.states);
        self.states.fill(None);
        for node in candidate.nodes() {
            let slot = node.id.slot() as usize;
            self.states[slot] = Some(
                self.scratch.states[slot]
                    .filter(|state| state.id == node.id)
                    .map(|mut state| {
                        if state.composite.is_some()
                            && candidate.program(node.id) != self.topology.program(node.id)
                        {
                            state.composite = Some(CompositeState::default());
                            state.content_length = node.length;
                        }
                        state
                    })
                    .unwrap_or_else(|| initial_state(*node)),
            );
            self.loop_generations[slot] = node.id.generation();
        }
        for record in self.records.iter_mut().flatten() {
            if record.terminal.is_some() {
                continue;
            }
            let reason = match candidate.node(record.request.target) {
                Err(error) => Some(error),
                Ok(target) => match record.request.timing {
                    Timing::AfterTicks { source, .. }
                        if target.sync_source != Some(source)
                            || candidate.node(source).is_err() =>
                    {
                        Some(Error::MissingSync)
                    }
                    _ => None,
                },
            };
            if let Some(reason) = reason {
                record.terminal = Some(Outcome::Failed {
                    frame: self.frame,
                    reason,
                });
            }
        }
        if let Some(change) = &self.program_change {
            if change.activated_epoch.is_none() {
                let record = self.records[change.id.slot() as usize]
                    .as_mut()
                    .expect("retained program request");
                if record.terminal.is_none() {
                    record.terminal = Some(Outcome::Failed {
                        frame: self.frame,
                        reason: Error::InvalidState,
                    });
                }
            }
        }
        self.preview = None;
        self.epoch = next;
        Ok(std::mem::replace(&mut self.topology, candidate))
    }
}

struct Boundary<'a> {
    topology: &'a PreparedTopology,
    replacement: Option<Replacement>,
    replaced: Option<crate::program::CompiledProgram<'a>>,
    scratch: &'a mut Scratch,
    frame: Frame,
    observations: &'a [Observation],
}

impl Boundary<'_> {
    fn event(&mut self, target: LoopId, kind: EventKind) {
        if self.scratch.events.len() < self.topology.limits().journal_events {
            self.scratch.events.push(Event {
                frame: self.frame,
                target,
                kind,
            });
        } else {
            self.scratch.dropped = self.scratch.dropped.saturating_add(1);
        }
    }
    fn terminal(&mut self, request: Option<usize>, outcome: Outcome) {
        if let Some(slot) = request {
            self.scratch.records[slot]
                .as_mut()
                .expect("retained request")
                .terminal = Some(outcome);
        }
    }
    fn offer(&mut self, target: usize, intent: Intent) {
        if let Some(previous) = self.scratch.intents[target] {
            if !intent.priority.beats(previous.priority) {
                self.terminal(intent.request, Outcome::Superseded { frame: self.frame });
                return;
            }
            self.terminal(previous.request, Outcome::Superseded { frame: self.frame });
        }
        self.scratch.intents[target] = Some(intent);
    }
    fn direct(&self, index: usize) -> Intent {
        let record = self.scratch.records[index].expect("retained request");
        Intent {
            transition: record.request.transition,
            priority: Priority::Direct(record.request.sequence),
            request: Some(index),
            origin: Some(record.id),
        }
    }
    fn apply(&mut self, target: usize, intent: Intent) {
        if self.scratch.protected[target].is_some_and(|previous| !intent.priority.beats(previous)) {
            self.terminal(intent.request, Outcome::Superseded { frame: self.frame });
            return;
        }
        self.scratch.protected[target] = Some(intent.priority);
        if let Some(reason) = self
            .observations
            .iter()
            .find_map(|observation| match *observation {
                Observation::Refused { request, reason } if intent.origin == Some(request) => {
                    Some(reason)
                }
                _ => None,
            })
            .or_else(|| {
                self.observations
                    .iter()
                    .find_map(|observation| match *observation {
                        Observation::CaptureUnavailable {
                            target: unavailable,
                            reason,
                        } if unavailable.slot() as usize == target
                            && matches!(intent.transition, Transition::Record { .. }) =>
                        {
                            Some(reason)
                        }
                        _ => None,
                    })
            })
        {
            self.terminal(
                intent.request,
                Outcome::Failed {
                    frame: self.frame,
                    reason,
                },
            );
            if intent.request.is_none() {
                let id = self.scratch.states[target].expect("prepared state").id;
                self.event(id, EventKind::Refused(reason));
            }
            return;
        }
        let mut state = self.scratch.states[target].expect("prepared state");
        let before = state;
        let result = if let Some(program) = self.topology.program(state.id) {
            composite_transition(
                &mut state,
                intent.transition,
                program.iterations,
                self.frame,
            )
        } else {
            transition(&mut state, intent.transition).map(|pulse| (pulse, ProgramAction::None))
        };
        match result {
            Err(reason) => {
                self.terminal(
                    intent.request,
                    Outcome::Failed {
                        frame: self.frame,
                        reason,
                    },
                );
                if intent.request.is_none() {
                    self.event(state.id, EventKind::Refused(reason));
                }
            }
            Ok((pulse, action)) => {
                self.scratch.playback_discontinuities[target] |=
                    playback_discontinuity(before, state, intent.transition);
                if action != ProgramAction::None {
                    self.scratch.program_actions[target] = action;
                }
                self.scratch.states[target] = Some(state);
                self.scratch.origins[target] = intent.origin;
                self.terminal(intent.request, Outcome::Applied { frame: self.frame });
                if state != before {
                    self.event(state.id, EventKind::Transition(intent.transition));
                }
                if intent.transition == Transition::Stop {
                    self.scratch.masked[target] = true;
                    if let Priority::Direct(sequence) = intent.priority {
                        for record in self.scratch.records.iter_mut().flatten() {
                            if record.terminal.is_none()
                                && record.request.target == state.id
                                && record.request.sequence < sequence
                            {
                                record.terminal = Some(Outcome::Cancelled { frame: self.frame });
                            }
                        }
                    }
                } else if pulse {
                    self.scratch.masked[target] = false;
                    self.scratch.pulses[target] = true;
                }
            }
        }
    }

    fn settle(&mut self, probe: Option<LoopId>) -> Result<(), Error> {
        for observation in self.observations {
            if let Observation::CaptureExhausted { target } = *observation {
                let slot = target.slot() as usize;
                let state = self.scratch.states[slot]
                    .as_mut()
                    .expect("validated observation");
                state.capture_incomplete = true;
                state.mode = if state.monitor_previous && state.content_length != 0 {
                    Mode::Playing
                } else {
                    Mode::Stopped
                };
                state.monitor_previous = false;
                if state.mode == Mode::Stopped {
                    state.position = 0;
                }
                self.event(target, EventKind::CaptureExhausted);
            }
        }
        for slot in 0..self.scratch.records.len() {
            let Some(record) = self.scratch.records[slot] else {
                continue;
            };
            if record.terminal.is_none()
                && record.effective <= self.frame
                && record.request.timing == Timing::At
                && record.request.transition != Transition::Program
            {
                self.offer(record.request.target.slot() as usize, self.direct(slot));
            }
        }
        if let Some(change) = self.replacement {
            self.prepare_replacement(change)?;
        }
        for component in self.topology.components() {
            for id in component.iter().copied() {
                let slot = id.slot() as usize;
                if let Some(intent) = self.scratch.intents[slot].take() {
                    self.apply(slot, intent);
                }
                let state = self.scratch.states[slot].as_mut().expect("prepared state");
                if state.plays() && state.position == state.content_length {
                    state.cycles = state.cycles.checked_add(1).ok_or(Error::Exhausted)?;
                    state.position = 0;
                    self.scratch.playback_discontinuities[slot] = true;
                    self.scratch.pulses[slot] = true;
                    self.event(id, EventKind::Wrap);
                }
                if self
                    .topology
                    .node(id)?
                    .sync_source
                    .is_some_and(|source| self.scratch.delivered[source.slot() as usize])
                    && !self.scratch.masked[slot]
                {
                    self.scratch.pulses[slot] = true;
                }
            }
            // Upstream source edges already occurred; internal source edges are
            // considered as each member is delivered below.
            for id in component.iter().copied() {
                if let Some(source) = self.topology.node(id)?.sync_source {
                    if self.scratch.delivered[source.slot() as usize] {
                        self.react(id, source);
                    }
                }
            }
            while let Some(id) = component.iter().copied().find(|id| {
                let slot = id.slot() as usize;
                self.scratch.pulses[slot] && !self.scratch.delivered[slot]
            }) {
                let slot = id.slot() as usize;
                self.scratch.delivered[slot] = true;
                self.scratch.states[slot]
                    .as_mut()
                    .expect("prepared state")
                    .last_pulse = Some(self.frame);
                self.event(id, EventKind::Pulse);
                for follower in component.iter().copied() {
                    if self.topology.node(follower)?.sync_source == Some(id) {
                        let target = follower.slot() as usize;
                        if !self.scratch.masked[target] {
                            self.scratch.pulses[target] = true;
                        }
                        self.react(follower, id);
                    }
                }
            }
            for id in component.iter().copied() {
                if probe == Some(id) {
                    self.advance_program(id)?;
                    return Ok(());
                }
                self.emit_program(id)?;
                if !self.scratch.delivered[id.slot() as usize] {
                    continue;
                }
                for (ordinal, trigger) in self.topology.triggers().iter().enumerate() {
                    if trigger.source == id {
                        let intent = Intent {
                            transition: mode_transition(trigger.mode),
                            priority: Priority::Action {
                                source: id,
                                ordinal: 2 * self.topology.limits().program_entries + ordinal,
                                inherited: false,
                            },
                            request: None,
                            origin: None,
                        };
                        self.offer(trigger.target.slot() as usize, intent);
                    }
                }
            }
        }
        Ok(())
    }

    fn advance_program(
        &mut self,
        id: LoopId,
    ) -> Result<Option<(u64, ProgramAction, Option<RequestId>)>, Error> {
        let Some(program) = self.topology.program(id) else {
            return Ok(None);
        };
        let slot = id.slot() as usize;
        let mut state = self.scratch.states[slot].expect("prepared state");
        let mut cursor = state.composite.expect("compiled composite");
        let previous = cursor.iteration;
        let mut action = self.scratch.program_actions[slot];
        let origin = if action == ProgramAction::None {
            None
        } else {
            self.scratch.origins[slot]
        };
        if action == ProgramAction::None {
            if state.mode != Mode::Playing || !self.scratch.delivered[slot] {
                return Ok(None);
            }
            cursor.iteration = cursor.iteration.checked_add(1).ok_or(Error::Exhausted)?;
            cursor.entered_at = Some(self.frame);
            let timing = self
                .replaced
                .filter(|old| old.source == id)
                .unwrap_or(program);
            if cursor.iteration == timing.iterations {
                cursor.passes = cursor.passes.checked_add(1).ok_or(Error::Exhausted)?;
                cursor.iteration = 0;
                if timing.kind == ProgramKind::Explicit {
                    state.mode = Mode::Stopped;
                    action = ProgramAction::Stop;
                    self.event(id, EventKind::ProgramEnd);
                }
            }
        }
        if self.replacement.is_some_and(|change| change.target == id)
            && program.iterations == 0
            && state.mode == Mode::Playing
        {
            state.mode = Mode::Stopped;
            cursor.iteration = 0;
            action = ProgramAction::Stop;
        }
        if action != ProgramAction::Stop {
            self.event(
                id,
                EventKind::Iteration {
                    iteration: cursor.iteration,
                    passes: cursor.passes,
                },
            );
        }
        state.composite = Some(cursor);
        self.scratch.states[slot] = Some(state);
        Ok(Some((previous, action, origin)))
    }

    fn emit_program(&mut self, id: LoopId) -> Result<(), Error> {
        let Some((previous, action, origin)) = self.advance_program(id)? else {
            return Ok(());
        };
        let cursor = self.scratch.states[id.slot() as usize]
            .expect("prepared composite")
            .composite
            .expect("compiled composite");
        // The program is immutable, and every target belongs to a downstream
        // component. No consumer can observe these intents before arbitration.
        for index in 0..self
            .topology
            .program(id)
            .expect("compiled program")
            .targets()
            .len()
        {
            let (target, old, selected, unit, inherited) = {
                let program = self.topology.program(id).expect("compiled program");
                let timeline = program.targets().get(index).expect("compiled target");
                (
                    timeline.target,
                    self.replaced.filter(|old| old.source == id).map_or_else(
                        || timeline.at(previous),
                        |old| {
                            old.target(timeline.target)
                                .and_then(|target| target.at(previous))
                        },
                    ),
                    timeline.at(cursor.iteration),
                    program.nominal_unit_frames,
                    program.kind == ProgramKind::Regular,
                )
            };
            let desired = if action == ProgramAction::Stop {
                None
            } else {
                selected
            };
            if action == ProgramAction::None
                && old.map(|segment| segment.mode) == desired.map(|segment| segment.mode)
            {
                continue;
            }
            let child = self.scratch.states[target.slot() as usize].expect("prepared target");
            let ordinal = desired
                .map_or(self.topology.limits().program_entries + index, |segment| {
                    segment.ordinal as usize
                });
            let operation = match desired {
                None => Transition::Stop,
                Some(segment) if segment.mode == Mode::Stopped => Transition::Stop,
                Some(segment) if segment.mode == Mode::Recording => Transition::Record {
                    monitor_previous: false,
                },
                Some(segment) => {
                    let offset = if matches!(action, ProgramAction::Snapshot { .. }) {
                        (cursor.iteration - segment.occurrence_start)
                            .checked_mul(unit)
                            .ok_or(Error::Exhausted)?
                    } else {
                        0
                    };
                    let restart = !matches!(action, ProgramAction::Snapshot { restart: false });
                    if let Some(nested) = self.topology.program(target) {
                        let iteration = offset / nested.nominal_unit_frames;
                        if nested.iterations == 0
                            || (nested.kind == ProgramKind::Explicit
                                && iteration >= nested.iterations)
                        {
                            Transition::Stop
                        } else {
                            let iteration = iteration % nested.iterations;
                            if !restart && child.mode == Mode::Playing {
                                Transition::SeekIteration {
                                    iteration,
                                    retrigger: false,
                                }
                            } else {
                                Transition::PlayIteration {
                                    iteration,
                                    retrigger: true,
                                }
                            }
                        }
                    } else {
                        let length = if child.mode == Mode::Recording {
                            child.capture_frames
                        } else {
                            child.content_length
                        };
                        if length == 0 {
                            Transition::Stop
                        } else if !restart && child.mode == Mode::Playing {
                            Transition::Seek {
                                position: offset % length,
                                retrigger: false,
                            }
                        } else {
                            Transition::Play {
                                position: offset % length,
                                retrigger: true,
                            }
                        }
                    }
                }
            };
            self.offer(
                target.slot() as usize,
                Intent {
                    transition: operation,
                    priority: Priority::Action {
                        source: id,
                        ordinal,
                        inherited,
                    },
                    request: None,
                    origin: self
                        .replacement
                        .filter(|change| change.target == id)
                        .map(|change| change.id)
                        .or(origin),
                },
            );
        }
        Ok(())
    }

    fn react(&mut self, target: LoopId, source: LoopId) {
        let target_slot = target.slot() as usize;
        for index in 0..self.scratch.records.len() {
            let Some(record) = self.scratch.records[index].as_mut() else {
                continue;
            };
            if record.terminal.is_some()
                || record.effective > self.frame
                || record.request.target != target
                || self.scratch.eligible[index]
            {
                continue;
            }
            if let Timing::AfterTicks {
                source: expected, ..
            } = record.request.timing
            {
                if expected == source {
                    self.scratch.eligible[index] = true;
                    if record.remaining != 0 {
                        record.remaining -= 1;
                    } else {
                        self.offer(target_slot, self.direct(index));
                    }
                }
            }
        }
        if let Some(intent) = self.scratch.intents[target_slot].take() {
            self.apply(target_slot, intent);
        }
    }
}

fn initial_state(node: LoopSpec) -> State {
    let mut state = State::new(node.id, node.length);
    if node.kind == LoopKind::Composite {
        state.composite = Some(CompositeState::default());
    }
    state
}

fn composite_transition(
    state: &mut State,
    operation: Transition,
    iterations: u64,
    frame: Frame,
) -> Result<(bool, ProgramAction), Error> {
    let mut cursor = state.composite.ok_or(Error::InvalidState)?;
    let (iteration, retrigger, seeking) = match operation {
        Transition::Stop => {
            state.mode = Mode::Stopped;
            cursor.iteration = 0;
            cursor.entered_at = Some(frame);
            state.composite = Some(cursor);
            return Ok((false, ProgramAction::Stop));
        }
        Transition::Play {
            position: 0,
            retrigger,
        } => (0, retrigger, false),
        Transition::PlayIteration {
            iteration,
            retrigger,
        } => (iteration, retrigger, false),
        Transition::SeekIteration {
            iteration,
            retrigger,
        } => (iteration, retrigger, true),
        _ => return Err(Error::InvalidState),
    };
    if seeking && state.mode != Mode::Playing {
        return Err(Error::InvalidState);
    }
    if !seeking && state.mode == Mode::Playing && !retrigger {
        return Ok((false, ProgramAction::None));
    }
    if iterations == 0 && iteration == 0 && !seeking {
        state.mode = Mode::Stopped;
        return Ok((false, ProgramAction::Stop));
    }
    if iteration >= iterations {
        return Err(Error::InvalidFrame);
    }
    state.mode = Mode::Playing;
    cursor.iteration = iteration;
    if !seeking || retrigger {
        cursor.passes = 0;
    }
    cursor.entered_at = Some(frame);
    state.composite = Some(cursor);
    Ok((
        !seeking || retrigger,
        ProgramAction::Snapshot {
            restart: !seeking || retrigger,
        },
    ))
}

fn prepared<T: Copy>(count: usize, value: T) -> Vec<T> {
    let mut storage = vec![value; count];
    for slot in &mut storage {
        *slot = std::hint::black_box(value);
    }
    storage
}

fn mode_transition(mode: Mode) -> Transition {
    match mode {
        Mode::Stopped => Transition::Stop,
        Mode::Playing => Transition::Play {
            position: 0,
            retrigger: false,
        },
        Mode::Recording => Transition::Record {
            monitor_previous: false,
        },
    }
}

fn playback_discontinuity(before: State, after: State, operation: Transition) -> bool {
    if !after.plays() {
        return false;
    }
    if !before.plays() {
        return true;
    }
    match operation {
        Transition::Seek { .. } | Transition::Replace { .. } => true,
        Transition::Play { retrigger, .. } => retrigger || before.mode != Mode::Playing,
        _ => false,
    }
}

fn transition(state: &mut State, operation: Transition) -> Result<bool, Error> {
    match operation {
        Transition::Program
        | Transition::PlayIteration { .. }
        | Transition::SeekIteration { .. } => Err(Error::InvalidState),
        Transition::Stop => {
            if state.mode == Mode::Recording {
                state.content_length = state.capture_frames;
                state.content_revision = state.capture_revision;
            }
            state.mode = Mode::Stopped;
            state.position = 0;
            state.monitor_previous = false;
            Ok(false)
        }
        Transition::Record { monitor_previous } => {
            state.capture_revision = state
                .revision_watermark
                .checked_next()
                .map_err(|_| Error::Exhausted)?;
            state.revision_watermark = state.capture_revision;
            state.mode = Mode::Recording;
            state.capture_frames = 0;
            state.capture_incomplete = false;
            state.monitor_previous = monitor_previous;
            Ok(true)
        }
        Transition::Play {
            position,
            retrigger,
        } => {
            if state.mode == Mode::Playing && !retrigger {
                return Ok(false);
            }
            if state.mode == Mode::Recording {
                state.content_length = state.capture_frames;
                state.content_revision = state.capture_revision;
            }
            if state.content_length == 0 {
                return Err(Error::EmptyContent);
            }
            if position >= state.content_length {
                return Err(Error::InvalidFrame);
            }
            state.mode = Mode::Playing;
            state.position = position;
            state.cycles = 0;
            state.monitor_previous = false;
            Ok(true)
        }
        Transition::Seek {
            position,
            retrigger,
        } => {
            if state.mode != Mode::Playing {
                return Err(Error::InvalidState);
            }
            if position >= state.content_length {
                return Err(Error::InvalidFrame);
            }
            state.position = position;
            if retrigger {
                state.cycles = 0;
            }
            Ok(retrigger)
        }
        Transition::Replace {
            length,
            revision,
            preserve_position,
        } => {
            if state.mode == Mode::Recording {
                return Err(Error::InvalidState);
            }
            if preserve_position && state.position >= length && state.position != 0 {
                return Err(Error::InvalidFrame);
            }
            state.content_length = length;
            state.content_revision = revision;
            state.revision_watermark = state.revision_watermark.max(revision);
            if !preserve_position {
                state.position = 0;
            }
            if length == 0 {
                state.mode = Mode::Stopped;
            }
            Ok(false)
        }
    }
}
