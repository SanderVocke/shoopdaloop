//! Sync-ordered loop evolution followed by audio-ordered state/job emission.

use crate::{
    fixed::Ramp,
    jobs::{BufferId, Job, JobQueue},
    records::{Identity, Mode, SettledRecords},
    storage::ViewId,
    Error,
};
use alloc::vec::Vec;
pub mod sync;
use crate::transitions::{Event, LoopId, LoopState, TransitionError, TransitionPlan};
pub use sync::{LoopSlot, ModePlan, ModeStep, PlannedTransition, Resolution, SyncInstruction};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Program {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub sync_instructions: Vec<SyncInstruction>,
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
    ProcessSilence {
        destination: BufferId,
    },
    ProcessConstant {
        value: i32,
        destination: BufferId,
    },
    ProcessArithmetic {
        left: BufferId,
        right: BufferId,
        destination: BufferId,
        mode: crate::jobs::ArithmeticMode,
    },
    ProcessSmoothing {
        loop_id: Identity,
        control: Address,
        source: BufferId,
    },
    ProcessLogicalPlayback {
        loop_id: Identity,
        view: Address,
        destination: BufferId,
    },
    ProcessLogicalCapture {
        loop_id: Identity,
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
    loops: Option<sync::LoopMemory>,
    fault: Option<Error>,
}
impl StateProcessor {
    pub fn new(memory: Vec<Value>) -> Self {
        Self {
            memory,
            loops: None,
            fault: None,
        }
    }
    pub fn with_loops(plan: TransitionPlan, memory: Vec<Value>) -> Self {
        Self {
            memory,
            loops: Some(sync::LoopMemory::prepare(plan)),
            fault: None,
        }
    }
    pub fn quantum(&self) -> Option<u32> {
        self.loops
            .as_ref()
            .map(|m| m.plan.image.description.quantum)
    }
    pub fn frame(&self) -> u64 {
        self.loops.as_ref().map_or(0, |loops| loops.frame)
    }
    pub fn loop_state(&self, id: LoopId) -> Result<&LoopSlot, TransitionError> {
        self.loops
            .as_ref()
            .ok_or(TransitionError::InvalidState)?
            .slot(id)
    }
    pub fn loop_states(&self) -> impl Iterator<Item = LoopState> + '_ {
        self.loops
            .iter()
            .flat_map(|loops| loops.slots.iter().flatten().map(|slot| slot.state))
    }
    pub fn events(&self) -> &[Event] {
        self.loops
            .as_ref()
            .map_or(&[], |loops| loops.events.as_slice())
    }
    pub fn fault(&self) -> Option<Error> {
        self.fault
    }
    /// Replace the two physical plan slots atomically.
    pub fn write_plans(
        &mut self,
        id: LoopId,
        plans: [Option<PlannedTransition>; 2],
    ) -> Result<(), TransitionError> {
        if self.fault.is_some() {
            return Err(TransitionError::InvalidState);
        }
        let loops = self.loops.as_mut().ok_or(TransitionError::InvalidState)?;
        loops.slot(id)?;
        if matches!(plans, [Some(a), Some(b)] if a.sequence == b.sequence) {
            return Err(TransitionError::DuplicateId);
        }
        for plan in plans.iter().flatten() {
            if plan.on_pulse
                && loops.plan.specs[id.slot as usize]
                    .unwrap()
                    .sync_source
                    .is_none()
            {
                return Err(TransitionError::MissingSync);
            }
            if let crate::transitions::Operation::Play { position, .. }
            | crate::transitions::Operation::Seek { position, .. } = plan.operation
            {
                if position % u64::from(loops.plan.image().description.quantum) != 0 {
                    return Err(TransitionError::InvalidFrame);
                }
            }
        }
        let slot = loops.slot_mut(id)?;
        slot.mode_plan = false;
        slot.planned = plans;
        Ok(())
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn initialize_frame(&mut self, frame: u64) -> Result<(), Error> {
        let memory = self.loops.as_mut().ok_or(Error::InvalidState)?;
        if memory.frame != 0 || frame % u64::from(memory.plan.image.description.quantum) != 0 {
            return Err(Error::InvalidShape);
        }
        memory.frame = frame;
        Ok(())
    }
    pub fn write_mode_plan(&mut self, id: LoopId, plan: ModePlan) -> Result<(), TransitionError> {
        if self.fault.is_some() {
            return Err(TransitionError::InvalidState);
        }
        self.loops
            .as_mut()
            .ok_or(TransitionError::InvalidState)?
            .write_mode_plan(id, plan)
    }
    pub fn clear_mode_plan(&mut self, id: LoopId, mask: u8) -> Result<(), TransitionError> {
        if self.fault.is_some() {
            return Err(TransitionError::InvalidState);
        }
        self.loops
            .as_mut()
            .ok_or(TransitionError::InvalidState)?
            .clear_mode_plan(id, mask)
    }
    pub fn mode_plan(&self, id: LoopId) -> Result<(ModePlan, u32), TransitionError> {
        self.loops
            .as_ref()
            .ok_or(TransitionError::InvalidState)?
            .mode_plan(id)
    }
    /// Settled state for the audio pass, before persistent cursor advancement.
    pub fn render_state(&self, id: LoopId) -> Result<LoopState, TransitionError> {
        let slot = self.loop_state(id)?;
        let mut state = slot.state;
        state.position = slot.render.position;
        if self.frame() != 0 && state.mode == crate::transitions::Mode::Recording {
            state.capture_frames = state
                .capture_frames
                .checked_sub(u64::from(
                    self.loops.as_ref().unwrap().plan.image.description.quantum,
                ))
                .ok_or(TransitionError::InvalidState)?;
        }
        Ok(state)
    }
    /// Plan-slot outcomes from the most recently executed quantum.
    pub fn resolutions(&self) -> &[Resolution] {
        self.loops.as_ref().map_or(&[], |loops| &loops.resolutions)
    }
    fn loop_record(
        &self,
        external: Option<SettledRecords<'_>>,
        id: Identity,
    ) -> Result<crate::records::LoopRecord, Error> {
        if let Some(records) = external {
            return records.read(id);
        }
        self.loop_state(id)
            .map(|slot| slot.render)
            .map_err(|_| Error::InvalidState)
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
    pub fn run_with_records(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        records: SettledRecords<'_>,
    ) -> Result<(), Error> {
        self.run_inner(program, queue, Some(records), &mut |_| {})
    }
    pub fn run_with_intents(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        mut sink: impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        self.run_inner(program, queue, None, &mut sink)
    }
    pub fn run_with_records_and_intents(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: SettledRecords<'_>,
        mut sink: impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        self.run_inner(program, queue, Some(loops), &mut sink)
    }
    fn run_inner(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: Option<SettledRecords<'_>>,
        sink: &mut impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        let result = self.run_program(program, queue, loops, sink);
        if self.loops.is_some() {
            self.fault = result.err();
        }
        result
    }
    fn run_program(
        &mut self,
        program: &Program,
        queue: &mut JobQueue,
        loops: Option<SettledRecords<'_>>,
        sink: &mut impl FnMut(crate::resolver::ReadIntent),
    ) -> Result<(), Error> {
        if !queue.is_empty() || queue.remaining() < program.jobs_per_quantum {
            return Err(Error::QueueFull);
        }
        if !program.chunk_frames.is_power_of_two()
            || program.quantum == 0
            || program.quantum % program.chunk_frames != 0
        {
            return Err(Error::InvalidState);
        }
        if let Some(memory) = &mut self.loops {
            if loops.is_some() {
                return Err(Error::InvalidState);
            }
            memory
                .run(&program.sync_instructions, program.quantum)
                .map_err(Error::Transition)?;
        } else if !program.sync_instructions.is_empty() {
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
                Instruction::ProcessSilence { destination } => emit(Job::Silence { destination })?,
                Instruction::ProcessConstant { value, destination } => {
                    emit(Job::Constant { value, destination })?
                }
                Instruction::ProcessArithmetic {
                    left,
                    right,
                    destination,
                    mode,
                } => emit(Job::Arithmetic {
                    left,
                    right,
                    destination,
                    mode,
                })?,
                Instruction::ProcessSmoothing {
                    loop_id,
                    control,
                    source,
                } => {
                    let record = self.loop_record(loops, loop_id)?;
                    let word = self.read(control)?.word()?;
                    emit(Job::Smoothing {
                        source,
                        destination: BufferId((word & 0xffff) as usize),
                        history: ((word >> 16) & 0xffff) as usize,
                        duration: (word >> 32) as u32,
                        playing: record.plays(),
                        restart: record.position == 0,
                    })?;
                }
                Instruction::ProcessLogicalPlayback {
                    loop_id,
                    view,
                    destination,
                } => {
                    let state = self.loop_record(loops, loop_id)?;
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
                    let state = self.loop_record(loops, loop_id)?;
                    if state.mode == Mode::Recording {
                        if state.begin_capture {
                            emit(Job::BeginCapture {
                                view: self.view(view)?,
                            })?;
                        }
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
