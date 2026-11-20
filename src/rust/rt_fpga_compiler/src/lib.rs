//! Graph compilation and target lowering, independent of execution and transport.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
mod graph;
pub use graph::{compile, compile_with_chunk_frames, CompileContext, Feature};
pub use shoop_graph;
pub mod loading;
use alloc::vec::Vec;

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
/// Legacy schedule for historical fixtures. Current targets use `MidiOperation`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MidiPhase {
    Begin,
    Capture,
    Observe,
    Append,
    Cleanup,
    ClaimEstablished,
    ClaimNew,
    Playback,
    Output,
    Complete,
    Maintain,
}
impl MidiPhase {
    pub const ORDER: [Self; 11] = [
        Self::Begin,
        Self::Capture,
        Self::Observe,
        Self::Append,
        Self::Cleanup,
        Self::ClaimEstablished,
        Self::ClaimNew,
        Self::Playback,
        Self::Output,
        Self::Complete,
        Self::Maintain,
    ];
    pub fn global(self) -> bool {
        matches!(
            self,
            Self::Begin | Self::Observe | Self::Output | Self::Maintain
        )
    }
    pub fn jobs(self) -> u32 {
        if self == Self::Capture {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MidiBinding {
    pub loop_id: Identity,
    pub context: u16,
}

/// Concrete MIDI data operations, emitted by SP in program order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MidiOperation {
    Input {
        port: u16,
        destination: u16,
    },
    Capture {
        loop_id: Identity,
        sequence: u16,
        source: u16,
    },
    Playback {
        loop_id: Identity,
        sequence: u16,
        cursor: u16,
        channels: u16,
        destination: u16,
    },
    Merge {
        left: u16,
        right: u16,
        destination: u16,
    },
    Output {
        source: u16,
        port: u16,
    },
}
impl MidiOperation {
    pub fn jobs(self) -> u32 {
        if matches!(self, Self::Capture { .. }) {
            3
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Operation {
    MidiData(MidiOperation),
    Midi {
        phase: MidiPhase,
        loop_id: Option<Identity>,
        context: u16,
    },
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
    pub work_pass: Vec<Operation>,
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
        feature: Feature,
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

impl LogicalStateProgram {
    /// Logical identities are names, not physical state-memory addresses.
    pub fn validate_logical(&self, limits: ProgramLimits) -> Result<(), CompileError> {
        if self.loops.len() as u64 > u64::from(limits.loop_slots) {
            return Err(CompileError::Capacity(Resource::Loops));
        }
        if self.state_bindings.len() != self.initial_state.len() {
            return Err(CompileError::InvalidProgram);
        }
        self.validate(ProgramLimits {
            loop_slots: u32::MAX,
            ..limits
        })
    }
    /// Add MIDI work to an already compiled audio graph. Context slots are stable
    /// external resources, independent of instruction and loop-memory placement.
    pub fn with_midi(mut self, bindings: &[MidiBinding]) -> Result<Self, CompileError> {
        if self
            .work_pass
            .iter()
            .any(|op| matches!(op, Operation::Midi { .. }))
        {
            return Err(CompileError::InvalidProgram);
        }
        for (i, b) in bindings.iter().enumerate() {
            if b.context >= 128
                || !self.loops.iter().any(|l| l.id == b.loop_id)
                || bindings[..i]
                    .iter()
                    .any(|old| old.context == b.context || old.loop_id == b.loop_id)
            {
                return Err(CompileError::InvalidProgram);
            }
        }
        let mut ordered = bindings.to_vec();
        ordered.sort_unstable_by_key(|b| b.context);
        for phase in MidiPhase::ORDER {
            if phase.global() {
                self.work_pass.push(Operation::Midi {
                    phase,
                    loop_id: None,
                    context: 0,
                });
                self.jobs_per_quantum = self
                    .jobs_per_quantum
                    .checked_add(phase.jobs())
                    .ok_or(CompileError::InvalidProgram)?;
            } else {
                // Context identity determines MIDI tie order, independently of RAM layout.
                for b in &ordered {
                    self.work_pass.push(Operation::Midi {
                        phase,
                        loop_id: Some(b.loop_id),
                        context: b.context,
                    });
                    self.jobs_per_quantum = self
                        .jobs_per_quantum
                        .checked_add(phase.jobs())
                        .ok_or(CompileError::InvalidProgram)?;
                }
            }
        }
        self.resources.queue_capacity = self.resources.queue_capacity.max(self.jobs_per_quantum);
        Ok(self)
    }
    /// Append explicit data jobs; MIDI buffers use the logical buffer-number space.
    pub fn with_midi_data(mut self, operations: &[MidiOperation]) -> Result<Self, CompileError> {
        if self
            .work_pass
            .iter()
            .any(|op| matches!(op, Operation::Midi { .. }))
        {
            return Err(CompileError::InvalidProgram);
        }
        for &operation in operations {
            self.jobs_per_quantum = self
                .jobs_per_quantum
                .checked_add(operation.jobs())
                .ok_or(CompileError::InvalidProgram)?;
            self.work_pass.push(Operation::MidiData(operation));
        }
        self.resources.queue_capacity = self.resources.queue_capacity.max(self.jobs_per_quantum);
        Ok(self)
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
                (self.sync_pass.len() + self.work_pass.len()) as u64,
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
        for op in &self.work_pass {
            match *op {
                Operation::MidiData(operation) => match operation {
                    MidiOperation::Input { port, destination } => {
                        buffer(u32::from(destination))?;
                        if u32::from(port) >= r.input_ports {
                            return Err(InvalidProgram);
                        }
                    }
                    MidiOperation::Capture {
                        loop_id: id,
                        source,
                        ..
                    } => {
                        loop_id(id)?;
                        buffer(u32::from(source))?;
                    }
                    MidiOperation::Playback {
                        loop_id: id,
                        channels,
                        destination,
                        ..
                    } => {
                        loop_id(id)?;
                        buffer(u32::from(destination))?;
                        if channels == 0 {
                            return Err(InvalidProgram);
                        }
                    }
                    MidiOperation::Merge {
                        left,
                        right,
                        destination,
                    } => {
                        buffer(u32::from(left))?;
                        buffer(u32::from(right))?;
                        buffer(u32::from(destination))?;
                        if destination == left || destination == right {
                            return Err(InvalidProgram);
                        }
                    }
                    MidiOperation::Output { source, port } => {
                        buffer(u32::from(source))?;
                        if u32::from(port) >= r.output_ports {
                            return Err(InvalidProgram);
                        }
                    }
                },
                Operation::Midi {
                    phase,
                    loop_id: id,
                    context,
                } => {
                    if phase.global() != id.is_none() || context >= 128 {
                        return Err(InvalidProgram);
                    }
                    if let Some(id) = id {
                        loop_id(id)?;
                    }
                }
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
        if self
            .work_pass
            .iter()
            .any(|op| matches!(op, Operation::MidiData(_)))
        {
            if self
                .work_pass
                .iter()
                .any(|op| matches!(op, Operation::Midi { .. }))
            {
                return Err(InvalidProgram);
            }
            let needed = self
                .work_pass
                .iter()
                .try_fold(0u32, |n, op| {
                    n.checked_add(match op {
                        Operation::MidiData(op) => op.jobs(),
                        Operation::Capture { .. } => 2,
                        _ => 1,
                    })
                })
                .ok_or(InvalidProgram)?;
            if self.jobs_per_quantum < needed {
                return Err(InvalidProgram);
            }
            let mut captures = Vec::new();
            let mut cursors = Vec::new();
            for op in &self.work_pass {
                match op {
                    Operation::MidiData(MidiOperation::Capture {
                        loop_id, sequence, ..
                    }) => {
                        if captures
                            .iter()
                            .any(|(id, seq)| id == loop_id || seq == sequence)
                        {
                            return Err(InvalidProgram);
                        }
                        captures.push((*loop_id, *sequence));
                    }
                    Operation::MidiData(MidiOperation::Playback { cursor, .. }) => {
                        if cursors.contains(cursor) {
                            return Err(InvalidProgram);
                        }
                        cursors.push(*cursor);
                    }
                    _ => {}
                }
            }
        }
        let midi = self
            .work_pass
            .iter()
            .filter(|op| matches!(op, Operation::Midi { .. }));
        if midi.clone().next().is_some() {
            let bindings = midi
                .clone()
                .filter_map(|op| match *op {
                    Operation::Midi {
                        phase: MidiPhase::Capture,
                        loop_id: Some(loop_id),
                        context,
                    } => Some(MidiBinding { loop_id, context }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if bindings.windows(2).any(|w| w[0].context >= w[1].context)
                || bindings
                    .iter()
                    .enumerate()
                    .any(|(i, b)| bindings[..i].iter().any(|a| a.loop_id == b.loop_id))
            {
                return Err(InvalidProgram);
            }
            let mut actual = midi;
            for phase in MidiPhase::ORDER {
                if phase.global() {
                    if actual.next()
                        != Some(&Operation::Midi {
                            phase,
                            loop_id: None,
                            context: 0,
                        })
                    {
                        return Err(InvalidProgram);
                    }
                } else {
                    for b in &bindings {
                        if actual.next()
                            != Some(&Operation::Midi {
                                phase,
                                loop_id: Some(b.loop_id),
                                context: b.context,
                            })
                        {
                            return Err(InvalidProgram);
                        }
                    }
                }
            }
            if actual.next().is_some() {
                return Err(InvalidProgram);
            }
            let needed = self
                .work_pass
                .iter()
                .try_fold(0u32, |n, op| {
                    n.checked_add(match op {
                        Operation::Midi { phase, .. } => phase.jobs(),
                        Operation::Capture { .. } => 2,
                        _ => 1,
                    })
                })
                .ok_or(InvalidProgram)?;
            if self.jobs_per_quantum < needed {
                return Err(InvalidProgram);
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
