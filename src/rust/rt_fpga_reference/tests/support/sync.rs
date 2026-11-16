//! Direct SP plan writes compared with an independent native resolver.
use rt_fpga_reference::{
    state::{PlannedTransition, Resolution, StateProcessor},
    transitions::*,
};
use shoop_loops as native;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanInput {
    pub description: PlanDescription,
    pub limits: TransitionLimits,
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanWrite {
    pub target: LoopId,
    pub index: usize,
    pub plan: Option<PlannedTransition>,
}
impl PlanWrite {
    pub fn values(&self, state: &StateProcessor) -> [Option<PlannedTransition>; 2] {
        let mut plans = state.loop_state(self.target).unwrap().planned;
        plans[self.index] = self.plan;
        plans
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Snapshot {
    pub frame: u64,
    pub states: Vec<LoopState>,
    pub plans: Vec<(LoopId, [Option<PlannedTransition>; 2])>,
    pub events: Vec<Event>,
    pub resolutions: Vec<Resolution>,
}
impl Snapshot {
    pub fn from_sp(sp: &StateProcessor) -> Self {
        let states: Vec<_> = sp.loop_states().collect();
        let plans = states
            .iter()
            .map(|s| (s.id, sp.loop_state(s.id).unwrap().planned))
            .collect();
        let mut resolutions = sp.resolutions().to_vec();
        resolutions.sort_by_key(|r| (r.target, r.plan_index));
        Self {
            frame: sp.frame(),
            states,
            plans,
            events: sp.events().to_vec(),
            resolutions,
        }
    }
}

pub struct Oracle {
    resolver: native::Resolver,
    specs: Vec<LoopSpec>,
    plans: Vec<[Option<(native::RequestId, PlannedTransition)>; 2]>,
}
impl Oracle {
    pub fn prepare(input: &PlanInput) -> Result<Self, TransitionError> {
        let mut specs = input.description.loops.clone();
        specs.sort_unstable_by_key(|s| s.id);
        let topology = native::TopologyDescription {
            loops: specs
                .iter()
                .map(|s| {
                    Ok(native::LoopSpec {
                        id: native_loop(s.id)?,
                        kind: match s.kind {
                            LoopKind::Primitive => native::LoopKind::Primitive,
                            LoopKind::Composite => native::LoopKind::Composite,
                        },
                        length: s.length,
                        sync_source: s.sync_source.map(native_loop).transpose()?,
                    })
                })
                .collect::<Result<_, TransitionError>>()?,
            triggers: vec![],
            programs: vec![],
        };
        let prepared = native::compile(
            native::Limits {
                loop_slots: input.limits.loop_slots as usize,
                relations: input.limits.loop_slots as usize,
                program_entries: 1,
                program_segments: 1,
                pending_requests: (2 * input.limits.loop_slots) as usize,
                journal_events: input.limits.events as usize,
            },
            &topology,
        )
        .map_err(error)?;
        let mut resolver = native::Resolver::prepare(
            prepared,
            rt_runtime::Quantum::new(input.description.quantum as usize)
                .map_err(|_| TransitionError::InvalidConfig)?,
        )
        .map_err(error)?;
        for spec in &specs {
            resolver
                .set_repeat_sync(native_loop(spec.id)?, spec.repeat_sync)
                .map_err(error)?;
        }
        Ok(Self {
            resolver,
            plans: vec![[None; 2]; specs.len()],
            specs,
        })
    }
    pub fn write(&mut self, write: &PlanWrite) {
        let index = self
            .specs
            .iter()
            .position(|s| s.id == write.target)
            .unwrap();
        if let Some((id, _)) = self.plans[index][write.index].take() {
            self.resolver.cancel(id).unwrap();
            self.resolver.take_completion(id).unwrap().unwrap();
        }
        if let Some(plan) = write.plan {
            let request = native::Request {
                target: native_loop(write.target).unwrap(),
                sequence: native::Sequence(plan.sequence),
                at: native::Frame(plan.effective),
                timing: if plan.on_pulse {
                    native::Timing::AfterTicks {
                        source: native_loop(self.specs[index].sync_source.unwrap()).unwrap(),
                        skip: plan.remaining_ticks,
                    }
                } else {
                    native::Timing::At
                },
                transition: native_operation(plan.operation),
            };
            let id = self.resolver.submit(request).unwrap();
            self.plans[index][write.index] = Some((id, plan));
        }
    }
    pub fn tick(&mut self) -> Snapshot {
        self.resolver.preview_tick(&[]).unwrap();
        self.resolver.commit().unwrap();
        self.resolver.finish().unwrap();
        let mut resolutions = vec![];
        for (index, spec) in self.specs.iter().enumerate() {
            for slot in 0..2 {
                if let Some((id, mut plan)) = self.plans[index][slot] {
                    if let Some(completion) = self.resolver.take_completion(id).unwrap() {
                        resolutions.push(Resolution {
                            target: spec.id,
                            plan_index: slot,
                            outcome: outcome(completion.outcome),
                        });
                        self.plans[index][slot] = None;
                    } else {
                        if let Some(pending) = self
                            .resolver
                            .pending_transitions()
                            .find(|p| p.request.sequence.0 == plan.sequence)
                        {
                            plan.remaining_ticks = pending.remaining_ticks.unwrap_or(0);
                        }
                        self.plans[index][slot] = Some((id, plan));
                    }
                }
            }
        }
        assert_eq!(self.resolver.dropped_events(), 0);
        Snapshot {
            frame: self.resolver.frame().0,
            states: self
                .specs
                .iter()
                .map(|s| state(self.resolver.state(native_loop(s.id).unwrap()).unwrap()))
                .collect(),
            plans: self
                .specs
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, self.plans[i].map(|p| p.map(|(_, plan)| plan))))
                .collect(),
            events: self
                .resolver
                .events()
                .iter()
                .map(|e| Event {
                    frame: e.frame.0,
                    target: loop_id(e.target),
                    kind: match e.kind {
                        native::EventKind::Transition(op) => EventKind::Transition(operation(op)),
                        native::EventKind::Wrap => EventKind::Wrap,
                        native::EventKind::Pulse => EventKind::Pulse,
                        _ => panic!("outside primitive profile"),
                    },
                })
                .collect(),
            resolutions,
        }
    }
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
        E::InstantaneousDependencyCycle => TransitionError::InstantaneousDependencyCycle,
        E::InvalidProgram | E::RecursiveProgram => TransitionError::Unsupported,
    }
}
fn loop_id(id: native::LoopId) -> LoopId {
    LoopId {
        slot: id.slot(),
        generation: id.generation(),
    }
}
fn native_loop(id: LoopId) -> Result<native::LoopId, TransitionError> {
    native::LoopId::from_parts(id.slot, id.generation).map_err(error)
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
