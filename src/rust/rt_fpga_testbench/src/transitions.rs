//! Boundary traces shared by the hardware model, native resolver and future RTL.
use rt_fpga_reference::transitions::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shoop_loops as native;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionPlanInput {
    pub description: PlanDescription,
    pub limits: TransitionLimits,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionCase {
    pub name: String,
    pub plan: TransitionPlanInput,
    pub steps: Vec<TransitionStep>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionStep {
    pub collect_completed: bool,
    pub collect: Vec<RequestId>,
    pub cancel: Vec<RequestId>,
    pub commands: Vec<Command>,
    /// Retries replace the candidate at the same frame; only the last can commit.
    pub previews: Vec<Vec<Observation>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Response<T> {
    Accepted(T),
    Rejected(TransitionError),
}
impl<T> From<Result<T, TransitionError>> for Response<T> {
    fn from(value: Result<T, TransitionError>) -> Self {
        match value {
            Ok(value) => Self::Accepted(value),
            Err(error) => Self::Rejected(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    pub request: RequestId,
    pub result: Response<Option<Completion>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionResult {
    pub frame: u64,
    pub collected: Vec<Collection>,
    pub cancelled: Vec<Response<bool>>,
    pub submitted: Vec<Response<RequestId>>,
    pub previews: Vec<Response<Vec<Decision>>>,
    pub committed: bool,
    pub states: Vec<LoopState>,
    pub requests: Vec<Option<RequestSnapshot>>,
    pub events: Vec<Event>,
    pub after_finish: Vec<LoopState>,
    pub next_frame: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionTrace {
    pub case: TransitionCase,
    pub results: Vec<TransitionResult>,
}

trait Engine {
    fn frame(&self) -> u64;
    fn states(&self) -> Vec<LoopState>;
    fn requests(&self) -> Vec<Option<RequestSnapshot>>;
    fn events(&self) -> Vec<Event>;
    fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError>;
    fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError>;
    fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError>;
    fn preview(&mut self, observations: &[Observation]) -> Result<Vec<Decision>, TransitionError>;
    fn commit(&mut self);
    fn finish(&mut self);
}

fn run(case: TransitionCase, mut engine: impl Engine) -> Result<TransitionTrace, TransitionError> {
    let mut results = Vec::with_capacity(case.steps.len());
    for step in &case.steps {
        if step.previews.is_empty() {
            return Err(TransitionError::InvalidConfig);
        }
        let frame = engine.frame();
        let automatic: Vec<_> = if step.collect_completed {
            engine
                .requests()
                .into_iter()
                .flatten()
                .filter(|r| r.outcome.is_some())
                .map(|r| r.id)
                .collect()
        } else {
            Vec::new()
        };
        let collected = automatic
            .iter()
            .chain(&step.collect)
            .map(|&request| Collection {
                request,
                result: engine.collect(request).into(),
            })
            .collect();
        let cancelled = step
            .cancel
            .iter()
            .map(|&id| engine.cancel(id).into())
            .collect();
        let submitted = step
            .commands
            .iter()
            .map(|&command| engine.submit(command).into())
            .collect();
        let previews: Vec<_> = step
            .previews
            .iter()
            .map(|observations| engine.preview(observations).into())
            .collect();
        let committed = matches!(previews.last(), Some(Response::Accepted(_)));
        if committed {
            engine.commit();
        }
        let states = engine.states();
        let requests = engine.requests();
        let events = engine.events();
        if committed {
            engine.finish();
        }
        results.push(TransitionResult {
            frame,
            collected,
            cancelled,
            submitted,
            previews,
            committed,
            states,
            requests,
            events,
            after_finish: engine.states(),
            next_frame: engine.frame(),
        });
    }
    Ok(TransitionTrace { case, results })
}

struct Model {
    processor: TransitionProcessor,
    boundary: Option<BoundaryId>,
}
impl Engine for Model {
    fn frame(&self) -> u64 {
        self.processor.frame()
    }
    fn states(&self) -> Vec<LoopState> {
        self.processor.states().collect()
    }
    fn requests(&self) -> Vec<Option<RequestSnapshot>> {
        self.processor.request_slots().collect()
    }
    fn events(&self) -> Vec<Event> {
        self.processor.events().to_vec()
    }
    fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError> {
        self.processor.submit(command)
    }
    fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError> {
        self.processor.cancel(id)
    }
    fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        self.processor.take_completion(id)
    }
    fn preview(&mut self, observations: &[Observation]) -> Result<Vec<Decision>, TransitionError> {
        self.boundary = None;
        let id = self.processor.preview(observations)?;
        self.boundary = Some(id);
        Ok(self.processor.decisions(id)?.to_vec())
    }
    fn commit(&mut self) {
        self.processor
            .commit(self.boundary.expect("successful preview"))
            .expect("commit");
    }
    fn finish(&mut self) {
        self.processor
            .finish_quantum(self.boundary.take().expect("committed boundary"))
            .expect("finish");
    }
}

pub fn run_transition_case(case: TransitionCase) -> Result<TransitionTrace, TransitionError> {
    let plan = TransitionPlan::compile(case.plan.description.clone(), case.plan.limits)?;
    run(
        case,
        Model {
            processor: TransitionProcessor::prepare(plan),
            boundary: None,
        },
    )
}

fn error(error: native::Error) -> TransitionError {
    use native::Error as E;
    match error {
        E::InvalidConfig => TransitionError::InvalidConfig,
        E::Capacity => TransitionError::Capacity,
        E::InvalidId => TransitionError::InvalidId,
        E::DuplicateId => TransitionError::DuplicateId,
        E::InvalidState => TransitionError::InvalidState,
        E::InvalidFrame => TransitionError::InvalidFrame,
        E::MissingSync => TransitionError::MissingSync,
        E::Exhausted => TransitionError::Exhausted,
        E::EmptyContent => TransitionError::EmptyContent,
        E::InvalidProgram | E::InstantaneousDependencyCycle | E::RecursiveProgram => {
            TransitionError::Unsupported
        }
    }
}
fn native_error(value: TransitionError) -> native::Error {
    match value {
        TransitionError::InvalidConfig => native::Error::InvalidConfig,
        TransitionError::Capacity => native::Error::Capacity,
        TransitionError::InvalidId => native::Error::InvalidId,
        TransitionError::DuplicateId => native::Error::DuplicateId,
        TransitionError::InvalidState => native::Error::InvalidState,
        TransitionError::InvalidFrame => native::Error::InvalidFrame,
        TransitionError::MissingSync => native::Error::MissingSync,
        TransitionError::Exhausted => native::Error::Exhausted,
        TransitionError::EmptyContent => native::Error::EmptyContent,
        TransitionError::Unsupported => native::Error::InvalidProgram,
    }
}
fn loop_id(id: native::LoopId) -> LoopId {
    LoopId {
        slot: id.slot(),
        generation: id.generation(),
    }
}
fn request_id(id: native::RequestId) -> RequestId {
    RequestId {
        slot: id.slot(),
        generation: id.generation(),
    }
}
fn native_loop(id: LoopId) -> Result<native::LoopId, TransitionError> {
    native::LoopId::from_parts(id.slot, id.generation).map_err(error)
}
fn native_request(id: RequestId) -> Result<native::RequestId, TransitionError> {
    native::RequestId::from_parts(id.slot, id.generation).map_err(error)
}
fn native_operation(operation: Operation) -> native::Transition {
    match operation {
        Operation::Stop => native::Transition::Stop,
        Operation::Play {
            position,
            retrigger,
        } => native::Transition::Play {
            position,
            retrigger,
        },
        Operation::Record => native::Transition::Record {
            monitor_previous: false,
        },
        Operation::Seek {
            position,
            retrigger,
        } => native::Transition::Seek {
            position,
            retrigger,
        },
    }
}
fn operation(operation: native::Transition) -> Operation {
    match operation {
        native::Transition::Stop => Operation::Stop,
        native::Transition::Play {
            position,
            retrigger,
        } => Operation::Play {
            position,
            retrigger,
        },
        native::Transition::Record {
            monitor_previous: false,
        } => Operation::Record,
        native::Transition::Seek {
            position,
            retrigger,
        } => Operation::Seek {
            position,
            retrigger,
        },
        _ => panic!("operation outside primitive oracle profile"),
    }
}
fn state(state: native::State) -> LoopState {
    LoopState {
        id: loop_id(state.id),
        mode: match state.mode {
            native::Mode::Stopped => Mode::Stopped,
            native::Mode::Playing => Mode::Playing,
            native::Mode::Recording => Mode::Recording,
            _ => panic!("mode outside primitive oracle profile"),
        },
        content_length: state.content_length,
        position: state.position,
        capture_frames: state.capture_frames,
        cycles: state.cycles,
        content_revision: state.content_revision.0,
        capture_revision: state.capture_revision.0,
        revision_watermark: state.content_revision.0.max(state.capture_revision.0),
        last_pulse: state.last_pulse.map(|frame| frame.0),
        repeat_sync: state.repeat_sync,
    }
}
fn outcome(value: native::Outcome) -> Outcome {
    match value {
        native::Outcome::Applied { frame } => Outcome::Applied { frame: frame.0 },
        native::Outcome::Superseded { frame } => Outcome::Superseded { frame: frame.0 },
        native::Outcome::Cancelled { frame } => Outcome::Cancelled { frame: frame.0 },
        native::Outcome::Failed { frame, reason } => Outcome::Failed {
            frame: frame.0,
            reason: error(reason),
        },
    }
}
fn completion(value: native::Completion) -> Completion {
    Completion {
        id: request_id(value.id),
        sequence: value.sequence.0,
        requested: value.requested.0,
        effective: value.effective.0,
        outcome: outcome(value.outcome),
    }
}

struct Oracle {
    resolver: native::Resolver,
    ids: Vec<native::LoopId>,
    retained: Vec<Option<(RequestId, Command, u64)>>,
    preparing: bool,
}
impl Engine for Oracle {
    fn frame(&self) -> u64 {
        self.resolver.frame().0
    }
    fn states(&self) -> Vec<LoopState> {
        self.ids
            .iter()
            .map(|&id| state(self.resolver.state(id).expect("prepared loop")))
            .collect()
    }
    fn requests(&self) -> Vec<Option<RequestSnapshot>> {
        self.retained
            .iter()
            .map(|record| {
                record.map(|(id, command, effective)| {
                    let terminal = self
                        .resolver
                        .completion(native_request(id).expect("accepted identity"))
                        .expect("retained request")
                        .map(|c| outcome(c.outcome));
                    let pending = self
                        .resolver
                        .pending_transitions()
                        .find(|p| p.request.sequence.0 == command.sequence);
                    RequestSnapshot {
                        id,
                        command,
                        effective,
                        outcome: terminal,
                        remaining_ticks: pending.and_then(|p| p.remaining_ticks),
                    }
                })
            })
            .collect()
    }
    fn events(&self) -> Vec<Event> {
        assert_eq!(
            self.resolver.dropped_events(),
            0,
            "oracle journal must be complete"
        );
        self.resolver
            .events()
            .iter()
            .map(|e| Event {
                frame: e.frame.0,
                target: loop_id(e.target),
                kind: match e.kind {
                    native::EventKind::Transition(op) => EventKind::Transition(operation(op)),
                    native::EventKind::Wrap => EventKind::Wrap,
                    native::EventKind::Pulse => EventKind::Pulse,
                    _ => panic!("event outside primitive oracle profile"),
                },
            })
            .collect()
    }
    fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError> {
        if self.preparing {
            return Err(TransitionError::InvalidState);
        }
        let request = native::Request {
            target: native_loop(command.target)?,
            sequence: native::Sequence(command.sequence),
            at: native::Frame(command.at),
            timing: match command.timing {
                Timing::At => native::Timing::At,
                Timing::AfterTicks { source, skip } => native::Timing::AfterTicks {
                    source: native_loop(source)?,
                    skip,
                },
            },
            transition: native_operation(command.operation),
        };
        let id = request_id(self.resolver.submit(request).map_err(error)?);
        let effective = self
            .resolver
            .pending_transitions()
            .find(|p| p.request.sequence.0 == command.sequence)
            .expect("submitted request")
            .effective
            .0;
        self.retained[id.slot as usize] = Some((id, command, effective));
        Ok(id)
    }
    fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError> {
        if self.preparing {
            return Err(TransitionError::InvalidState);
        }
        self.resolver
            .cancel(native_request(id)?)
            .map(|result| result == native::CancelResult::Cancelled)
            .map_err(error)
    }
    fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        if self.preparing {
            return Err(TransitionError::InvalidState);
        }
        let result = self
            .resolver
            .take_completion(native_request(id)?)
            .map_err(error)?;
        if result.is_some() {
            self.retained[id.slot as usize] = None;
        }
        Ok(result.map(completion))
    }
    fn preview(&mut self, observations: &[Observation]) -> Result<Vec<Decision>, TransitionError> {
        // The hardware transport freezes control input across preflight retries.
        self.preparing = true;
        let observations: Result<Vec<_>, _> = observations
            .iter()
            .map(|o| {
                Ok(match *o {
                    Observation::CaptureUnavailable { target, reason } => {
                        native::Observation::CaptureUnavailable {
                            target: native_loop(target)?,
                            reason: native_error(reason),
                        }
                    }
                    Observation::Refused { request, reason } => native::Observation::Refused {
                        request: native_request(request)?,
                        reason: native_error(reason),
                    },
                })
            })
            .collect();
        self.resolver.preview_tick(&observations?).map_err(error)?;
        Ok(self
            .resolver
            .decisions()
            .map_err(error)?
            .iter()
            .map(|d| {
                let state = state(d.state);
                Decision {
                    state,
                    playback_discontinuity: d.playback_discontinuity,
                    origin: d.origin.map(request_id),
                    waiting_repeat: state.at_end(),
                }
            })
            .collect())
    }
    fn commit(&mut self) {
        self.resolver.commit().expect("successful preview");
    }
    fn finish(&mut self) {
        self.resolver.finish().expect("committed boundary");
        self.preparing = false;
    }
}

/// Independent musical oracle. The hardware profile deliberately excludes
/// composites, media ownership, program replacement, batches and capture monitoring.
pub fn run_transition_oracle(case: TransitionCase) -> Result<TransitionTrace, TransitionError> {
    let plan = TransitionPlan::compile(case.plan.description.clone(), case.plan.limits)?;
    let limits = case.plan.limits;
    let topology = native::TopologyDescription {
        loops: plan
            .image()
            .description
            .loops
            .iter()
            .map(|s| {
                Ok(native::LoopSpec {
                    id: native_loop(s.id)?,
                    kind: native::LoopKind::Primitive,
                    length: s.length,
                    sync_source: s.sync_source.map(native_loop).transpose()?,
                })
            })
            .collect::<Result<_, TransitionError>>()?,
        triggers: Vec::new(),
        programs: Vec::new(),
    };
    let prepared = native::compile(
        native::Limits {
            loop_slots: limits.loop_slots as usize,
            relations: limits.loop_slots as usize,
            program_entries: 1,
            program_segments: 1,
            pending_requests: limits.pending_requests as usize,
            journal_events: limits.events as usize,
        },
        &topology,
    )
    .map_err(error)?;
    let components: Vec<Vec<_>> = prepared
        .components()
        .iter()
        .map(|c| c.iter().copied().map(loop_id).collect())
        .collect();
    assert_eq!(components, plan.image().components, "compiled timing order");
    let mut resolver = native::Resolver::prepare(
        prepared,
        rt_runtime::Quantum::new(case.plan.description.quantum as usize)
            .map_err(|_| TransitionError::InvalidConfig)?,
    )
    .map_err(error)?;
    for spec in &plan.image().description.loops {
        resolver
            .set_repeat_sync(native_loop(spec.id)?, spec.repeat_sync)
            .map_err(error)?;
    }
    let ids = topology.loops.iter().map(|s| s.id).collect();
    run(
        case,
        Oracle {
            resolver,
            ids,
            retained: vec![None; limits.pending_requests as usize],
            preparing: false,
        },
    )
}
