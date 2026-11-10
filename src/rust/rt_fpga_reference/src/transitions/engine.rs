use super::index::{PendingRequests, SlotMask};
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
    ready: SlotMask,
    pending: PendingRequests,
    absolute_to_visit: SlotMask,
    delivered: Vec<bool>,
    masked: Vec<bool>,
    eligible: Vec<bool>,
    capture_refused: Vec<Option<TransitionError>>,
    request_refused: Vec<Option<TransitionError>>,
    decisions: Vec<Decision>,
    events: Vec<Event>,
    #[cfg(test)]
    work: Work,
}

#[cfg(test)]
#[derive(Debug, Default, Eq, PartialEq)]
struct Work {
    requests: usize,
    followers: usize,
    pulses: usize,
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
                ready: SlotMask::new(n),
                pending: PendingRequests::new(n, p),
                absolute_to_visit: SlotMask::new(p),
                delivered: vec![false; n],
                masked: vec![false; n],
                eligible: vec![false; p],
                capture_refused: vec![None; n],
                request_refused: vec![None; p],
                decisions: Vec::with_capacity(plan.image.description.loops.len()),
                events: Vec::with_capacity(limits.events as usize),
                #[cfg(test)]
                work: Work::default(),
            },
            plan,
            states,
            records: vec![None; p],
            pending: PendingRequests::new(n, p),
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
        self.scratch.capture_refused.fill(None);
        self.scratch.request_refused.fill(None);
        for observation in observations {
            match *observation {
                Observation::CaptureUnavailable { target, reason } => {
                    self.spec(target)?;
                    if self.scratch.capture_refused[target.slot as usize]
                        .replace(reason)
                        .is_some()
                    {
                        return Err(TransitionError::DuplicateId);
                    }
                }
                Observation::Refused { request, reason } => {
                    if self.record(request)?.outcome.is_some() {
                        return Err(TransitionError::InvalidState);
                    }
                    if self.scratch.request_refused[request.slot as usize]
                        .replace(reason)
                        .is_some()
                    {
                        return Err(TransitionError::DuplicateId);
                    }
                }
            }
        }
        self.scratch.states.copy_from_slice(&self.states);
        self.scratch.records.copy_from_slice(&self.records);
        self.scratch.pending.copy_from(&self.pending);
        self.scratch
            .absolute_to_visit
            .copy_from(&self.pending.absolute);
        self.scratch.intents.fill(None);
        self.scratch.protected.fill(None);
        self.scratch.origins.fill(None);
        self.scratch.discontinuities.fill(false);
        self.scratch.ready.clear();
        self.scratch.delivered.fill(false);
        self.scratch.masked.fill(false);
        self.scratch.eligible.fill(false);
        self.scratch.events.clear();
        self.scratch.decisions.clear();
        #[cfg(test)]
        {
            self.scratch.work = Work::default();
        }
        while let Some(i) = self.scratch.absolute_to_visit.pop_first() {
            #[cfg(test)]
            {
                self.scratch.work.requests += 1;
            }
            if self.scratch.records[i]
                .is_some_and(|r| r.outcome.is_none() && r.effective <= self.frame)
            {
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
                let id = self.scratch.states[slot]
                    .ok_or(TransitionError::InvalidId)?
                    .id;
                self.scratch.delivered[slot] = true;
                #[cfg(test)]
                {
                    self.scratch.work.pulses += 1;
                }
                self.scratch.states[slot]
                    .as_mut()
                    .ok_or(TransitionError::InvalidId)?
                    .last_pulse = Some(self.frame);
                self.event(id, EventKind::Pulse)?;
                for i in self.plan.follower_ranges[slot].clone() {
                    let follower = self.plan.followers[i];
                    #[cfg(test)]
                    {
                        self.scratch.work.followers += 1;
                    }
                    if self.plan.component_of[follower.slot as usize] == c {
                        if !self.scratch.masked[follower.slot as usize] {
                            self.queue_pulse(follower);
                        }
                        self.react(follower, id)?;
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
    fn queue_pulse(&mut self, id: LoopId) {
        let slot = id.slot as usize;
        if !self.scratch.delivered[slot] {
            self.scratch.ready.insert(slot);
        }
    }
    fn terminal(&mut self, request: usize, outcome: Outcome) {
        let record = self.scratch.records[request]
            .as_mut()
            .expect("retained request");
        debug_assert!(record.outcome.is_none());
        record.outcome = Some(outcome);
        let absolute = record.command.timing == Timing::At;
        self.scratch
            .pending
            .remove(record.command.target.slot as usize, request, absolute);
        if absolute {
            self.scratch.absolute_to_visit.remove(request);
        }
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
        let record = self.scratch.records[request].expect("retained request");
        if self.scratch.protected[slot].is_some_and(|seq| record.command.sequence <= seq) {
            self.terminal(request, Outcome::Superseded { frame: self.frame });
            return Ok(());
        }
        self.scratch.protected[slot] = Some(record.command.sequence);
        let refused = self.scratch.request_refused[request].or_else(|| {
            (record.command.operation == Operation::Record)
                .then_some(self.scratch.capture_refused[slot])
                .flatten()
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
                self.terminal(
                    request,
                    Outcome::Failed {
                        frame: self.frame,
                        reason,
                    },
                );
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
                self.terminal(request, Outcome::Applied { frame: self.frame });
                if before != state {
                    self.event(id, EventKind::Transition(record.command.operation))?;
                }
                if record.command.operation == Operation::Stop {
                    self.scratch.masked[slot] = true;
                    self.scratch.ready.remove(slot);
                    for absolute in [true, false] {
                        let mut next = self.scratch.pending.first(slot, absolute);
                        while let Some(i) = next {
                            next = self.scratch.pending.next(i);
                            #[cfg(test)]
                            {
                                self.scratch.work.requests += 1;
                            }
                            let old = self.scratch.records[i].expect("indexed pending request");
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
        self.queue_pulse(id);
        self.event(id, EventKind::Wrap)
    }
    fn react(&mut self, target: LoopId, source: LoopId) -> Result<(), TransitionError> {
        let mut next = self.scratch.pending.first(target.slot as usize, false);
        while let Some(i) = next {
            next = self.scratch.pending.next(i);
            #[cfg(test)]
            {
                self.scratch.work.requests += 1;
            }
            let record = self.scratch.records[i].expect("indexed pending request");
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
        self.apply_pending(target)?;
        self.natural_boundary(target)
    }
    pub fn commit(&mut self, id: BoundaryId) -> Result<(), TransitionError> {
        if self.phase != Phase::Preview(id) {
            return Err(TransitionError::InvalidState);
        }
        core::mem::swap(&mut self.states, &mut self.scratch.states);
        core::mem::swap(&mut self.records, &mut self.scratch.records);
        core::mem::swap(&mut self.pending, &mut self.scratch.pending);
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

    #[shoop_test]
    fn empty_request_tables_and_sync_cycles_do_work_proportional_to_edges() {
        let n = 64;
        for requests in [64, 1024] {
            let plan = TransitionPlan::compile(
                PlanDescription {
                    quantum: 16,
                    loops: (0..n)
                        .map(|slot| LoopSpec {
                            id: LoopId {
                                slot,
                                generation: 1,
                            },
                            kind: LoopKind::Primitive,
                            length: 16,
                            sync_source: Some(LoopId {
                                slot: (slot + 1) % n,
                                generation: 1,
                            }),
                            repeat_sync: false,
                            timeline: None,
                        })
                        .collect(),
                },
                TransitionLimits {
                    loop_slots: n,
                    pending_requests: requests,
                    events: 2 * n + requests,
                },
            )
            .unwrap();
            let mut processor = TransitionProcessor::prepare(plan);
            for slot in 0..n {
                processor
                    .submit(Command {
                        sequence: u64::from(slot),
                        ..command(
                            LoopId {
                                slot,
                                generation: 1,
                            },
                            Operation::Play {
                                position: 0,
                                retrigger: true,
                            },
                        )
                    })
                    .unwrap();
            }
            let boundary = processor.preview(&[]).unwrap();
            assert_eq!(
                processor.scratch.work,
                Work {
                    requests: 64,
                    followers: 64,
                    pulses: 64
                }
            );
            processor.commit(boundary).unwrap();
            processor.finish_quantum(boundary).unwrap();
            // Retained terminal records and spare slots must add no request searches.
            let boundary = processor.preview(&[]).unwrap();
            assert_eq!(
                processor.scratch.work,
                Work {
                    requests: 0,
                    followers: 64,
                    pulses: 64
                }
            );
            assert!(processor
                .decisions(boundary)
                .unwrap()
                .iter()
                .all(|d| d.state.cycles == 1));
        }
    }
}
