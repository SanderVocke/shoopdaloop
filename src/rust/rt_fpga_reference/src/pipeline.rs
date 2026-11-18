//! One quantum of state, job and media execution.
use crate::{
    fixed::Fixed,
    jobs::{Job, JobProcessor, JobQueue, Memory},
    state::{Address, ModePlan, PlannedTransition, Program, StateProcessor, Value},
    transitions::*,
    Error,
};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "stage",
        content = "error",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum PipelineError {
    Transition(TransitionError),
    State(Error),
    Jobs(Error),
}

/// The first settled tick and each completed sample quantum of one callback.
pub trait Observer {
    fn settled(&mut self, _state: &StateProcessor) {}
    fn processed(&mut self, _jobs: &JobProcessor) {}
}
impl Observer for () {}

pub struct Pipeline {
    revision: u64,
    region: u8,
    state: StateProcessor,
    program: Program,
    queue: JobQueue,
    fault: Option<PipelineError>,
    trace_jobs: bool,
    emitted: Vec<Job>,
}
impl Pipeline {
    pub fn from_image(
        image: crate::compiler::ProgramImage,
        limits: rt_fpga_compiler::ProgramLimits,
    ) -> Result<Self, PipelineError> {
        let revision = image.layout.revision;
        let region = image.layout.region;
        let compiled = image.compiled;
        let mut pipeline = Self::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            limits.jobs as usize,
        )?;
        pipeline.reserve_loading(limits)?;
        pipeline.revision = revision;
        pipeline.region = region;
        Ok(pipeline)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn region(&self) -> u8 {
        self.region
    }
    pub fn reserve_loading(
        &mut self,
        limits: rt_fpga_compiler::ProgramLimits,
    ) -> Result<(), PipelineError> {
        self.state.reserve_loading(limits);
        self.queue = JobQueue::new(limits.jobs as usize);
        self.emitted = Vec::with_capacity(limits.jobs as usize);
        Ok(())
    }
    /// Called at a completed quantum boundary; the caller retains the old image until its fence completes.
    pub fn switch_image(
        &mut self,
        image: &mut crate::compiler::ProgramImage,
    ) -> Result<(), PipelineError> {
        if image.layout.expected != self.revision
            || image.layout.revision <= self.revision
            || (self.revision != 0 && image.layout.region == self.region)
            || self.fault.is_some()
            || !self.queue.is_empty()
            || image.compiled.program.quantum != self.program.quantum
            || image.compiled.program.chunk_frames != self.program.chunk_frames
            || image.compiled.program.jobs_per_quantum > self.queue.remaining()
        {
            return Err(PipelineError::State(Error::InvalidState));
        }
        self.state.switch_image(image);
        core::mem::swap(&mut self.program, &mut image.compiled.program);
        self.revision = image.layout.revision;
        self.region = image.layout.region;
        Ok(())
    }
    pub fn prepare(
        plan: TransitionPlan,
        mut program: Program,
        initial_state: Vec<Value>,
        capacity: usize,
    ) -> Result<Self, PipelineError> {
        if plan.image().description.quantum != program.quantum {
            return Err(PipelineError::Transition(TransitionError::InvalidConfig));
        }
        if capacity < program.jobs_per_quantum {
            return Err(PipelineError::State(Error::QueueFull));
        }
        if program.sync_instructions.is_empty() {
            program.sync_instructions = plan.sync_instructions();
        }
        Ok(Self {
            revision: 0,
            region: 0,
            state: StateProcessor::with_loops(plan, initial_state),
            program,
            queue: JobQueue::new(capacity),
            fault: None,
            trace_jobs: false,
            emitted: Vec::with_capacity(capacity),
        })
    }
    pub fn state(&self) -> &StateProcessor {
        &self.state
    }
    pub fn uses_media_view(&self, view: crate::storage::ViewId) -> bool {
        self.program.instructions.iter().any(|instruction| {
            let address = match instruction {
                crate::state::Instruction::ProcessLogicalPlayback { view, .. }
                | crate::state::Instruction::ProcessLogicalCapture { view, .. }
                | crate::state::Instruction::ProcessLoopPlayback { view, .. }
                | crate::state::Instruction::ProcessLoopCapture { view, .. } => *view,
                _ => return false,
            };
            self.state.read(address).ok().and_then(|v| v.word().ok()) == Some(view.0 as u64)
        })
    }
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn fault(&self) -> Option<PipelineError> {
        self.fault
    }
    pub fn trace_jobs(&mut self, enabled: bool) {
        self.trace_jobs = enabled;
    }
    pub fn emitted_jobs(&self) -> &[Job] {
        &self.emitted
    }
    pub fn queued_jobs(&self) -> impl Iterator<Item = &Job> {
        self.queue.iter()
    }
    fn ready(&self) -> Result<(), TransitionError> {
        if self.fault.is_some() {
            Err(TransitionError::InvalidState)
        } else {
            Ok(())
        }
    }
    pub fn write_plans(
        &mut self,
        id: LoopId,
        plans: [Option<PlannedTransition>; 2],
    ) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.write_plans(id, plans)
    }
    pub fn write_mode_plan(&mut self, id: LoopId, plan: ModePlan) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.write_mode_plan(id, plan)
    }
    pub fn clear_mode_plan(&mut self, id: LoopId, mask: u8) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.clear_mode_plan(id, mask)
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn initialize_frame(&mut self, frame: u64) -> Result<(), Error> {
        self.state.initialize_frame(frame)
    }
    pub fn write_state(&mut self, address: Address, value: Value) -> Result<(), Error> {
        if self.fault.is_some() {
            return Err(Error::InvalidState);
        }
        self.state.write(address, value)
    }
    /// Failed output is silent. Recreate the engine to resume after a fault.
    pub fn process(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), PipelineError> {
        self.process_block(jobs, memory, inputs, outputs, self.program.quantum as usize)
    }
    pub fn process_block(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
    ) -> Result<(), PipelineError> {
        self.process_observed(jobs, memory, inputs, outputs, frames, &mut ())
    }
    pub fn process_observed(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
        observer: &mut impl Observer,
    ) -> Result<(), PipelineError> {
        for output in outputs.iter_mut() {
            output.fill(Fixed::ZERO);
        }
        self.emitted.clear();
        if let Some(error) = self.fault {
            return Err(error);
        }
        let q = self.program.quantum as usize;
        if q == 0
            || frames == 0
            || frames % q != 0
            || jobs.quantum() != q
            || memory.chunk_frames() != self.program.chunk_frames as usize
            || inputs.iter().any(|input| input.len() != frames)
            || outputs.iter().any(|output| output.len() != frames)
        {
            return Err(PipelineError::Jobs(Error::InvalidShape));
        }
        let result = (0..frames).step_by(q).try_for_each(|base| {
            self.process_quantum(jobs, memory, inputs, outputs, base, observer, base == 0)
        });
        if let Err(error) = result {
            self.fault = Some(error);
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
        }
        result
    }
    pub(crate) fn process_quantum(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
        observer: &mut impl Observer,
        publish: bool,
    ) -> Result<(), PipelineError> {
        self.emitted.clear();
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        let state_result = self
            .state
            .run_with_intents(&self.program, &mut self.queue, |intent| {
                intent.prefetch(memory)
            });
        if self.trace_jobs {
            self.emitted.extend(self.queue.iter().copied());
        }
        state_result.map_err(|error| match error {
            Error::Transition(error) => PipelineError::Transition(error),
            error => PipelineError::State(error),
        })?;
        if publish {
            observer.settled(&self.state);
        }
        jobs.run_at(&mut self.queue, memory, inputs, outputs, base)
            .map_err(PipelineError::Jobs)?;
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        observer.processed(jobs);
        Ok(())
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn latch_fault(&mut self, error: PipelineError) {
        self.fault = Some(error);
    }
}
