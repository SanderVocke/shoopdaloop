use super::types::Record;
use super::*;
use alloc::{vec, vec::Vec};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Preparing,
    Preview(BoundaryId),
    Committed(BoundaryId),
}

struct Scratch {
    states: Vec<Option<LoopState>>,
    records: Vec<Option<Record>>,
    intents: Vec<Option<usize>>,
    protected: Vec<Option<u64>>,
    origins: Vec<Option<RequestId>>,
    discontinuities: Vec<bool>,
    pulses: Vec<bool>,
    delivered: Vec<bool>,
    masked: Vec<bool>,
    eligible: Vec<bool>,
    decisions: Vec<Decision>,
    events: Vec<Event>,
}

pub struct TransitionProcessor {
    plan: TransitionPlan,
    states: Vec<Option<LoopState>>,
    records: Vec<Option<Record>>,
    generations: Vec<u32>,
    events: Vec<Event>,
    scratch: Scratch,
    frame: u64,
    serial: u64,
    phase: Phase,
}

impl TransitionProcessor {
    pub fn prepare(plan: TransitionPlan) -> Self {
        let states = plan.initial_states();
        let limits = plan.image.limits;
        let n = limits.loop_slots as usize;
        let p = limits.pending_requests as usize;
        Self {
            scratch: Scratch {
                states: states.clone(),
                records: vec![None; p],
                intents: vec![None; n],
                protected: vec![None; n],
                origins: vec![None; n],
                discontinuities: vec![false; n],
                pulses: vec![false; n],
                delivered: vec![false; n],
                masked: vec![false; n],
                eligible: vec![false; p],
                decisions: Vec::with_capacity(plan.image.description.loops.len()),
                events: Vec::with_capacity(limits.events as usize),
            },
            plan,
            states,
            records: vec![None; p],
            generations: vec![1; p],
            events: Vec::with_capacity(limits.events as usize),
            frame: 0,
            serial: 0,
            phase: Phase::Idle,
        }
    }
    pub fn plan(&self) -> &TransitionPlanImage {
        self.plan.image()
    }
    pub fn frame(&self) -> u64 {
        self.frame
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
        if self.record(id)?.outcome.is_some() {
            return Ok(false);
        }
        self.records[id.slot as usize]
            .as_mut()
            .ok_or(TransitionError::InvalidId)?
            .outcome = Some(Outcome::Cancelled { frame: self.frame });
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
        Ok(id)
    }
    fn check_boundary(&self, id: BoundaryId) -> Result<(), TransitionError> {
        match self.phase {
            Phase::Preview(current) | Phase::Committed(current) if current == id => Ok(()),
            _ => Err(TransitionError::InvalidState),
        }
    }
    pub fn decisions(&self, id: BoundaryId) -> Result<&[Decision], TransitionError> {
        self.check_boundary(id)?;
        Ok(&self.scratch.decisions)
    }
    pub fn boundary_events(&self, id: BoundaryId) -> Result<&[Event], TransitionError> {
        self.check_boundary(id)?;
        Ok(if matches!(self.phase, Phase::Preview(_)) {
            &self.scratch.events
        } else {
            &self.events
        })
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn preview(&mut self, observations: &[Observation]) -> Result<BoundaryId, TransitionError> {
        if matches!(self.phase, Phase::Committed(_)) {
            return Err(TransitionError::InvalidState);
        }
        // Even a failed replacement preview invalidates its predecessor.
        self.phase = Phase::Preparing;
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(TransitionError::Exhausted)?;
        self.frame
            .checked_add(u64::from(self.plan.image.description.quantum))
            .ok_or(TransitionError::Exhausted)?;
        if observations.len() > self.states.len() + self.records.len() {
            return Err(TransitionError::Capacity);
        }
        for (i, observation) in observations.iter().enumerate() {
            match *observation {
                Observation::CaptureUnavailable { target, .. } => {
                    self.spec(target)?;
                }
                Observation::Refused { request, .. } => {
                    if self.record(request)?.outcome.is_some() {
                        return Err(TransitionError::InvalidState);
                    }
                }
            }
            if observations[..i]
                .iter()
                .any(|old| match (*old, *observation) {
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
                return Err(TransitionError::DuplicateId);
            }
        }
        self.scratch.states.copy_from_slice(&self.states);
        self.scratch.records.copy_from_slice(&self.records);
        self.scratch.intents.fill(None);
        self.scratch.protected.fill(None);
        self.scratch.origins.fill(None);
        self.scratch.discontinuities.fill(false);
        self.scratch.pulses.fill(false);
        self.scratch.delivered.fill(false);
        self.scratch.masked.fill(false);
        self.scratch.eligible.fill(false);
        self.scratch.events.clear();
        self.scratch.decisions.clear();
        for i in 0..self.records.len() {
            if self.records[i].is_some_and(|r| {
                r.outcome.is_none() && r.effective <= self.frame && r.command.timing == Timing::At
            }) {
                self.offer(i);
            }
        }
        for c in 0..self.plan.image.components.len() {
            let len = self.plan.image.components[c].len();
            for i in 0..len {
                let id = self.plan.image.components[c][i];
                self.apply_pending(id, observations)?;
                self.natural_boundary(id)?;
                if self
                    .spec(id)?
                    .sync_source
                    .is_some_and(|source| self.scratch.delivered[source.slot as usize])
                    && !self.scratch.masked[id.slot as usize]
                {
                    self.scratch.pulses[id.slot as usize] = true;
                }
            }
            for i in 0..len {
                let id = self.plan.image.components[c][i];
                if let Some(source) = self
                    .spec(id)?
                    .sync_source
                    .filter(|s| self.scratch.delivered[s.slot as usize])
                {
                    self.react(id, source, observations)?;
                }
            }
            loop {
                let next = self.plan.image.components[c].iter().copied().find(|id| {
                    self.scratch.pulses[id.slot as usize]
                        && !self.scratch.delivered[id.slot as usize]
                });
                let Some(id) = next else { break };
                self.scratch.delivered[id.slot as usize] = true;
                self.scratch.states[id.slot as usize]
                    .as_mut()
                    .ok_or(TransitionError::InvalidId)?
                    .last_pulse = Some(self.frame);
                self.event(id, EventKind::Pulse)?;
                for i in 0..len {
                    let follower = self.plan.image.components[c][i];
                    if self.spec(follower)?.sync_source == Some(id) {
                        if !self.scratch.masked[follower.slot as usize] {
                            self.scratch.pulses[follower.slot as usize] = true;
                        }
                        self.react(follower, id, observations)?;
                    }
                }
            }
        }
        let q = u64::from(self.plan.image.description.quantum);
        for slot in 0..self.scratch.states.len() {
            let Some(state) = self.scratch.states[slot] else {
                continue;
            };
            if state.plays()
                && state
                    .position
                    .checked_add(q)
                    .is_none_or(|p| p > state.content_length)
            {
                return Err(TransitionError::InvalidFrame);
            }
            if state.mode == Mode::Recording {
                state
                    .capture_frames
                    .checked_add(q)
                    .ok_or(TransitionError::Exhausted)?;
            }
            self.scratch.decisions.push(Decision {
                state,
                playback_discontinuity: state.plays() && self.scratch.discontinuities[slot],
                origin: self.scratch.origins[slot],
                waiting_repeat: state.at_end(),
            });
        }
        let id = BoundaryId {
            frame: self.frame,
            serial: self.serial,
        };
        self.phase = Phase::Preview(id);
        Ok(id)
    }
    fn event(&mut self, target: LoopId, kind: EventKind) -> Result<(), TransitionError> {
        if self.scratch.events.len() == self.plan.image.limits.events as usize {
            return Err(TransitionError::Capacity);
        }
        self.scratch.events.push(Event {
            frame: self.frame,
            target,
            kind,
        });
        Ok(())
    }
    fn offer(&mut self, request: usize) {
        let record = self.scratch.records[request].expect("retained request");
        let target = record.command.target.slot as usize;
        if let Some(previous) = self.scratch.intents[target] {
            let old = self.scratch.records[previous].expect("retained intent");
            let loser = if record.command.sequence > old.command.sequence {
                previous
            } else {
                request
            };
            self.scratch.records[loser]
                .as_mut()
                .expect("retained loser")
                .outcome = Some(Outcome::Superseded { frame: self.frame });
            if loser == request {
                return;
            }
        }
        self.scratch.intents[target] = Some(request);
    }
    fn apply_pending(
        &mut self,
        id: LoopId,
        observations: &[Observation],
    ) -> Result<(), TransitionError> {
        let slot = id.slot as usize;
        let Some(request) = self.scratch.intents[slot].take() else {
            return Ok(());
        };
        let record = self.scratch.records[request].expect("retained request");
        if self.scratch.protected[slot].is_some_and(|seq| record.command.sequence <= seq) {
            self.scratch.records[request]
                .as_mut()
                .expect("retained request")
                .outcome = Some(Outcome::Superseded { frame: self.frame });
            return Ok(());
        }
        self.scratch.protected[slot] = Some(record.command.sequence);
        let refused = observations
            .iter()
            .find_map(|observation| match *observation {
                Observation::Refused { request, reason } if request == record.id => Some(reason),
                _ => None,
            })
            .or_else(|| {
                observations
                    .iter()
                    .find_map(|observation| match *observation {
                        Observation::CaptureUnavailable { target, reason }
                            if target == id && record.command.operation == Operation::Record =>
                        {
                            Some(reason)
                        }
                        _ => None,
                    })
            });
        let before = self.scratch.states[slot].ok_or(TransitionError::InvalidId)?;
        let mut state = before;
        let result = if let Some(reason) = refused {
            Err(reason)
        } else {
            apply_operation(&mut state, record.command.operation)
        };
        match result {
            Err(reason) => {
                self.scratch.records[request]
                    .as_mut()
                    .expect("retained request")
                    .outcome = Some(Outcome::Failed {
                    frame: self.frame,
                    reason,
                })
            }
            Ok(pulse) => {
                self.scratch.discontinuities[slot] |= state.plays()
                    && (!before.plays()
                        || match record.command.operation {
                            Operation::Seek { .. } => true,
                            Operation::Play { retrigger, .. } => retrigger,
                            _ => false,
                        });
                self.scratch.states[slot] = Some(state);
                self.scratch.origins[slot] = Some(record.id);
                self.scratch.records[request]
                    .as_mut()
                    .expect("retained request")
                    .outcome = Some(Outcome::Applied { frame: self.frame });
                if before != state {
                    self.event(id, EventKind::Transition(record.command.operation))?;
                }
                if record.command.operation == Operation::Stop {
                    self.scratch.masked[slot] = true;
                    self.scratch.pulses[slot] = false;
                    for old in self.scratch.records.iter_mut().flatten() {
                        if old.outcome.is_none()
                            && old.command.target == id
                            && old.command.sequence < record.command.sequence
                        {
                            old.outcome = Some(Outcome::Cancelled { frame: self.frame });
                        }
                    }
                } else if pulse {
                    self.scratch.masked[slot] = false;
                    self.scratch.pulses[slot] = true;
                }
            }
        }
        Ok(())
    }
    fn natural_boundary(&mut self, id: LoopId) -> Result<(), TransitionError> {
        let slot = id.slot as usize;
        let state = self.scratch.states[slot].ok_or(TransitionError::InvalidId)?;
        if !state.at_end() {
            return Ok(());
        }
        if state.repeat_sync {
            if let Some(source) = self.spec(id)?.sync_source.filter(|source| *source != id) {
                if self.scratch.states[source.slot as usize]
                    .ok_or(TransitionError::InvalidId)?
                    .mode
                    == Mode::Playing
                    && !self.scratch.delivered[source.slot as usize]
                {
                    return Ok(());
                }
            }
        }
        let state = self.scratch.states[slot].as_mut().expect("prepared state");
        state.cycles = state
            .cycles
            .checked_add(1)
            .ok_or(TransitionError::Exhausted)?;
        state.position = 0;
        self.scratch.discontinuities[slot] = true;
        self.scratch.pulses[slot] = true;
        self.event(id, EventKind::Wrap)
    }
    fn react(
        &mut self,
        target: LoopId,
        source: LoopId,
        observations: &[Observation],
    ) -> Result<(), TransitionError> {
        for i in 0..self.scratch.records.len() {
            let Some(record) = self.scratch.records[i] else {
                continue;
            };
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
                        self.scratch.records[i]
                            .as_mut()
                            .expect("retained request")
                            .remaining -= 1;
                    } else {
                        self.offer(i);
                    }
                }
            }
        }
        self.apply_pending(target, observations)?;
        self.natural_boundary(target)
    }
    pub fn commit(&mut self, id: BoundaryId) -> Result<(), TransitionError> {
        if self.phase != Phase::Preview(id) {
            return Err(TransitionError::InvalidState);
        }
        core::mem::swap(&mut self.states, &mut self.scratch.states);
        core::mem::swap(&mut self.records, &mut self.scratch.records);
        core::mem::swap(&mut self.events, &mut self.scratch.events);
        self.phase = Phase::Committed(id);
        Ok(())
    }
    pub fn finish_quantum(&mut self, id: BoundaryId) -> Result<(), TransitionError> {
        if self.phase != Phase::Committed(id) {
            return Err(TransitionError::InvalidState);
        }
        let q = u64::from(self.plan.image.description.quantum);
        let next = self
            .frame
            .checked_add(q)
            .ok_or(TransitionError::Exhausted)?;
        for state in self.states.iter_mut().flatten() {
            if state.plays() {
                state.position += q;
            }
            if state.mode == Mode::Recording {
                state.capture_frames += q;
            }
        }
        self.frame = next;
        self.phase = Phase::Idle;
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

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn processor() -> (TransitionProcessor, LoopId) {
        let id = LoopId {
            slot: 0,
            generation: 1,
        };
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: 16,
                loops: vec![LoopSpec {
                    id,
                    kind: LoopKind::Primitive,
                    length: 16,
                    sync_source: None,
                    repeat_sync: false,
                    timeline: None,
                }],
            },
            TransitionLimits {
                loop_slots: 1,
                pending_requests: 1,
                events: 3,
            },
        )
        .unwrap();
        (TransitionProcessor::prepare(plan), id)
    }
    fn command(target: LoopId, operation: Operation) -> Command {
        Command {
            target,
            sequence: 1,
            at: 0,
            timing: Timing::At,
            operation,
        }
    }

    #[shoop_test]
    fn counter_overflows_leave_committed_state_unchanged() {
        let (mut processor, id) = processor();
        processor.states[0].as_mut().unwrap().mode = Mode::Recording;
        processor.states[0].as_mut().unwrap().capture_frames = u64::MAX - 15;
        let before = processor.state(id).unwrap();
        assert_eq!(processor.preview(&[]), Err(TransitionError::Exhausted));
        assert_eq!(processor.state(id).unwrap(), before);
        let state = processor.states[0].as_mut().unwrap();
        state.mode = Mode::Playing;
        state.position = 16;
        state.cycles = u64::MAX;
        let before = processor.state(id).unwrap();
        assert_eq!(processor.preview(&[]), Err(TransitionError::Exhausted));
        assert_eq!(processor.state(id).unwrap(), before);
        processor.states[0].as_mut().unwrap().mode = Mode::Stopped;
        processor.frame = u64::MAX - 15;
        assert_eq!(processor.preview(&[]), Err(TransitionError::Exhausted));
        processor.frame = 0;
        processor.serial = u64::MAX;
        assert_eq!(processor.preview(&[]), Err(TransitionError::Exhausted));
    }

    #[shoop_test]
    fn exhausted_revision_fails_request_without_partial_record_start() {
        let (mut processor, id) = processor();
        processor.states[0].as_mut().unwrap().revision_watermark = u64::MAX;
        let before = processor.state(id).unwrap();
        let request = processor.submit(command(id, Operation::Record)).unwrap();
        let boundary = processor.preview(&[]).unwrap();
        processor.commit(boundary).unwrap();
        assert_eq!(processor.state(id).unwrap(), before);
        assert_eq!(
            processor.completion(request).unwrap().unwrap().outcome,
            Outcome::Failed {
                frame: 0,
                reason: TransitionError::Exhausted
            }
        );
    }

    #[shoop_test]
    fn exhausted_request_generation_retires_slot_without_reusing_identity() {
        let (mut processor, id) = processor();
        processor.generations[0] = u32::MAX;
        let request = processor.submit(command(id, Operation::Stop)).unwrap();
        processor.cancel(request).unwrap();
        processor.take_completion(request).unwrap();
        assert_eq!(
            processor.completion(request),
            Err(TransitionError::InvalidId)
        );
        assert_eq!(
            processor.submit(command(id, Operation::Record)),
            Err(TransitionError::Capacity)
        );
    }
}
