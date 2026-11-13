//! Standalone RTL transport observations backed only by the production resolver.
mod types;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shoop_loops as native;
pub use types::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionPlanInput {
    pub description: PlanDescription,
    pub limits: TransitionLimits,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionCase {
    pub format_version: u32,
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
    pub processed: Response<()>,
    pub states: Vec<LoopState>,
    pub requests: Vec<Option<RequestSnapshot>>,
    pub events: Vec<Event>,
    pub next_frame: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionTrace {
    pub format_version: u32,
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
    fn process(&mut self) -> Result<(), TransitionError>;
}

fn run(case: TransitionCase, mut engine: impl Engine) -> Result<TransitionTrace, TransitionError> {
    let mut results = Vec::with_capacity(case.steps.len());
    for step in &case.steps {
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
        let processed = engine.process().into();
        let states = engine.states();
        let requests = engine.requests();
        let events = engine.events();
        results.push(TransitionResult {
            frame,
            collected,
            cancelled,
            submitted,
            processed,
            states,
            requests,
            events,
            next_frame: engine.frame(),
        });
    }
    Ok(TransitionTrace {
        format_version: crate::spec::FORMAT_VERSION,
        case,
        results,
    })
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
    fault: Option<TransitionError>,
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
        if self.fault.is_some() {
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
        if self.fault.is_some() {
            return Err(TransitionError::InvalidState);
        }
        self.resolver
            .cancel(native_request(id)?)
            .map(|result| result == native::CancelResult::Cancelled)
            .map_err(error)
    }
    fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        if self.fault.is_some() {
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
    fn process(&mut self) -> Result<(), TransitionError> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        let result = (|| {
            self.resolver.preview_tick(&[]).map_err(error)?;
            self.resolver.commit().map_err(error)?;
            self.resolver.finish().map_err(error)
        })();
        if let Err(error) = result {
            self.fault = Some(error);
        }
        result
    }
}

/// Independent musical oracle. The hardware profile deliberately excludes
/// composites, media ownership, program replacement, batches and capture monitoring.
pub fn run_transition_oracle(case: TransitionCase) -> Result<TransitionTrace, TransitionError> {
    if case.format_version != crate::spec::FORMAT_VERSION {
        return Err(TransitionError::InvalidConfig);
    }
    let plan = compile(&case.plan)?;
    let limits = case.plan.limits;
    let topology = native::TopologyDescription {
        loops: plan
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
    let dependency_order: Vec<_> = prepared
        .dependency_order()
        .iter()
        .map(|id| loop_id(*id))
        .collect();
    assert_eq!(
        dependency_order, plan.dependency_order,
        "compiled timing order"
    );
    let mut resolver = native::Resolver::prepare(
        prepared,
        rt_runtime::Quantum::new(case.plan.description.quantum as usize)
            .map_err(|_| TransitionError::InvalidConfig)?,
    )
    .map_err(error)?;
    for spec in &plan.description.loops {
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
            fault: None,
        },
    )
}

/// Export only the production compiler's canonical timing order.
pub fn compile(input: &TransitionPlanInput) -> Result<TransitionPlanImage, TransitionError> {
    let mut description = input.description.clone();
    let limits = input.limits;
    if !matches!(description.quantum, 1 | 16 | 32 | 64 | 128)
        || limits.loop_slots == 0
        || limits.loop_slots > 65536
        || limits.pending_requests == 0
        || limits.pending_requests > 65536
        || limits.events < 2 * limits.loop_slots + limits.pending_requests
    {
        return Err(TransitionError::InvalidConfig);
    }
    if description
        .loops
        .iter()
        .any(|s| s.kind != LoopKind::Primitive || s.timeline.is_some())
    {
        return Err(TransitionError::Unsupported);
    }
    description.loops.sort_by_key(|s| s.id);
    let topology = native::TopologyDescription {
        loops: description
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
    Ok(TransitionPlanImage {
        format_version: crate::spec::FORMAT_VERSION,
        description,
        limits,
        dependency_order: prepared
            .dependency_order()
            .iter()
            .map(|id| loop_id(*id))
            .collect(),
    })
}
