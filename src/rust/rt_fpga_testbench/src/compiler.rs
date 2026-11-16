//! Lower logical programs to packed RTL instructions and configuration RAM images.
use crate::{bits::Bits, component};
use rt_fpga_compiler::{
    CompileError, Identity, InitialValue, LogicalStateProgram, Operation, ProgramLimits, Resource,
    Resources, StateProgramTarget,
};

pub struct RtlTarget<'a> {
    pub capacities: ProgramLimits,
    pub media_views: &'a [u64],
    pub state_base: u16,
}
impl<'a> RtlTarget<'a> {
    pub fn new(capacities: [u64; 7], loop_slots: u32, media_views: &'a [u64]) -> Self {
        let count = |n: u64| n.min(65535) as u32;
        Self {
            capacities: ProgramLimits {
                instructions: count(capacities[0]),
                state_values: count(capacities[1]),
                jobs: count(capacities[2]),
                quantum: capacities[3].min(128) as u32,
                buffers: count(capacities[4]),
                history_slots: count(capacities[4]),
                input_ports: count(capacities[5]),
                output_ports: count(capacities[6]),
                loop_slots: loop_slots.min(4096),
            },
            media_views,
            state_base: 0,
        }
    }
    fn address(&self, logical: u32) -> Result<u16, CompileError> {
        u16::try_from(
            u32::from(self.state_base)
                .checked_add(logical)
                .ok_or(CompileError::Capacity(Resource::State))?,
        )
        .map_err(|_| CompileError::Capacity(Resource::State))
    }
}
pub struct Binding {
    pub id: Identity,
    pub gain: Option<u16>,
    pub view: Option<u16>,
    pub meter: Option<u16>,
}
pub struct LoopImage {
    pub id: Identity,
    pub words: [u64; 16],
}
pub struct CompiledProgram {
    pub instructions: Vec<Bits>,
    pub state: Vec<Bits>,
    pub loops: Vec<LoopImage>,
    pub bindings: Vec<Binding>,
    pub quantum: u32,
    pub chunk_frames: u32,
    pub sync_count: usize,
    pub jobs_per_quantum: u32,
    pub resources: Resources,
}
fn id(value: Identity) -> component::Identity {
    component::Identity {
        slot: value.slot,
        generation: value.generation,
    }
}
fn operand(value: u32) -> Result<u16, CompileError> {
    u16::try_from(value).map_err(|_| CompileError::InvalidProgram)
}
impl StateProgramTarget for RtlTarget<'_> {
    type CompiledProgram = CompiledProgram;
    fn limits(&self) -> ProgramLimits {
        ProgramLimits {
            instructions: self.capacities.instructions.min(65535),
            state_values: self.capacities.state_values.min(65535),
            jobs: self.capacities.jobs.min(65535),
            buffers: self.capacities.buffers.min(65535),
            history_slots: self.capacities.history_slots.min(65535),
            input_ports: self.capacities.input_ports.min(65535),
            output_ports: self.capacities.output_ports.min(65535),
            loop_slots: self.capacities.loop_slots.min(4096),
            quantum: self.capacities.quantum.min(128),
        }
    }
    fn build(&self, logical: &LogicalStateProgram) -> Result<CompiledProgram, CompileError> {
        logical.validate(self.limits())?;
        if usize::from(self.state_base) + logical.initial_state.len()
            > self.limits().state_values as usize
        {
            return Err(CompileError::Capacity(Resource::State));
        }
        let mut state = vec![Bits::zero(129); usize::from(self.state_base)];
        for value in &logical.initial_state {
            state.push(
                Bits::from(component::value_words(match *value {
                    InitialValue::MediaView(view) => component::Value::Word(
                        *self
                            .media_views
                            .get(view as usize)
                            .ok_or(CompileError::MissingView(view))?,
                    ),
                    InitialValue::Ramp(r) => component::Value::Ramp(component::Ramp {
                        start: r.start,
                        target: r.target,
                        duration: r.duration,
                        elapsed: r.elapsed,
                    }),
                    InitialValue::Smoothing { duration, .. } => {
                        component::Value::Word(u64::from(duration) << 32)
                    }
                }))
                .resized(129),
            );
        }
        let mut instructions = Vec::new();
        for loop_id in &logical.sync_pass {
            let mut bits = Bits::zero(96);
            bits.set(0, 8, 12)
                .set(8, 16, u64::from(operand(loop_id.slot)?))
                .set(24, 32, u64::from(loop_id.generation))
                .set(
                    56,
                    16,
                    u64::from(
                        logical
                            .loops
                            .iter()
                            .find(|l| l.id == *loop_id)
                            .unwrap()
                            .sync_source
                            .map_or(u16::MAX, |s| s.slot as u16),
                    ),
                )
                .set(72, 1, 1);
            instructions.push(bits);
        }
        for op in &logical.audio_pass {
            let instruction = match *op {
                Operation::Silence { destination } => component::Instruction::ProcessSilence {
                    destination: operand(destination)?,
                },
                Operation::Mix {
                    left,
                    right,
                    destination,
                } => component::Instruction::ProcessMix {
                    left: operand(left)?,
                    right: operand(right)?,
                    destination: operand(destination)?,
                },
                Operation::Input { port, destination } => component::Instruction::ProcessInput {
                    port: operand(port)?,
                    destination: operand(destination)?,
                },
                Operation::Playback {
                    loop_id,
                    view,
                    destination,
                } => component::Instruction::ProcessLogicalPlayback {
                    loop_id: id(loop_id),
                    view: self.address(view)?,
                    destination: operand(destination)?,
                },
                Operation::Capture {
                    loop_id,
                    view,
                    source,
                } => component::Instruction::ProcessLogicalCapture {
                    loop_id: id(loop_id),
                    view: self.address(view)?,
                    source: operand(source)?,
                },
                Operation::Gain {
                    ramp,
                    source,
                    destination,
                } => component::Instruction::ProcessGain {
                    ramp: self.address(ramp)?,
                    source: operand(source)?,
                    destination: operand(destination)?,
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
                    component::Instruction::ProcessHistorySmoothing {
                        loop_id: id(loop_id),
                        control: self.address(control)?,
                        source: operand(source)?,
                        destination: operand(destination)?,
                        history: operand(history)?,
                    }
                }
                Operation::Output { source, port } => component::Instruction::ProcessOutput {
                    source: operand(source)?,
                    port: operand(port)?,
                },
            };
            instructions.push(
                Bits::from(
                    component::instruction_words(instruction)
                        .map_err(|_| CompileError::InvalidProgram)?,
                )
                .resized(96),
            );
        }
        let loops = logical
            .loops
            .iter()
            .map(|l| {
                let mut words = [0; 16];
                words[0] = 1 | (u64::from(l.id.generation) << 1) | (u64::from(l.repeat_sync) << 35);
                if let Some(source) = l.sync_source {
                    words[0] |= (1 << 36) | (u64::from(source.slot) << 37);
                    words[1] = u64::from(source.generation);
                }
                words[3] = l.length;
                LoopImage { id: l.id, words }
            })
            .collect();
        let bindings = logical
            .bindings
            .iter()
            .map(|b| {
                Ok(Binding {
                    id: b.id,
                    gain: b.gain.map(|a| self.address(a)).transpose()?,
                    view: b.view.map(|a| self.address(a)).transpose()?,
                    meter: b.meter.map(operand).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        Ok(CompiledProgram {
            instructions,
            state,
            loops,
            bindings,
            quantum: logical.quantum,
            chunk_frames: logical.chunk_frames,
            sync_count: logical.sync_pass.len(),
            jobs_per_quantum: logical.jobs_per_quantum,
            resources: logical.resources,
        })
    }
}
