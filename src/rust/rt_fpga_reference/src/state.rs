//! Persistent node-level state execution. No graph or sample-store access.

use crate::{
    fixed::Ramp,
    jobs::{BufferId, Job, JobQueue},
    storage::ViewId,
    transitions::{CommittedLoops, LoopId, Mode},
    Error,
};
use alloc::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub quantum: u32,
    /// Samples per media chunk. Media positions and lengths use chunk units.
    pub chunk_frames: u32,
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub jobs_per_quantum: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Address(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Value {
    Word(u64),
    Ramp(Ramp),
}
impl Value {
    pub fn word(self) -> Result<u64, Error> {
        match self {
            Self::Word(value) => Ok(value),
            _ => Err(Error::InvalidState),
        }
    }
    pub fn ramp(self) -> Result<Ramp, Error> {
        match self {
            Self::Ramp(value) => Ok(value),
            _ => Err(Error::InvalidState),
        }
    }
}

/// Operands reference live state, never values specialized to the current mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Instruction {
    ProcessLogicalPlayback {
        loop_id: LoopId,
        view: Address,
        destination: BufferId,
    },
    ProcessLogicalCapture {
        loop_id: LoopId,
        view: Address,
        source: BufferId,
    },
    ProcessInput {
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
        destination: BufferId,
    },
    ProcessLoopPlayback {
        view: Address,
        position: Address,
        length: Address,
        enabled: Address,
        destination: BufferId,
    },
    ProcessGain {
        ramp: Address,
        source: BufferId,
        destination: BufferId,
    },
    ProcessMix {
        left: BufferId,
        right: BufferId,
        destination: BufferId,
    },
    ProcessLoopCapture {
        view: Address,
        enabled: Address,
        source: BufferId,
    },
    ProcessOutput {
        source: BufferId,
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
    },
}

pub struct StateProcessor {
    memory: Vec<Value>,
}
impl StateProcessor {
    pub fn new(memory: Vec<Value>) -> Self {
        Self { memory }
    }
    pub fn values(&self) -> &[Value] {
        &self.memory
    }
    pub fn read(&self, address: Address) -> Result<Value, Error> {
        self.memory
            .get(address.0)
            .copied()
            .ok_or(Error::InvalidState)
    }
    pub fn write(&mut self, address: Address, value: Value) -> Result<(), Error> {
        *self.memory.get_mut(address.0).ok_or(Error::InvalidState)? = value;
        Ok(())
    }
    fn view(&self, address: Address) -> Result<ViewId, Error> {
        Ok(ViewId(
            usize::try_from(self.read(address)?.word()?).map_err(|_| Error::InvalidView)?,
        ))
    }
    pub fn run(&mut self, program: &Program, queue: &mut JobQueue) -> Result<(), Error> {
        self.run_inner(program, queue, None, &mut |_| {})
    }
    pub fn run_with_intents(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        mut sink: impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        self.run_inner(program, queue, None, &mut sink)
    }
    pub fn run_with_loops(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: CommittedLoops<'_>,
    ) -> Result<(), Error> {
        self.run_inner(program, queue, Some(loops), &mut |_| {})
    }
    pub fn run_with_loops_and_intents(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: CommittedLoops<'_>,
        mut sink: impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        self.run_inner(program, queue, Some(loops), &mut sink)
    }
    fn run_inner(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: Option<CommittedLoops<'_>>,
        sink: &mut impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        if !queue.is_empty() || queue.remaining() < program.jobs_per_quantum {
            return Err(Error::QueueFull);
        }
        if program.chunk_frames == 0
            || program.quantum == 0
            || program.quantum % program.chunk_frames != 0
        {
            return Err(Error::InvalidState);
        }
        let mut emit = |job| {
            queue.push(job)?;
            if let Job::ReadView {
                view,
                position,
                length,
                ..
            } = job
            {
                sink(crate::resolver::ReadIntent {
                    view,
                    position,
                    length,
                    chunks: u64::from(program.quantum / program.chunk_frames),
                });
            }
            Ok::<(), Error>(())
        };
        for &instruction in &program.instructions {
            match instruction {
                Instruction::ProcessLogicalPlayback {
                    loop_id,
                    view,
                    destination,
                } => {
                    let state = loops
                        .ok_or(Error::InvalidState)?
                        .read(loop_id)
                        .map_err(|_| Error::InvalidState)?;
                    let position = state.position / u64::from(program.chunk_frames);
                    let length = state.content_length / u64::from(program.chunk_frames);
                    emit(if state.plays() && length != 0 && position < length {
                        Job::ReadView {
                            view: self.view(view)?,
                            position,
                            length,
                            destination,
                        }
                    } else {
                        Job::Silence { destination }
                    })?;
                }
                Instruction::ProcessLogicalCapture {
                    loop_id,
                    view,
                    source,
                } => {
                    let state = loops
                        .ok_or(Error::InvalidState)?
                        .read(loop_id)
                        .map_err(|_| Error::InvalidState)?;
                    if state.mode == Mode::Recording {
                        emit(Job::Capture {
                            source,
                            view: self.view(view)?,
                        })?;
                    }
                }
                Instruction::ProcessInput { port, destination } => {
                    emit(Job::Input { port, destination })?
                }
                Instruction::ProcessLoopPlayback {
                    view,
                    position,
                    length,
                    enabled,
                    destination,
                } => {
                    if self.read(enabled)?.word()? == 0 {
                        emit(Job::Silence { destination })?;
                    } else {
                        let current = self.read(position)?.word()?;
                        let length = self.read(length)?.word()?;
                        if length == 0 || current >= length {
                            return Err(Error::InvalidState);
                        }
                        emit(Job::ReadView {
                            view: self.view(view)?,
                            position: current,
                            length,
                            destination,
                        })?;
                    }
                }
                Instruction::ProcessGain {
                    ramp,
                    source,
                    destination,
                } => {
                    let mut value = self.read(ramp)?.ramp()?;
                    emit(Job::Gain {
                        ramp: value,
                        source,
                        destination,
                    })?;
                    value.advance(program.quantum);
                    self.write(ramp, Value::Ramp(value))?;
                }
                Instruction::ProcessMix {
                    left,
                    right,
                    destination,
                } => emit(Job::Mix {
                    left,
                    right,
                    destination,
                })?,
                Instruction::ProcessLoopCapture {
                    view,
                    enabled,
                    source,
                } => {
                    if self.read(enabled)?.word()? != 0 {
                        emit(Job::Capture {
                            view: self.view(view)?,
                            source,
                        })?;
                    }
                }
                Instruction::ProcessOutput { source, port } => emit(Job::Output { source, port })?,
            }
        }
        Ok(())
    }
}
