//! Lower logical programs to the functional processor's typed representation.
use crate::{
    fixed::{Fixed, Ramp},
    jobs::BufferId,
    state::{Address, Instruction, Program, SyncInstruction, Value},
    transitions::{LoopId, LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan},
};
use alloc::vec::Vec;
use rt_fpga_compiler::{
    CompileError, InitialValue, LogicalStateProgram, Operation, ProgramLimits, Resources,
    StateProgramTarget,
};

pub struct RustTarget<'a> {
    pub limits: ProgramLimits,
    pub media_views: &'a [u64],
}
impl<'a> RustTarget<'a> {
    pub fn new(media_views: &'a [u64]) -> Self {
        Self {
            limits: ProgramLimits::default(),
            media_views,
        }
    }
}
pub struct Binding {
    pub id: LoopId,
    pub gain: Option<Address>,
    pub view: Option<Address>,
    pub meter: Option<BufferId>,
}
pub struct CompiledProgram {
    pub program: Program,
    pub memory: Vec<Value>,
    pub plan: TransitionPlan,
    pub bindings: Vec<Binding>,
    pub resources: Resources,
}
pub struct ProgramImage {
    pub layout: rt_fpga_compiler::loading::Layout,
    pub compiled: CompiledProgram,
    pub initializers: Vec<(Address, Value)>,
    pub new_loops: Vec<Option<crate::state::LoopSlot>>,
}
impl RustTarget<'_> {
    pub fn build_image(
        &self,
        logical: &LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
    ) -> Result<ProgramImage, CompileError> {
        let bound = layout.relocate(logical)?;
        let compiled = self.build(&bound)?;
        let initializers = layout
            .new_values()
            .map(|(_, address)| (Address(address as usize), compiled.memory[address as usize]))
            .collect();
        let states = compiled.plan.initial_states();
        let new_loops = layout
            .new_loops()
            .map(|id| {
                Some(crate::state::LoopSlot::initial(
                    states[id.slot as usize].unwrap(),
                    true,
                ))
            })
            .collect();
        Ok(ProgramImage {
            layout,
            compiled,
            initializers,
            new_loops,
        })
    }
}
fn id(value: rt_fpga_compiler::Identity) -> LoopId {
    LoopId {
        slot: value.slot,
        generation: value.generation,
    }
}
impl StateProgramTarget for RustTarget<'_> {
    type CompiledProgram = CompiledProgram;
    fn limits(&self) -> ProgramLimits {
        ProgramLimits {
            loop_slots: self.limits.loop_slots.min(65536),
            quantum: self.limits.quantum.min(128),
            ..self.limits
        }
    }
    fn build(&self, logical: &LogicalStateProgram) -> Result<CompiledProgram, CompileError> {
        logical.validate(self.limits())?;
        let memory = logical
            .initial_state
            .iter()
            .map(|value| {
                Ok(match *value {
                    InitialValue::MediaView(view) => Value::Word(
                        *self
                            .media_views
                            .get(view as usize)
                            .ok_or(CompileError::MissingView(view))?,
                    ),
                    InitialValue::Ramp(r) => Value::Ramp(Ramp {
                        start: Fixed(r.start),
                        target: Fixed(r.target),
                        duration: r.duration,
                        elapsed: r.elapsed,
                    }),
                    InitialValue::Smoothing { duration, .. } => {
                        Value::Word(u64::from(duration) << 32)
                    }
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        let instructions = logical
            .audio_pass
            .iter()
            .map(|op| match *op {
                Operation::Silence { destination } => Instruction::ProcessSilence {
                    destination: BufferId(destination as usize),
                },
                Operation::Mix {
                    left,
                    right,
                    destination,
                } => Instruction::ProcessMix {
                    left: BufferId(left as usize),
                    right: BufferId(right as usize),
                    destination: BufferId(destination as usize),
                },
                Operation::Input { port, destination } => Instruction::ProcessInput {
                    port: port as usize,
                    destination: BufferId(destination as usize),
                },
                Operation::Playback {
                    loop_id,
                    view,
                    destination,
                } => Instruction::ProcessLogicalPlayback {
                    loop_id: id(loop_id),
                    view: Address(view as usize),
                    destination: BufferId(destination as usize),
                },
                Operation::Capture {
                    loop_id,
                    view,
                    source,
                } => Instruction::ProcessLogicalCapture {
                    loop_id: id(loop_id),
                    view: Address(view as usize),
                    source: BufferId(source as usize),
                },
                Operation::Gain {
                    ramp,
                    source,
                    destination,
                } => Instruction::ProcessGain {
                    ramp: Address(ramp as usize),
                    source: BufferId(source as usize),
                    destination: BufferId(destination as usize),
                },
                Operation::Smoothing {
                    loop_id,
                    control,
                    source,
                } => {
                    let InitialValue::Smoothing {
                        destination,
                        history,
                        ..
                    } = logical.initial_state[control as usize]
                    else {
                        unreachable!()
                    };
                    Instruction::ProcessHistorySmoothing {
                        loop_id: id(loop_id),
                        control: Address(control as usize),
                        source: BufferId(source as usize),
                        destination: BufferId(destination as usize),
                        history: history as usize,
                    }
                }
                Operation::Output { source, port } => Instruction::ProcessOutput {
                    source: BufferId(source as usize),
                    port: port as usize,
                },
            })
            .collect();
        let slots = logical.loops.iter().map(|l| l.id.slot + 1).max().unwrap();
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: logical.quantum,
                loops: logical
                    .loops
                    .iter()
                    .map(|l| LoopSpec {
                        id: id(l.id),
                        kind: LoopKind::Primitive,
                        length: l.length,
                        sync_source: l.sync_source.map(id),
                        repeat_sync: l.repeat_sync,
                        timeline: None,
                    })
                    .collect(),
            },
            TransitionLimits {
                loop_slots: slots,
                events: logical.resources.events,
            },
        )
        .map_err(|_| CompileError::InvalidProgram)?;
        let sync_instructions: Vec<_> = logical
            .sync_pass
            .iter()
            .map(|&v| SyncInstruction::ProcessLoop {
                loop_id: id(v),
                sync_source: logical
                    .loops
                    .iter()
                    .find(|l| l.id == v)
                    .unwrap()
                    .sync_source
                    .map(id),
            })
            .collect();
        let bindings = logical
            .bindings
            .iter()
            .map(|b| Binding {
                id: id(b.id),
                gain: b.gain.map(|a| Address(a as usize)),
                view: b.view.map(|a| Address(a as usize)),
                meter: b.meter.map(|a| BufferId(a as usize)),
            })
            .collect();
        Ok(CompiledProgram {
            program: Program {
                sync_instructions,
                instructions,
                quantum: logical.quantum,
                chunk_frames: logical.chunk_frames,
                jobs_per_quantum: logical.jobs_per_quantum as usize,
            },
            memory,
            plan,
            bindings,
            resources: logical.resources,
        })
    }
}
