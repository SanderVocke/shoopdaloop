//! Graph compilation and target lowering, independent of execution and transport.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod graph;
pub mod loading;
use alloc::{vec, vec::Vec};

pub use shoop_graph::Id as Identity;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Loop {
    pub id: Identity,
    pub length: u64,
    pub sync_source: Option<Identity>,
    pub repeat_sync: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ramp {
    pub start: i32,
    pub target: i32,
    pub duration: u32,
    pub elapsed: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InitialValue {
    MediaView(u32),
    Ramp(Ramp),
    Smoothing {
        destination: u32,
        history: u32,
        duration: u32,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Operation {
    Silence {
        destination: u32,
    },
    Mix {
        left: u32,
        right: u32,
        destination: u32,
    },
    Input {
        port: u32,
        destination: u32,
    },
    Playback {
        loop_id: Identity,
        view: u32,
        destination: u32,
    },
    Capture {
        loop_id: Identity,
        view: u32,
        source: u32,
    },
    Gain {
        ramp: u32,
        source: u32,
        destination: u32,
    },
    Smoothing {
        loop_id: Identity,
        control: u32,
        source: u32,
    },
    Output {
        source: u32,
        port: u32,
    },
}
/// Stable ownership of persistent values, independent of instruction and allocation order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StateKey {
    pub owner: shoop_graph::Owner,
    pub role: StateRole,
    pub channel: u32,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StateRole {
    Gain,
    View,
    Smoothing(u32),
    /// Immutable coefficients receive a new slot when their value changes.
    Coefficient(i32),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StateBinding {
    pub key: StateKey,
    pub address: u32,
}
/// Logical state/buffer slots. Targets return their actual locations in their artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopBinding {
    pub id: Identity,
    pub gain: Option<u32>,
    pub view: Option<u32>,
    pub meter: Option<u32>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Resources {
    pub queue_capacity: u32,
    pub buffers: u32,
    pub history_slots: u32,
    pub input_ports: u32,
    pub output_ports: u32,
    pub events: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogicalStateProgram {
    pub quantum: u32,
    pub chunk_frames: u32,
    pub sync_pass: Vec<Identity>,
    pub audio_pass: Vec<Operation>,
    pub initial_state: Vec<InitialValue>,
    pub state_bindings: Vec<StateBinding>,
    pub loops: Vec<Loop>,
    pub bindings: Vec<LoopBinding>,
    pub jobs_per_quantum: u32,
    pub resources: Resources,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramLimits {
    pub instructions: u32,
    pub state_values: u32,
    pub loop_slots: u32,
    pub jobs: u32,
    pub buffers: u32,
    pub history_slots: u32,
    pub input_ports: u32,
    pub output_ports: u32,
    pub quantum: u32,
}
impl Default for ProgramLimits {
    fn default() -> Self {
        Self {
            instructions: 65536,
            state_values: 65536,
            loop_slots: 65536,
            jobs: 65536,
            buffers: 65536,
            history_slots: 65536,
            input_ports: 65536,
            output_ports: 65536,
            quantum: 128,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Resource {
    Instructions,
    State,
    Loops,
    Jobs,
    Buffers,
    History,
    Inputs,
    Outputs,
    Quantum,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompileError {
    InvalidProgram,
    Capacity(Resource),
    MissingView(u32),
    Unsupported {
        object: shoop_graph::Owner,
        feature: graph::Feature,
    },
}
impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl core::error::Error for CompileError {}

pub trait StateProgramTarget {
    type CompiledProgram;
    fn limits(&self) -> ProgramLimits;
    fn build(&self, program: &LogicalStateProgram) -> Result<Self::CompiledProgram, CompileError>;
}

/// Fixed fixtures retained for direct target and loader tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FixedProgram {
    MonoLoops { quantum: u32, loops: u32 },
    SyncSmoothing { quantum: u32 },
}
pub fn compile<T: StateProgramTarget>(
    fixed: FixedProgram,
    target: &T,
) -> Result<T::CompiledProgram, CompileError> {
    let program = fixed.emit()?;
    program.validate(target.limits())?;
    target.build(&program)
}
impl FixedProgram {
    pub fn emit(self) -> Result<LogicalStateProgram, CompileError> {
        let quantum = match self {
            Self::MonoLoops { quantum, .. } | Self::SyncSmoothing { quantum } => quantum,
        };
        if !matches!(quantum, 1 | 16 | 32 | 64 | 128) {
            return Err(CompileError::InvalidProgram);
        }
        let program = match self {
            Self::MonoLoops { loops: count, .. } => {
                if !matches!(count, 1 | 2) {
                    return Err(CompileError::InvalidProgram);
                }
                let id = |slot| Identity {
                    slot,
                    generation: 1,
                };
                let mut initial_state = vec![
                    InitialValue::Ramp(Ramp {
                        start: 1 << 24,
                        target: 1 << 24,
                        duration: 0,
                        elapsed: 0
                    });
                    count as usize
                ];
                let mut audio_pass = vec![Operation::Input {
                    port: 0,
                    destination: 0,
                }];
                let mut bindings = Vec::new();
                for i in 0..count {
                    let view = count + i;
                    let playback = 1 + 2 * i;
                    let output = 2 + 2 * i;
                    initial_state.push(InitialValue::MediaView(i));
                    audio_pass.extend([
                        Operation::Playback {
                            loop_id: id(i),
                            view,
                            destination: playback,
                        },
                        Operation::Gain {
                            ramp: i,
                            source: playback,
                            destination: output,
                        },
                        Operation::Output {
                            source: output,
                            port: i,
                        },
                        Operation::Capture {
                            loop_id: id(i),
                            view,
                            source: 0,
                        },
                    ]);
                    bindings.push(LoopBinding {
                        id: id(i),
                        gain: Some(i),
                        view: Some(view),
                        meter: Some(output),
                    });
                }
                LogicalStateProgram {
                    quantum,
                    chunk_frames: 1,
                    sync_pass: (0..count).map(id).collect(),
                    audio_pass,
                    initial_state,
                    state_bindings: vec![],
                    loops: (0..count)
                        .map(|i| Loop {
                            id: id(i),
                            length: 0,
                            sync_source: (i > 0).then(|| id(0)),
                            repeat_sync: false,
                        })
                        .collect(),
                    bindings,
                    jobs_per_quantum: 1 + 5 * count,
                    resources: Resources {
                        queue_capacity: 1 + 5 * count,
                        buffers: 1 + 2 * count,
                        history_slots: 1 + 2 * count,
                        input_ports: 1,
                        output_ports: count,
                        events: 4 * count,
                    },
                }
            }
            Self::SyncSmoothing { .. } => {
                let id = Identity {
                    slot: 0,
                    generation: 7,
                };
                LogicalStateProgram {
                    quantum,
                    chunk_frames: 1,
                    sync_pass: vec![id],
                    audio_pass: vec![
                        Operation::Input {
                            port: 0,
                            destination: 0,
                        },
                        Operation::Gain {
                            ramp: 1,
                            source: 0,
                            destination: 1,
                        },
                        Operation::Smoothing {
                            loop_id: id,
                            control: 0,
                            source: 1,
                        },
                        Operation::Output { source: 2, port: 0 },
                    ],
                    state_bindings: vec![],
                    initial_state: vec![
                        InitialValue::Smoothing {
                            destination: 2,
                            history: 7,
                            duration: 33,
                        },
                        InitialValue::Ramp(Ramp {
                            start: i32::MAX,
                            target: i32::MIN,
                            duration: 47,
                            elapsed: 0,
                        }),
                    ],
                    loops: vec![Loop {
                        id,
                        length: 4 * u64::from(quantum),
                        sync_source: None,
                        repeat_sync: false,
                    }],
                    bindings: vec![LoopBinding {
                        id,
                        gain: Some(1),
                        view: None,
                        meter: Some(2),
                    }],
                    jobs_per_quantum: 8,
                    resources: Resources {
                        queue_capacity: 32,
                        buffers: 8,
                        history_slots: 16,
                        input_ports: 1,
                        output_ports: 1,
                        events: 6,
                    },
                }
            }
        };
        program.validate(ProgramLimits::default())?;
        Ok(program)
    }
}
impl LogicalStateProgram {
    /// Logical identities are names, not physical state-memory addresses.
    pub fn validate_logical(&self, limits: ProgramLimits) -> Result<(), CompileError> {
        if self.loops.len() as u64 > u64::from(limits.loop_slots) {
            return Err(CompileError::Capacity(Resource::Loops));
        }
        if !self.state_bindings.is_empty() && self.state_bindings.len() != self.initial_state.len()
        {
            return Err(CompileError::InvalidProgram);
        }
        self.validate(ProgramLimits {
            loop_slots: u32::MAX,
            ..limits
        })
    }
    pub fn validate(&self, limits: ProgramLimits) -> Result<(), CompileError> {
        use CompileError::{Capacity, InvalidProgram};
        let r = self.resources;
        if !matches!(self.quantum, 1 | 16 | 32 | 64 | 128)
            || self.chunk_frames == 0
            || self.quantum % self.chunk_frames != 0
            || self.jobs_per_quantum > r.queue_capacity
            || u64::from(r.events) < 4 * self.loops.len() as u64
            || self.sync_pass.len() != self.loops.len()
            || self.bindings.len() != self.loops.len()
        {
            return Err(InvalidProgram);
        }
        let loop_slots = self
            .loops
            .iter()
            .map(|l| u64::from(l.id.slot) + 1)
            .max()
            .unwrap_or(0);
        for (resource, needed, available) in [
            (
                Resource::Instructions,
                (self.sync_pass.len() + self.audio_pass.len()) as u64,
                limits.instructions,
            ),
            (
                Resource::State,
                self.initial_state.len() as u64,
                limits.state_values,
            ),
            (Resource::Loops, loop_slots, limits.loop_slots),
            (Resource::Jobs, u64::from(r.queue_capacity), limits.jobs),
            (Resource::Buffers, u64::from(r.buffers), limits.buffers),
            (
                Resource::History,
                u64::from(r.history_slots),
                limits.history_slots,
            ),
            (
                Resource::Inputs,
                u64::from(r.input_ports),
                limits.input_ports,
            ),
            (
                Resource::Outputs,
                u64::from(r.output_ports),
                limits.output_ports,
            ),
            (Resource::Quantum, u64::from(self.quantum), limits.quantum),
        ] {
            if needed > u64::from(available) {
                return Err(Capacity(resource));
            }
        }
        for (i, row) in self.loops.iter().enumerate() {
            if row.id.generation == 0
                || self.loops[..i].iter().any(|l| l.id.slot == row.id.slot)
                || row.length % u64::from(self.quantum) != 0
                || self.bindings[i].id != row.id
            {
                return Err(InvalidProgram);
            }
            let position = self
                .sync_pass
                .iter()
                .position(|&id| id == row.id)
                .ok_or(InvalidProgram)?;
            if let Some(source) = row.sync_source.filter(|s| *s != row.id) {
                if self.sync_pass[..position].iter().all(|&id| id != source) {
                    return Err(InvalidProgram);
                }
            }
        }
        let state = |slot: u32| self.initial_state.get(slot as usize).ok_or(InvalidProgram);
        let loop_id = |id| {
            if self.loops.iter().any(|l| l.id == id) {
                Ok(())
            } else {
                Err(InvalidProgram)
            }
        };
        let buffer = |id: u32| {
            if id < r.buffers {
                Ok(())
            } else {
                Err(InvalidProgram)
            }
        };
        for value in &self.initial_state {
            if let InitialValue::Smoothing {
                destination,
                history,
                ..
            } = *value
            {
                buffer(destination)?;
                if history >= r.history_slots {
                    return Err(InvalidProgram);
                }
            }
        }
        for op in &self.audio_pass {
            match *op {
                Operation::Silence { destination } => buffer(destination)?,
                Operation::Mix {
                    left,
                    right,
                    destination,
                } => {
                    buffer(left)?;
                    buffer(right)?;
                    buffer(destination)?;
                }
                Operation::Input { port, destination } => {
                    buffer(destination)?;
                    if port >= r.input_ports {
                        return Err(InvalidProgram);
                    }
                }
                Operation::Playback {
                    loop_id: id,
                    view,
                    destination,
                } => {
                    loop_id(id)?;
                    buffer(destination)?;
                    if !matches!(state(view)?, InitialValue::MediaView(_)) {
                        return Err(InvalidProgram);
                    }
                }
                Operation::Capture {
                    loop_id: id,
                    view,
                    source,
                } => {
                    loop_id(id)?;
                    buffer(source)?;
                    if !matches!(state(view)?, InitialValue::MediaView(_)) {
                        return Err(InvalidProgram);
                    }
                }
                Operation::Gain {
                    ramp,
                    source,
                    destination,
                } => {
                    buffer(source)?;
                    buffer(destination)?;
                    if !matches!(state(ramp)?, InitialValue::Ramp(_)) {
                        return Err(InvalidProgram);
                    }
                }
                Operation::Smoothing {
                    loop_id: id,
                    control,
                    source,
                } => {
                    loop_id(id)?;
                    buffer(source)?;
                    if !matches!(state(control)?, InitialValue::Smoothing { .. }) {
                        return Err(InvalidProgram);
                    }
                }
                Operation::Output { source, port } => {
                    buffer(source)?;
                    if port >= r.output_ports {
                        return Err(InvalidProgram);
                    }
                }
            }
        }
        if !self.state_bindings.is_empty() {
            if self.state_bindings.len() > self.initial_state.len() {
                return Err(InvalidProgram);
            }
            for (i, binding) in self.state_bindings.iter().enumerate() {
                if self.state_bindings[..i]
                    .iter()
                    .any(|b| b.key == binding.key || b.address == binding.address)
                {
                    return Err(InvalidProgram);
                }
                let value = state(binding.address)?;
                if !matches!(
                    (binding.key.role, value),
                    (StateRole::View, InitialValue::MediaView(_))
                        | (
                            StateRole::Gain | StateRole::Coefficient(_),
                            InitialValue::Ramp(_)
                        )
                        | (StateRole::Smoothing(_), InitialValue::Smoothing { .. })
                ) {
                    return Err(InvalidProgram);
                }
            }
        }
        for binding in &self.bindings {
            if let Some(address) = binding.gain {
                if !matches!(state(address)?, InitialValue::Ramp(_)) {
                    return Err(InvalidProgram);
                }
            }
            if let Some(address) = binding.view {
                if !matches!(state(address)?, InitialValue::MediaView(_)) {
                    return Err(InvalidProgram);
                }
            }
            if let Some(id) = binding.meter {
                buffer(id)?;
            }
        }
        Ok(())
    }
}
