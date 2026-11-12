use super::index::{PendingRequests, SlotMask};
use super::types::Record;
use super::*;
use alloc::{vec, vec::Vec};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Settled,
    Faulted(TransitionError),
}

struct Scratch {
    intents: Vec<Option<usize>>,
    protected: Vec<Option<u64>>,
    ready: SlotMask,
    absolute_to_visit: SlotMask,
    delivered: Vec<bool>,
    masked: Vec<bool>,
    eligible: Vec<bool>,
}

pub struct TransitionProcessor {
    plan: TransitionPlan,
    states: Vec<Option<LoopState>>,
    records: Vec<Option<Record>>,
    pending: PendingRequests,
    generations: Vec<u32>,
    events: Vec<Event>,
    scratch: Scratch,
    frame: u64,
    phase: Phase,
}

#[derive(Clone, Copy)]
pub(crate) struct SettledLoops<'a> {
    states: &'a [Option<LoopState>],
}
impl SettledLoops<'_> {
    pub fn read(self, id: LoopId) -> Result<LoopState, TransitionError> {
        self.states
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|state| state.id == id)
            .ok_or(TransitionError::InvalidId)
    }
}

impl TransitionProcessor {
    pub fn prepare(plan: TransitionPlan) -> Self {
        let states = plan.initial_states();
        let limits = plan.image.limits;
        let n = limits.loop_slots as usize;
        let p = limits.pending_requests as usize;
        Self {
            scratch: Scratch {
                intents: vec![None; n],
                protected: vec![None; n],
                ready: SlotMask::new(n),
                absolute_to_visit: SlotMask::new(p),
                delivered: vec![false; n],
                masked: vec![false; n],
                eligible: vec![false; p],
            },
            plan,
            states,
            records: vec![None; p],
            pending: PendingRequests::new(n, p),
            generations: vec![1; p],
            events: Vec::with_capacity(limits.events as usize),
            frame: 0,
            phase: Phase::Idle,
        }
    }
    pub fn plan(&self) -> &TransitionPlanImage {
        self.plan.image()
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub(crate) fn settled(&self) -> Result<SettledLoops<'_>, TransitionError> {
        if self.phase != Phase::Settled {
            return Err(TransitionError::InvalidState);
        }
        Ok(SettledLoops {
            states: &self.states,
        })
    }
    pub fn states(&self) -> impl Iterator<Item = LoopState> + '_ {
        self.states.iter().flatten().copied()
    }
    pub fn state(&self, id: LoopId) -> Result<LoopState, TransitionError> {
        self.states
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|s| s.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    fn spec(&self, id: LoopId) -> Result<LoopSpec, TransitionError> {
        self.plan
            .specs
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|s| s.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    fn record(&self, id: RequestId) -> Result<Record, TransitionError> {
        self.records
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|r| r.id == id)
            .ok_or(TransitionError::InvalidId)
    }
    pub fn request_slots(&self) -> impl Iterator<Item = Option<RequestSnapshot>> + '_ {
        self.records.iter().map(|r| r.map(Record::snapshot))
    }
    pub fn completion(&self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        Ok(self.record(id)?.completion())
    }
    pub fn take_completion(
        &mut self,
        id: RequestId,
    ) -> Result<Option<Completion>, TransitionError> {
        self.idle()?;
        let completion = self.completion(id)?;
        if completion.is_some() {
            self.records[id.slot as usize] = None;
            self.generations[id.slot as usize] = id.generation.checked_add(1).unwrap_or(0);
        }
        Ok(completion)
    }
    pub fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError> {
        self.idle()?;
        let record = self.record(id)?;
        if record.outcome.is_some() {
            return Ok(false);
        }
        self.records[id.slot as usize]
            .as_mut()
            .ok_or(TransitionError::InvalidId)?
            .outcome = Some(Outcome::Cancelled { frame: self.frame });
        self.pending.remove(
            record.command.target.slot as usize,
            id.slot as usize,
            record.command.timing == Timing::At,
        );
        Ok(true)
    }
    fn idle(&self) -> Result<(), TransitionError> {
        if self.phase == Phase::Idle {
            Ok(())
        } else {
            Err(TransitionError::InvalidState)
        }
    }
    pub fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError> {
        self.idle()?;
        let target = self.spec(command.target)?;
        if let Operation::Play { position, .. } | Operation::Seek { position, .. } =
            command.operation
        {
            if position % u64::from(self.plan.image.description.quantum) != 0 {
                return Err(TransitionError::InvalidFrame);
            }
        }
        if let Timing::AfterTicks { source, .. } = command.timing {
            self.spec(source)?;
            if target.sync_source != Some(source) {
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
        let slot = self
            .records
            .iter()
            .enumerate()
            .position(|(i, r)| r.is_none() && self.generations[i] != 0)
            .ok_or(TransitionError::Capacity)?;
        let q = u64::from(self.plan.image.description.quantum);
        let at = command.at.max(self.frame);
        let remainder = at & (q - 1);
        let effective = at
            .checked_add(if remainder == 0 { 0 } else { q - remainder })
            .ok_or(TransitionError::InvalidFrame)?;
        let id = RequestId {
            slot: slot as u32,
            generation: self.generations[slot],
        };
        self.records[slot] = Some(Record {
            id,
            command,
            effective,
            remaining: match command.timing {
                Timing::At => 0,
                Timing::AfterTicks { skip, .. } => skip,
            },
            outcome: None,
        });
        self.pending.insert(
            command.target.slot as usize,
            slot,
            command.timing == Timing::At,
        );
        Ok(id)
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn fault(&self) -> Option<TransitionError> {
        match self.phase {
            Phase::Faulted(error) => Some(error),
            _ => None,
        }
    }
    pub fn process(&mut self) -> Result<(), TransitionError> {
        if let Some(error) = self.fault() {
            return Err(error);
        }
        self.settle()?;
        self.finish()
    }
    pub(crate) fn settle(&mut self) -> Result<(), TransitionError> {
        self.idle()?;
        let result = self.settle_inner();
        self.phase = match result {
            Ok(()) => Phase::Settled,
            Err(error) => Phase::Faulted(error),
        };
        result
    }
    fn settle_inner(&mut self) -> Result<(), TransitionError> {
        self.frame
            .checked_add(u64::from(self.plan.image.description.quantum))
            .ok_or(TransitionError::Exhausted)?;
        self.scratch
            .absolute_to_visit
            .copy_from(&self.pending.absolute);
        self.scratch.intents.fill(None);
        self.scratch.protected.fill(None);
        self.scratch.ready.clear();
        self.scratch.delivered.fill(false);
        self.scratch.masked.fill(false);
        self.scratch.eligible.fill(false);
        self.events.clear();
        while let Some(i) = self.scratch.absolute_to_visit.pop_first() {
            if self.records[i].is_some_and(|r| r.outcome.is_none() && r.effective <= self.frame) {
                self.offer(i);
            }
        }
        for c in 0..self.plan.image.components.len() {
            let len = self.plan.image.components[c].len();
            for i in 0..len {
                let id = self.plan.image.components[c][i];
                self.apply_pending(id)?;
                self.natural_boundary(id)?;
                if self
                    .spec(id)?
                    .sync_source
                    .is_some_and(|source| self.scratch.delivered[source.slot as usize])
                    && !self.scratch.masked[id.slot as usize]
                {
                    self.queue_pulse(id);
                }
            }
            for i in 0..len {
                let id = self.plan.image.components[c][i];
                if let Some(source) = self
                    .spec(id)?
                    .sync_source
                    .filter(|s| self.scratch.delivered[s.slot as usize])
                {
                    self.react(id, source)?;
                }
            }
            while let Some(slot) = self.scratch.ready.pop_first() {
                debug_assert_eq!(self.plan.component_of[slot], c);
                let id = self.states[slot].ok_or(TransitionError::InvalidId)?.id;
                self.scratch.delivered[slot] = true;
                self.states[slot]
                    .as_mut()
                    .ok_or(TransitionError::InvalidId)?
                    .last_pulse = Some(self.frame);
                self.event(id, EventKind::Pulse)?;
                for i in self.plan.follower_ranges[slot].clone() {
                    let follower = self.plan.followers[i];
                    if self.plan.component_of[follower.slot as usize] == c {
                        if !self.scratch.masked[follower.slot as usize] {
                            self.queue_pulse(follower);
                        }
                        self.react(follower, id)?;
                    }
                }
            }
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
    fn queue_pulse(&mut self, id: LoopId) {
        let slot = id.slot as usize;
        if !self.scratch.delivered[slot] {
            self.scratch.ready.insert(slot);
        }
    }
    fn terminal(&mut self, request: usize, outcome: Outcome) {
        let record = self.records[request].as_mut().expect("retained request");
        debug_assert!(record.outcome.is_none());
        record.outcome = Some(outcome);
        let absolute = record.command.timing == Timing::At;
        self.pending
            .remove(record.command.target.slot as usize, request, absolute);
        if absolute {
            self.scratch.absolute_to_visit.remove(request);
        }
    }
    fn offer(&mut self, request: usize) {
        let record = self.records[request].expect("retained request");
        let target = record.command.target.slot as usize;
        if let Some(previous) = self.scratch.intents[target] {
            let old = self.records[previous].expect("retained intent");
            let loser = if record.command.sequence > old.command.sequence {
                previous
            } else {
                request
            };
            self.terminal(loser, Outcome::Superseded { frame: self.frame });
            if loser == request {
                return;
            }
        }
        self.scratch.intents[target] = Some(request);
    }
    fn apply_pending(&mut self, id: LoopId) -> Result<(), TransitionError> {
        let slot = id.slot as usize;
        let Some(request) = self.scratch.intents[slot].take() else {
            return Ok(());
        };
        let record = self.records[request].expect("retained request");
        if self.scratch.protected[slot].is_some_and(|seq| record.command.sequence <= seq) {
            self.terminal(request, Outcome::Superseded { frame: self.frame });
            return Ok(());
        }
        self.scratch.protected[slot] = Some(record.command.sequence);
        let before = self.states[slot].ok_or(TransitionError::InvalidId)?;
        let mut state = before;
        let result = apply_operation(&mut state, record.command.operation);
        match result {
            Err(reason) => {
                self.terminal(
                    request,
                    Outcome::Failed {
                        frame: self.frame,
                        reason,
                    },
                );
            }
            Ok(pulse) => {
                self.states[slot] = Some(state);
                self.terminal(request, Outcome::Applied { frame: self.frame });
                if before != state {
                    self.event(id, EventKind::Transition(record.command.operation))?;
                }
                if record.command.operation == Operation::Stop {
                    self.scratch.masked[slot] = true;
                    self.scratch.ready.remove(slot);
                    for absolute in [true, false] {
                        let mut next = self.pending.first(slot, absolute);
                        while let Some(i) = next {
                            next = self.pending.next(i);
                            let old = self.records[i].expect("indexed pending request");
                            if old.command.sequence < record.command.sequence {
                                self.terminal(i, Outcome::Cancelled { frame: self.frame });
                            }
                        }
                    }
                } else if pulse {
                    self.scratch.masked[slot] = false;
                    self.queue_pulse(id);
                }
            }
        }
        Ok(())
    }
    fn natural_boundary(&mut self, id: LoopId) -> Result<(), TransitionError> {
        let slot = id.slot as usize;
        let state = self.states[slot].ok_or(TransitionError::InvalidId)?;
        if !state.at_end() {
            return Ok(());
        }
        if state.repeat_sync {
            if let Some(source) = self.spec(id)?.sync_source.filter(|source| *source != id) {
                if self.states[source.slot as usize]
                    .ok_or(TransitionError::InvalidId)?
                    .mode
                    == Mode::Playing
                    && !self.scratch.delivered[source.slot as usize]
                {
                    return Ok(());
                }
            }
        }
        let state = self.states[slot].as_mut().expect("prepared state");
        state.cycles = state
            .cycles
            .checked_add(1)
            .ok_or(TransitionError::Exhausted)?;
        state.position = 0;
        self.queue_pulse(id);
        self.event(id, EventKind::Wrap)
    }
    fn react(&mut self, target: LoopId, source: LoopId) -> Result<(), TransitionError> {
        let mut next = self.pending.first(target.slot as usize, false);
        while let Some(i) = next {
            next = self.pending.next(i);
            let record = self.records[i].expect("indexed pending request");
            if record.outcome.is_some()
                || record.effective > self.frame
                || record.command.target != target
                || self.scratch.eligible[i]
            {
                continue;
            }
            if let Timing::AfterTicks {
                source: expected, ..
            } = record.command.timing
            {
                if expected == source {
                    self.scratch.eligible[i] = true;
                    if record.remaining != 0 {
                        self.records[i]
                            .as_mut()
                            .expect("retained request")
                            .remaining -= 1;
                    } else {
                        self.offer(i);
                    }
                }
            }
        }
        self.apply_pending(target)?;
        self.natural_boundary(target)
    }
    pub(crate) fn finish(&mut self) -> Result<(), TransitionError> {
        if self.phase != Phase::Settled {
            return Err(TransitionError::InvalidState);
        }
        let result = self.finish_inner();
        self.phase = match result {
            Ok(()) => Phase::Idle,
            Err(error) => Phase::Faulted(error),
        };
        result
    }
    fn finish_inner(&mut self) -> Result<(), TransitionError> {
        let q = u64::from(self.plan.image.description.quantum);
        let next = self
            .frame
            .checked_add(q)
            .ok_or(TransitionError::Exhausted)?;
        for state in self.states.iter_mut().flatten() {
            if state.plays() {
                state.position = state
                    .position
                    .checked_add(q)
                    .filter(|position| *position <= state.content_length)
                    .ok_or(TransitionError::InvalidFrame)?;
            }
            if state.mode == Mode::Recording {
                state.capture_frames = state
                    .capture_frames
                    .checked_add(q)
                    .ok_or(TransitionError::Exhausted)?;
            }
        }
        self.frame = next;
        Ok(())
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
