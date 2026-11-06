//! Fixed-storage boundary resolution with unpublished transactional previews.

use crate::command::Record;
use crate::config::checked_storage;
use crate::*;
use alloc::{vec, vec::Vec};

mod program_change;
mod retime;
pub use program_change::ProgramChangeResult;
use program_change::{ProgramChange, Replacement};
pub use retime::ContentRetime;

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
    Wrap,
    Snapshot {
        restart: bool,
    },
    Stop,
    End,
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
    batch_failures: Vec<Option<Error>>,
    clock_edges: Vec<bool>,
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
    /// True only on entry to zero, excluding an end-of-pass alignment wait.
    pub fn enters_zero(self) -> bool {
        self.state.mode == Mode::Playing
            && self.state.composite.is_some_and(|cursor| {
                !cursor.waiting_repeat
                    && cursor.iteration == 0
                    && cursor.entered_at == Some(self.frame)
            })
    }
}

pub struct Resolver {
    quantum: rt_runtime::Quantum,
    topology: PreparedTopology,
    epoch: Epoch,
    frame: Frame,
    states: Vec<Option<State>>,
    retimed: Vec<Option<Transition>>,
    retimed_pending: bool,
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
                + size_of::<Option<Transition>>()
                + 4,
        ),
        (
            limits.pending_requests,
            2 * size_of::<Option<Record>>() + size_of::<u32>() + size_of::<Option<Error>>() + 2,
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
    pub fn prepare(
        topology: PreparedTopology,
        quantum: rt_runtime::Quantum,
    ) -> Result<Self, Error> {
        topology.validate_quantum(quantum)?;
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
            quantum,
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
                batch_failures: prepared(limits.pending_requests, None),
                clock_edges: prepared(limits.pending_requests, false),
                decisions,
                events: preview_events,
                dropped: 0,
            },
            states,
            retimed: prepared(limits.loop_slots, None),
            retimed_pending: false,
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

    pub fn quantum(&self) -> rt_runtime::Quantum {
        self.quantum
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
    pub fn pending_transitions(&self) -> impl Iterator<Item = PendingTransition> + '_ {
        self.records.iter().flatten().filter_map(|record| {
            (record.terminal.is_none() && !matches!(record.request.transition, Transition::Program))
                .then_some(PendingTransition {
                    request: record.request,
                    effective: record.effective,
                    remaining_ticks: match record.request.timing {
                        Timing::At => None,
                        Timing::AfterTicks { .. } => Some(record.remaining),
                    },
                })
        })
    }
    pub fn state(&self, id: LoopId) -> Result<State, Error> {
        self.states
            .get(id.slot() as usize)
            .copied()
            .flatten()
            .filter(|state| state.id == id)
            .ok_or(Error::InvalidId)
    }
    /// Whether a new authored occurrence must enter a program at this boundary.
    /// A playing regular pass or unfinished one-pass program keeps its own phase.
    pub fn program_needs_entry_at(&self, id: LoopId, frame: Frame) -> Result<bool, Error> {
        let program = self.topology.program(id).ok_or(Error::InvalidId)?;
        Ok(program_needs_entry_at(program, self.state(id)?, frame))
    }
    pub fn set_repeat_sync(&mut self, id: LoopId, enabled: bool) -> Result<(), Error> {
        if self.quiescent || self.committed {
            return Err(Error::InvalidState);
        }
        self.state(id)?;
        self.states[id.slot() as usize]
            .as_mut()
            .expect("validated state")
            .repeat_sync = enabled;
        self.preview = None;
        Ok(())
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
    /// Head delayed transition in the prerouting window, after supersession and
    /// cancellation have settled. The reference routing window has skip count one.
    pub fn prerouting_transition(&self, target: LoopId) -> Result<Option<Transition>, Error> {
        self.topology.node(target)?;
        let records = if self.preview.is_some() && !self.committed {
            &self.scratch.records
        } else {
            &self.records
        };
        let head = records
            .iter()
            .flatten()
            .filter(|record| {
                record.terminal.is_none()
                    && record.request.target == target
                    && record.effective <= self.frame
                    && matches!(record.request.timing, Timing::AfterTicks { .. })
            })
            .min_by_key(|record| {
                (
                    record.remaining,
                    record.effective,
                    core::cmp::Reverse(record.request.sequence),
                )
            });
        Ok(head
            .filter(|record| record.remaining == 1)
            .map(|record| record.request.transition))
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

    /// Admit distinct targets at one cut, with all-or-nothing settlement.
    pub fn submit_batch(
        &mut self,
        requests: &[Request],
        output: &mut [Option<RequestId>],
    ) -> Result<(), Error> {
        if requests.is_empty() || output.len() != requests.len() {
            return Err(Error::InvalidConfig);
        }
        if requests.iter().enumerate().any(|(i, request)| {
            request.at != requests[0].at
                || request.timing != requests[0].timing
                || request.sequence != requests[0].sequence
                || request.transition == Transition::Program
                || requests[..i].iter().any(|old| old.target == request.target)
        }) {
            return Err(Error::InvalidConfig);
        }
        if requests.len()
            > self
                .records
                .iter()
                .enumerate()
                .filter(|(i, record)| record.is_none() && self.request_generations[*i] != 0)
                .count()
        {
            return Err(Error::Capacity);
        }
        let slot = self
            .records
            .iter()
            .enumerate()
            .position(|(i, record)| record.is_none() && self.request_generations[i] != 0)
            .ok_or(Error::Capacity)?;
        let batch = RequestId::from_parts(slot as u32, self.request_generations[slot])?;
        output.fill(None);
        for (i, request) in requests.iter().enumerate() {
            match self.submit_record_group(*request, Some(batch), false) {
                Ok(id) => output[i] = Some(id),
                Err(error) => {
                    for id in output.iter_mut().filter_map(Option::take) {
                        self.records[id.slot() as usize] = None;
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    /// Reserve a later action owned by the same accepted musical intent.
    pub fn submit_followup(
        &mut self,
        parent: RequestId,
        request: Request,
    ) -> Result<RequestId, Error> {
        let owner = self.record(parent)?;
        if owner.terminal.is_some()
            || owner.followup
            || owner.request.target != request.target
            || owner.request.sequence != request.sequence
            || request.at
                < owner
                    .effective
                    .checked_add(self.quantum.frames())
                    .map_err(|_| Error::InvalidFrame)?
        {
            return Err(Error::InvalidState);
        }
        self.submit_record_group(request, None, true)
    }
    /// Remaining clock edges in the candidate boundary, including cancellations.
    pub fn pending_delay(&self, target: LoopId) -> Option<u64> {
        let records = if self.preview.is_some() {
            &self.scratch.records
        } else {
            &self.records
        };
        records
            .iter()
            .flatten()
            .filter(|record| {
                record.terminal.is_none()
                    && record.request.target == target
                    && matches!(record.request.timing, Timing::AfterTicks { .. })
            })
            .map(|record| record.remaining)
            .min()
    }

    fn submit_record(&mut self, request: Request) -> Result<RequestId, Error> {
        self.submit_record_group(request, None, false)
    }
    fn submit_record_group(
        &mut self,
        request: Request,
        batch: Option<RequestId>,
        followup: bool,
    ) -> Result<RequestId, Error> {
        if self.quiescent || self.committed {
            return Err(Error::InvalidState);
        }
        {
            let q = self.quantum;
            let aligned = match request.transition {
                Transition::Play { position, .. }
                | Transition::PlayDry { position, .. }
                | Transition::Seek { position, .. }
                | Transition::ReplaceInput {
                    position: Some(position),
                } => position % q.frames() as u64 == 0,
                Transition::Adopt {
                    length, position, ..
                } => length % q.frames() as u64 == 0 && position % q.frames() as u64 == 0,
                Transition::Replace { length, .. } => length % q.frames() as u64 == 0,
                Transition::RecordPrefix { frames } => frames % q.frames() as u64 == 0,
                _ => true,
            };
            if !aligned {
                return Err(Error::InvalidFrame);
            }
        }
        let target = self.topology.node(request.target)?;
        if let Timing::AfterTicks { source, .. } = request.timing {
            self.topology.node(source)?;
            if target.sync_source != Some(source) && batch.is_none() && !followup {
                return Err(Error::MissingSync);
            }
        }
        if !followup
            && self.records.iter().flatten().any(|record| {
                record.request.sequence == request.sequence
                    && (batch.is_none() || record.batch != batch)
            })
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
            effective: self
                .quantum
                .effective(request.at, self.frame)
                .map_err(|_| Error::InvalidFrame)?,
            remaining: match request.timing {
                Timing::At => 0,
                Timing::AfterTicks { skip, .. } => skip,
            },
            terminal: None,
            batch,
            followup,
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
        for target in self.records.iter_mut().flatten() {
            if target.id == id || record.batch.is_some() && target.batch == record.batch {
                target.terminal = Some(Outcome::Cancelled { frame: self.frame });
            }
        }
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
        if self.topology.node(target)?.kind != LoopKind::Composite {
            return Err(Error::InvalidProgram);
        }
        self.settle_preview(
            Frame(self.frame.0.saturating_add(self.quantum.frames() as u64)),
            observations,
            Some(target),
        )?;
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
                        Observation::CaptureAborted { target: a, .. },
                        Observation::CaptureAborted { target: b, .. },
                    ) => a == b,
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
                Observation::CaptureAborted { target, .. }
                | Observation::CaptureExhausted { target } => {
                    if !self.state(target)?.writes() {
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
        for record in scratch.records.iter_mut().flatten() {
            if let Some(reason) = scratch.batch_failures[record.id.slot() as usize] {
                if record.terminal.is_none() {
                    record.terminal = Some(Outcome::Failed {
                        frame: self.frame,
                        reason,
                    });
                }
            }
        }
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
        if self.retimed_pending {
            for (slot, transition) in self.retimed.iter().enumerate() {
                let Some(transition) = transition else {
                    continue;
                };
                let Some(state) = self.states[slot] else {
                    continue;
                };
                scratch.playback_discontinuities[slot] = true;
                if scratch.events.len() < self.topology.limits().journal_events {
                    scratch.events.push(Event {
                        frame: self.frame,
                        target: state.id,
                        kind: EventKind::Transition(*transition),
                    });
                } else {
                    scratch.dropped = scratch.dropped.saturating_add(1);
                }
            }
        }
        Ok(())
    }

    fn refuse_batch(&mut self, batch: RequestId, reason: Error) -> bool {
        let mut changed = false;
        for record in self
            .records
            .iter()
            .flatten()
            .filter(|record| record.batch == Some(batch))
        {
            let failure = &mut self.scratch.batch_failures[record.id.slot() as usize];
            if failure.is_none() {
                *failure = Some(reason);
                changed = true;
            }
        }
        changed
    }
    fn settle_preview(
        &mut self,
        end: Frame,
        observations: &[Observation],
        probe: Option<LoopId>,
    ) -> Result<Option<Replacement>, Error> {
        self.scratch.batch_failures.fill(None);
        self.scratch.clock_edges.fill(false);
        let mut attempts = 0;
        for slot in 0..self.records.len() {
            let Some(record) = self.records[slot] else {
                continue;
            };
            if let (Some(batch), Some(outcome)) = (record.batch, record.terminal) {
                let reason = match outcome {
                    Outcome::Applied { .. } => continue,
                    Outcome::Failed { reason, .. } => reason,
                    _ => Error::InvalidState,
                };
                self.refuse_batch(batch, reason);
            }
        }
        for observation in observations {
            if let Observation::Refused { request, reason } = *observation {
                if let Some(batch) = self.record(request)?.batch {
                    self.refuse_batch(batch, reason);
                }
            }
        }
        loop {
            attempts += 1;
            if attempts > 2 * self.records.len() + 2 {
                return Err(Error::InvalidState);
            }
            self.begin_preview(end, observations)?;
            let replacement = if probe.is_none() {
                self.select_program(end, observations)?
            } else {
                None
            };
            let replaced =
                replacement.map(|c| self.topology.program(c.target).expect("validated program"));
            let topology = if replacement.is_some() {
                &self
                    .program_change
                    .as_ref()
                    .expect("selected owner")
                    .candidate
            } else {
                &self.topology
            };
            Boundary {
                topology,
                replacement,
                replaced,
                scratch: &mut self.scratch,
                frame: self.frame,
                observations,
            }
            .settle(probe)?;
            let mut retry = false;
            for slot in 0..self.records.len() {
                let Some(before) = self.records[slot].filter(|record| record.terminal.is_none())
                else {
                    continue;
                };
                let Some(batch) = before.batch else { continue };
                let Some(outcome) = self.scratch.records[slot].and_then(|record| record.terminal)
                else {
                    continue;
                };
                let reason = match outcome {
                    Outcome::Applied { .. } => continue,
                    Outcome::Failed { reason, .. } => reason,
                    _ => Error::InvalidState,
                };
                retry |= self.refuse_batch(batch, reason);
            }
            for slot in 0..self.records.len() {
                let edge = self.records[slot]
                    .filter(|record| {
                        record.terminal.is_none()
                            && (record.followup || record.batch.is_some())
                            && record.effective <= self.frame
                    })
                    .and_then(|record| match record.request.timing {
                        Timing::AfterTicks { source, .. } => {
                            Some(self.scratch.delivered[source.slot() as usize])
                        }
                        _ => None,
                    })
                    .unwrap_or(false);
                retry |= self.scratch.clock_edges[slot] != edge;
                self.scratch.clock_edges[slot] = edge;
            }
            if !retry {
                return Ok(replacement);
            }
        }
    }
    /// All preflight retries start from committed state and the same retained inputs.
    /// Whole-tick transactional settlement. No future boundary search is performed.
    pub fn preview_tick(&mut self, observations: &[Observation]) -> Result<Interval, Error> {
        let quantum = self.quantum;
        let end = self
            .frame
            .checked_add(quantum.frames())
            .map_err(|_| Error::Exhausted)?;
        let replacement = self.settle_preview(end, observations, None)?;
        let topology = if replacement.is_some() {
            &self
                .program_change
                .as_ref()
                .expect("selected owner")
                .candidate
        } else {
            &self.topology
        };
        let q = quantum.frames() as u64;
        for node in topology.nodes() {
            let slot = node.id.slot() as usize;
            let state = self.scratch.states[slot].ok_or(Error::InvalidState)?;
            if state.content_length % q != 0
                || state.position % q != 0
                || state.capture_frames % q != 0
                || (state.advances()
                    && state
                        .position
                        .checked_add(q)
                        .is_none_or(|p| p > state.content_length))
            {
                return Err(Error::InvalidFrame);
            }
            state
                .capture_frames
                .checked_add(if state.mode.records() { q } else { 0 })
                .ok_or(Error::Exhausted)?;
            self.scratch.decisions.push(Decision {
                state,
                playback_discontinuity: state.plays()
                    && self.scratch.playback_discontinuities[slot],
                origin: self.scratch.origins[slot],
            });
        }
        let interval = Interval {
            start: self.frame,
            end,
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
            core::mem::swap(&mut self.topology, &mut change.candidate);
            change.activated_epoch = Some(epoch);
            self.epoch = epoch;
        }
        core::mem::swap(&mut self.states, &mut self.scratch.states);
        core::mem::swap(&mut self.records, &mut self.scratch.records);
        core::mem::swap(&mut self.events, &mut self.scratch.events);
        self.dropped_events = self.dropped_events.saturating_add(self.scratch.dropped);
        self.committed = true;
        if self.retimed_pending {
            self.retimed.fill(None);
            self.retimed_pending = false;
        }
        Ok(interval)
    }

    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.committed {
            return Err(Error::InvalidState);
        }
        let end = self
            .frame
            .checked_add(self.quantum.frames())
            .map_err(|_| Error::Exhausted)?;
        if self.preview.map(|interval| interval.end) != Some(end) {
            return Err(Error::InvalidState);
        }
        let frames = end.0 - self.frame.0;
        for state in self.states.iter_mut().flatten() {
            if state.advances()
                || (state.mode == Mode::Playing
                    && state.composite.is_some_and(|cursor| !cursor.waiting_repeat))
            {
                state.position += frames;
            }
            if state.mode.records() {
                state.capture_frames += frames;
            }
        }
        self.frame = end;
        self.preview = None;
        self.committed = false;
        Ok(())
    }

    pub(crate) fn confirm_capture_frames(
        &mut self,
        target: LoopId,
        revision: Revision,
        frames: u64,
    ) -> Result<(), Error> {
        if self.committed || frames % self.quantum.frames() as u64 != 0 {
            return Err(Error::InvalidState);
        }
        let slot = target.slot() as usize;
        let state = self
            .states
            .get_mut(slot)
            .and_then(Option::as_mut)
            .filter(|state| state.id == target)
            .ok_or(Error::InvalidId)?;
        if state.mode != Mode::Recording
            || state.capture_revision != revision
            || frames > state.capture_frames
        {
            return Err(Error::InvalidState);
        }
        state.capture_frames = frames;
        Ok(())
    }

    /// Displaced owners are returned, never destroyed on this path.
    pub fn replace_topology(
        &mut self,
        expected: Epoch,
        candidate: PreparedTopology,
    ) -> Result<PreparedTopology, (Error, PreparedTopology)> {
        self.replace_topology_inner(expected, candidate, false)
    }

    /// Replace every logical owner, retiring playback at publication. Recording is refused.
    /// The caller must coordinate media/graph ownership and device MIDI cleanup.
    pub fn replace_topology_stopping_playback(
        &mut self,
        expected: Epoch,
        candidate: PreparedTopology,
    ) -> Result<PreparedTopology, (Error, PreparedTopology)> {
        self.replace_topology_inner(expected, candidate, true)
    }

    fn replace_topology_inner(
        &mut self,
        expected: Epoch,
        candidate: PreparedTopology,
        stop_playback: bool,
    ) -> Result<PreparedTopology, (Error, PreparedTopology)> {
        let validation = (|| {
            {
                let q = self.quantum;
                candidate.validate_quantum(q)?;
            }
            if self.quiescent || self.committed || expected != self.epoch {
                return Err(Error::InvalidState);
            }
            if candidate.limits() != self.topology.limits() {
                return Err(Error::InvalidProgram);
            }
            for state in self.states.iter().flatten() {
                if candidate.node(state.id).is_err()
                    && state.mode != Mode::Stopped
                    && !(stop_playback && state.mode.playback_mode())
                {
                    return Err(Error::InvalidState);
                }
            }
            for node in candidate.nodes() {
                if stop_playback && self.state(node.id).is_ok() {
                    return Err(Error::InvalidId);
                }
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
            if self.scratch.states[slot].is_none_or(|state| state.id != node.id) {
                self.retimed[slot] = None;
            }
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
                        if (!record.followup
                            && record.batch.is_none()
                            && target.sync_source != Some(source))
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
        Ok(core::mem::replace(&mut self.topology, candidate))
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
    fn repeat_ready(&self, id: LoopId) -> Result<bool, Error> {
        let state = self.scratch.states[id.slot() as usize].ok_or(Error::InvalidId)?;
        if !state.repeat_sync {
            return Ok(true);
        }
        let Some(source) = self
            .topology
            .node(id)?
            .sync_source
            .filter(|source| *source != id)
        else {
            return Ok(true);
        };
        let source_slot = source.slot() as usize;
        Ok(!self.scratch.states[source_slot]
            .ok_or(Error::InvalidId)?
            .mode
            .playback_mode()
            || self.scratch.delivered[source_slot])
    }

    fn natural_boundary(&mut self, id: LoopId) -> Result<(), Error> {
        let slot = id.slot() as usize;
        let state = self.scratch.states[slot].ok_or(Error::InvalidId)?;
        if !((state.mode.playback_mode() || state.mode == Mode::Replacing)
            || (state.composite.is_none()
                && state.mode == Mode::Recording
                && state.monitor_previous))
            || !self.repeat_ready(id)?
        {
            return Ok(());
        }
        if let Some(cursor) = state.composite {
            let program = self.topology.program(id).ok_or(Error::InvalidProgram)?;
            let timing = self
                .replaced
                .filter(|old| old.source == id)
                .unwrap_or(program);
            if timing.kind == ProgramKind::Regular
                && program.iterations != 0
                && (cursor.waiting_repeat
                    || (cursor.iteration.checked_add(1) == Some(timing.iterations)
                        && cursor
                            .entered_at
                            .and_then(|at| at.0.checked_add(timing.nominal_unit_frames))
                            == Some(self.frame.0)))
            {
                self.scratch.pulses[slot] = true;
            }
        } else if state.content_length != 0 && state.position == state.content_length {
            let state = self.scratch.states[slot].as_mut().expect("validated state");
            state.cycles = state.cycles.checked_add(1).ok_or(Error::Exhausted)?;
            state.position = 0;
            self.scratch.playback_discontinuities[slot] = true;
            self.scratch.pulses[slot] = true;
            self.event(id, EventKind::Wrap);
        }
        Ok(())
    }

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
        if let Some(reason) = intent
            .origin
            .and_then(|id| self.scratch.records[id.slot() as usize])
            .and_then(|record| self.scratch.batch_failures[record.id.slot() as usize])
        {
            self.terminal(
                intent.request,
                Outcome::Failed {
                    frame: self.frame,
                    reason,
                },
            );
            return;
        }

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
        if let Some(reason) = intent
            .origin
            .and_then(|id| self.scratch.records[id.slot() as usize])
            .and_then(|record| self.scratch.batch_failures[record.id.slot() as usize])
        {
            self.terminal(
                intent.request,
                Outcome::Failed {
                    frame: self.frame,
                    reason,
                },
            );
            return;
        }

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
                            && matches!(
                                intent.transition,
                                Transition::Record { .. }
                                    | Transition::Rerecord
                                    | Transition::RecordPrefix { .. }
                                    | Transition::ReplaceInput { .. }
                            ) =>
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
                program.nominal_unit_frames,
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
                    self.scratch.pulses[target] = false;
                }
                let batch_member = intent
                    .request
                    .and_then(|slot| self.scratch.records[slot])
                    .is_some_and(|record| record.batch.is_some());
                if batch_member
                    || self.scratch.records.iter().flatten().any(|record| {
                        record.followup
                            && record.request.target == state.id
                            && record.terminal.is_none()
                    })
                    || matches!(
                        intent.transition,
                        Transition::Stop | Transition::Adopt { .. }
                    )
                {
                    if let Priority::Direct(sequence) = intent.priority {
                        for index in 0..self.scratch.records.len() {
                            let Some(record) = self.scratch.records[index] else {
                                continue;
                            };
                            if record.terminal.is_none()
                                && record.request.target == state.id
                                && record.request.transition != Transition::Program
                                && record.request.sequence < sequence
                            {
                                for old in self.scratch.records.iter_mut().flatten() {
                                    if old.id == record.id
                                        || record.batch.is_some() && old.batch == record.batch
                                    {
                                        old.terminal =
                                            Some(Outcome::Cancelled { frame: self.frame });
                                    }
                                }
                            }
                        }
                    }
                }
                if intent.transition == Transition::Stop
                    || matches!(
                        intent.transition,
                        Transition::Adopt {
                            mode: Mode::Stopped,
                            ..
                        }
                    )
                {
                    self.scratch.masked[target] = true;
                } else if pulse {
                    self.scratch.masked[target] = false;
                    self.scratch.pulses[target] = true;
                }
            }
        }
    }

    fn settle(&mut self, probe: Option<LoopId>) -> Result<(), Error> {
        for observation in self.observations {
            if let Observation::CaptureAborted { target, .. }
            | Observation::CaptureExhausted { target } = *observation
            {
                for record in self.scratch.records.iter_mut().flatten() {
                    if record.followup
                        && record.request.target == target
                        && record.terminal.is_none()
                    {
                        record.terminal = Some(Outcome::Cancelled { frame: self.frame });
                    }
                }
            }
            if let Observation::CaptureAborted { target, reason } = *observation {
                let state = self.scratch.states[target.slot() as usize]
                    .as_mut()
                    .expect("validated observation");
                state.mode = Mode::Stopped;
                state.position = 0;
                state.capture_frames = 0;
                state.capture_incomplete = true;
                state.monitor_previous = false;
                self.event(target, EventKind::Refused(reason));
            }
            if let Observation::CaptureExhausted { target } = *observation {
                let slot = target.slot() as usize;
                let state = self.scratch.states[slot]
                    .as_mut()
                    .expect("validated observation");
                state.capture_incomplete = true;
                if state.mode == Mode::Replacing {
                    state.content_revision = state.capture_revision;
                }
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
                && record.request.transition != Transition::Program
            {
                if record.request.timing == Timing::At {
                    self.offer(record.request.target.slot() as usize, self.direct(slot));
                } else if self.scratch.clock_edges[slot] {
                    self.scratch.eligible[slot] = true;
                    if record.remaining == 0 {
                        self.offer(record.request.target.slot() as usize, self.direct(slot));
                    } else {
                        self.scratch.records[slot]
                            .as_mut()
                            .expect("prepared record")
                            .remaining -= 1;
                    }
                }
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
                self.natural_boundary(id)?;
                if self.scratch.states[slot].is_some_and(|state| state.composite.is_none())
                    && self
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
                        self.react(id, source)?;
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
                        if !self.scratch.masked[target]
                            && self.scratch.states[target]
                                .is_some_and(|state| state.composite.is_none())
                        {
                            self.scratch.pulses[target] = true;
                        }
                        self.react(follower, id)?;
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
            if state.mode != Mode::Playing {
                return Ok(None);
            }
            let timing = self
                .replaced
                .filter(|old| old.source == id)
                .unwrap_or(program);
            if cursor.waiting_repeat {
                if !self.repeat_ready(id)? {
                    return Ok(None);
                }
                cursor.waiting_repeat = false;
                cursor.iteration = 0;
                state.position = 0;
                action = ProgramAction::Snapshot { restart: true };
            } else {
                let due = cursor
                    .entered_at
                    .ok_or(Error::InvalidState)?
                    .0
                    .checked_add(timing.nominal_unit_frames)
                    .ok_or(Error::Exhausted)?;
                if self.frame.0 < due {
                    return Ok(None);
                }
                if self.frame.0 != due {
                    return Err(Error::InvalidFrame);
                }
                cursor.iteration = cursor.iteration.checked_add(1).ok_or(Error::Exhausted)?;
                if cursor.iteration == timing.iterations {
                    cursor.passes = cursor.passes.checked_add(1).ok_or(Error::Exhausted)?;
                    if timing.kind == ProgramKind::Explicit {
                        cursor.iteration = 0;
                        state.position = 0;
                        state.mode = Mode::Stopped;
                        action = ProgramAction::Stop;
                        self.event(id, EventKind::ProgramEnd);
                    } else if self.repeat_ready(id)? {
                        cursor.iteration = 0;
                        state.position = 0;
                        action = ProgramAction::Wrap;
                    } else {
                        cursor.iteration = timing.iterations - 1;
                        cursor.waiting_repeat = true;
                        action = ProgramAction::End;
                    }
                }
            }
            cursor.entered_at = Some(self.frame);
        }
        if self.replacement.is_some_and(|change| change.target == id)
            && program.iterations == 0
            && state.mode == Mode::Playing
        {
            state.mode = Mode::Stopped;
            cursor.iteration = 0;
            cursor.waiting_repeat = false;
            state.position = 0;
            action = ProgramAction::Stop;
        }
        if !matches!(action, ProgramAction::Stop | ProgramAction::End) {
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
            let desired = if matches!(action, ProgramAction::Stop | ProgramAction::End) {
                None
            } else {
                selected
            };
            let child = self.scratch.states[target.slot() as usize].expect("prepared target");
            let nested_entry = action == ProgramAction::Wrap
                && desired.is_some_and(|segment| segment.mode == Mode::Playing)
                && self
                    .topology
                    .program(target)
                    .is_some_and(|program| program_needs_entry_at(program, child, self.frame));
            if matches!(action, ProgramAction::None | ProgramAction::Wrap)
                && !nested_entry
                && old.map(|segment| segment.mode) == desired.map(|segment| segment.mode)
            {
                continue;
            }
            let ordinal = desired
                .map_or(self.topology.limits().program_entries + index, |segment| {
                    segment.ordinal as usize
                });
            let operation = match desired {
                None => Transition::Stop,
                Some(segment) if segment.mode == Mode::Stopped => Transition::Stop,
                Some(segment) if segment.mode == Mode::RecordingDryIntoWet => Transition::Rerecord,
                Some(segment) if segment.mode == Mode::Recording => Transition::Record {
                    monitor_previous: false,
                },
                Some(segment) if segment.mode == Mode::Replacing => {
                    Transition::ReplaceInput { position: None }
                }
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
                        } else if !restart && child.mode == segment.mode {
                            Transition::Seek {
                                position: offset % length,
                                retrigger: false,
                            }
                        } else if segment.mode == Mode::PlayingDryThroughWet {
                            Transition::PlayDry {
                                position: offset % length,
                                retrigger: true,
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

    fn react(&mut self, target: LoopId, source: LoopId) -> Result<(), Error> {
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
        self.natural_boundary(target)
    }
}

fn initial_state(node: LoopSpec) -> State {
    let mut state = State::new(node.id, node.length);
    if node.kind == LoopKind::Composite {
        state.composite = Some(CompositeState::default());
    }
    state
}

fn program_needs_entry_at(
    program: crate::program::CompiledProgram<'_>,
    child: State,
    frame: Frame,
) -> bool {
    if child.mode != Mode::Playing {
        return true;
    }
    if program.kind != ProgramKind::Explicit {
        return false;
    }
    let Some(cursor) = child.composite else {
        return true;
    };
    cursor
        .entered_at
        .and_then(|entered| {
            program
                .iterations
                .checked_sub(cursor.iteration)
                .and_then(|remaining| remaining.checked_mul(program.nominal_unit_frames))
                .and_then(|remaining| entered.0.checked_add(remaining))
        })
        .is_none_or(|end| frame.0 >= end)
}

fn composite_transition(
    state: &mut State,
    operation: Transition,
    iterations: u64,
    unit_frames: u64,
    frame: Frame,
) -> Result<(bool, ProgramAction), Error> {
    let mut cursor = state.composite.ok_or(Error::InvalidState)?;
    let (iteration, retrigger, seeking) = match operation {
        Transition::Stop => {
            state.mode = Mode::Stopped;
            state.position = 0;
            cursor.iteration = 0;
            cursor.waiting_repeat = false;
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
        state.position = 0;
        return Ok((false, ProgramAction::Stop));
    }
    if iteration >= iterations {
        return Err(Error::InvalidFrame);
    }
    state.mode = Mode::Playing;
    state.position = iteration.checked_mul(unit_frames).ok_or(Error::Exhausted)?;
    cursor.iteration = iteration;
    cursor.waiting_repeat = false;
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
        *slot = core::hint::black_box(value);
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
        Mode::PlayingDryThroughWet => Transition::PlayDry {
            position: 0,
            retrigger: false,
        },
        Mode::RecordingDryIntoWet => Transition::Rerecord,
        Mode::Recording => Transition::Record {
            monitor_previous: false,
        },
        Mode::Replacing => Transition::ReplaceInput { position: None },
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
        Transition::Seek { .. } | Transition::Replace { .. } | Transition::Adopt { .. } => true,
        Transition::Play { retrigger, .. } | Transition::PlayDry { retrigger, .. } => {
            retrigger || !before.mode.playback_mode()
        }
        Transition::Rerecord => !before.mode.playback_mode(),
        _ => false,
    }
}

fn transition(state: &mut State, operation: Transition) -> Result<bool, Error> {
    match operation {
        Transition::Program
        | Transition::PlayIteration { .. }
        | Transition::SeekIteration { .. } => Err(Error::InvalidState),
        Transition::Stop => {
            if state.writes() {
                if state.mode == Mode::Recording {
                    state.content_length = state.capture_frames;
                }
                state.content_revision = state.capture_revision;
            }
            state.mode = Mode::Stopped;
            state.position = 0;
            state.monitor_previous = false;
            Ok(false)
        }
        Transition::ReplaceInput { position } => {
            let entering = state.mode != Mode::Replacing;
            if state.mode == Mode::Recording {
                state.content_length = state.capture_frames;
                state.content_revision = state.capture_revision;
            }
            if state.content_length == 0 {
                return Err(Error::EmptyContent);
            }
            let preserve =
                position.is_none() && matches!(state.mode, Mode::Playing | Mode::Replacing);
            let position = position.unwrap_or_else(|| {
                if matches!(state.mode, Mode::Playing | Mode::Replacing) {
                    state.position
                } else {
                    0
                }
            });
            if position >= state.content_length && !(preserve && position == state.content_length) {
                return Err(Error::InvalidFrame);
            }
            if state.mode != Mode::Replacing {
                state.capture_revision = state
                    .revision_watermark
                    .checked_next()
                    .map_err(|_| Error::Exhausted)?;
                state.revision_watermark = state.capture_revision;
            }
            if !matches!(state.mode, Mode::Playing | Mode::Replacing) {
                state.cycles = 0;
            }
            state.mode = Mode::Replacing;
            state.position = position;
            state.capture_frames = state.content_length;
            state.capture_incomplete = false;
            state.monitor_previous = false;
            Ok(entering && position == 0)
        }
        Transition::RecordPrefix { frames } => {
            transition(
                state,
                Transition::Record {
                    monitor_previous: false,
                },
            )?;
            state.capture_frames = frames;
            Ok(true)
        }
        Transition::Record { monitor_previous } => {
            if state.mode == Mode::Replacing {
                state.content_revision = state.capture_revision;
            }
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
        }
        | Transition::PlayDry {
            position,
            retrigger,
        } => {
            if state.mode == Mode::Replacing && !retrigger {
                state.content_revision = state.capture_revision;
                state.mode = if matches!(operation, Transition::PlayDry { .. }) {
                    Mode::PlayingDryThroughWet
                } else {
                    Mode::Playing
                };
                state.monitor_previous = false;
                return Ok(false);
            }
            let next = if matches!(operation, Transition::PlayDry { .. }) {
                Mode::PlayingDryThroughWet
            } else {
                Mode::Playing
            };
            let preserve = state.mode.playback_mode() && !retrigger;
            if state.writes() {
                if state.mode == Mode::Recording {
                    state.content_length = state.capture_frames;
                }
                state.content_revision = state.capture_revision;
            }
            if state.content_length == 0 {
                return Err(Error::EmptyContent);
            }
            if !preserve && position >= state.content_length {
                return Err(Error::InvalidFrame);
            }
            state.mode = next;
            state.monitor_previous = false;
            if !preserve {
                state.position = position;
                state.cycles = 0;
            }
            Ok(!preserve)
        }
        Transition::Rerecord => {
            if state.mode == Mode::RecordingDryIntoWet {
                return Ok(false);
            }
            if state.mode == Mode::Recording || state.content_length == 0 {
                return Err(Error::EmptyContent);
            }
            let preserve = state.mode.playback_mode();
            state.capture_revision = state
                .revision_watermark
                .checked_next()
                .map_err(|_| Error::Exhausted)?;
            state.revision_watermark = state.capture_revision;
            state.capture_frames = 0;
            state.capture_incomplete = false;
            state.monitor_previous = false;
            state.mode = Mode::RecordingDryIntoWet;
            if !preserve {
                state.position = 0;
                state.cycles = 0;
            }
            Ok(!preserve)
        }
        Transition::Seek {
            position,
            retrigger,
        } => {
            if !state.mode.playback_mode() && state.mode != Mode::Replacing {
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
        Transition::Adopt {
            length,
            revision,
            position,
            mode,
        } => {
            if position > length {
                return Err(Error::InvalidFrame);
            }
            state.content_length = length;
            state.content_revision = revision;
            state.revision_watermark = state.revision_watermark.max(revision);
            state.mode = Mode::Stopped;
            state.position = position;
            state.cycles = 0;
            state.monitor_previous = false;
            match mode {
                Mode::Stopped => Ok(false),
                Mode::Playing => transition(
                    state,
                    Transition::Play {
                        position,
                        retrigger: true,
                    },
                ),
                Mode::Recording => transition(state, Transition::RecordPrefix { frames: length }),
                Mode::PlayingDryThroughWet | Mode::RecordingDryIntoWet => Err(Error::InvalidState),
                Mode::Replacing => transition(
                    state,
                    Transition::ReplaceInput {
                        position: Some(position),
                    },
                ),
            }
        }
        Transition::Replace {
            length,
            revision,
            preserve_position,
        } => {
            if state.writes() {
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

#[cfg(test)]
mod clock_tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn final_tick_and_exhausted_clocks_do_not_wrap_or_reexecute() {
        for frames in [1, 16, 32, 64, 128] {
            let topology = crate::compile(
                crate::Limits::default(),
                &crate::TopologyDescription::default(),
            )
            .unwrap();
            let mut resolver =
                Resolver::prepare(topology, rt_runtime::Quantum::new(frames).unwrap()).unwrap();
            resolver.frame = Frame(u64::MAX - (frames as u64 - 1));
            let at = resolver.frame;
            assert_eq!(resolver.preview_tick(&[]), Err(Error::Exhausted));
            assert_eq!(resolver.frame(), at);
            assert_eq!(resolver.decisions(), Err(Error::InvalidState));
            assert_eq!(resolver.finish(), Err(Error::InvalidState));
            if frames == 1 {
                resolver.frame = Frame(u64::MAX - 1);
                assert_eq!(resolver.preview_tick(&[]).unwrap().end, Frame(u64::MAX));
                resolver.commit().unwrap();
                resolver.finish().unwrap();
                assert_eq!(resolver.frame(), Frame(u64::MAX));
                assert_eq!(resolver.preview_tick(&[]), Err(Error::Exhausted));
                assert_eq!(resolver.finish(), Err(Error::InvalidState));
            }
        }
    }
}
