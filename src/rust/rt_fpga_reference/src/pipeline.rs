//! One quantum of transition settlement, node state and sample execution.
use crate::{
    fixed::Fixed,
    jobs::{Job, JobProcessor, JobQueue, Memory},
    state::{Address, Program, StateProcessor, Value},
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

pub struct Pipeline {
    transitions: TransitionProcessor,
    state: StateProcessor,
    program: Program,
    queue: JobQueue,
    fault: Option<PipelineError>,
    trace_jobs: bool,
    emitted: Vec<Job>,
}
impl Pipeline {
    pub fn prepare(
        plan: TransitionPlan,
        program: Program,
        initial_state: Vec<Value>,
        capacity: usize,
    ) -> Result<Self, PipelineError> {
        if plan.image().description.quantum != program.quantum {
            return Err(PipelineError::Transition(TransitionError::InvalidConfig));
        }
        if capacity < program.jobs_per_quantum {
            return Err(PipelineError::State(Error::QueueFull));
        }
        Ok(Self {
            transitions: TransitionProcessor::prepare(plan),
            state: StateProcessor::new(initial_state),
            program,
            queue: JobQueue::new(capacity),
            fault: None,
            trace_jobs: false,
            emitted: Vec::with_capacity(capacity),
        })
    }
    pub fn transitions(&self) -> &TransitionProcessor {
        &self.transitions
    }
    pub fn state(&self) -> &StateProcessor {
        &self.state
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
    pub fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError> {
        self.ready()?;
        self.transitions.submit(command)
    }
    pub fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError> {
        self.ready()?;
        self.transitions.cancel(id)
    }
    pub fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        self.ready()?;
        self.transitions.take_completion(id)
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
        let result = (0..frames)
            .step_by(q)
            .try_for_each(|base| self.process_quantum(jobs, memory, inputs, outputs, base));
        if let Err(error) = result {
            self.fault = Some(error);
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
        }
        result
    }
    fn process_quantum(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
    ) -> Result<(), PipelineError> {
        self.emitted.clear();
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        self.transitions
            .settle()
            .map_err(PipelineError::Transition)?;
        let state_result = self.state.run_with_loops_and_intents(
            &self.program,
            &mut self.queue,
            self.transitions
                .settled()
                .map_err(PipelineError::Transition)?,
            |intent| intent.prefetch(memory),
        );
        if self.trace_jobs {
            self.emitted.extend(self.queue.iter().copied());
        }
        state_result.map_err(PipelineError::State)?;
        jobs.run_at(&mut self.queue, memory, inputs, outputs, base)
            .map_err(PipelineError::Jobs)?;
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        self.transitions.finish().map_err(PipelineError::Transition)
    }
}
