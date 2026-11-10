//! Serial quantum coordination with read-only access to committed loop records.
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
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "stage",
        content = "error",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PipelineError {
    Transition(TransitionError),
    State(Error),
    Jobs(Error),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Preparing,
    Running(BoundaryId),
    Faulted,
}

pub struct Pipeline {
    transitions: TransitionProcessor,
    state: StateProcessor,
    program: Program,
    queue: JobQueue,
    phase: Phase,
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
            phase: Phase::Idle,
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
    pub fn queued_jobs(&self) -> impl Iterator<Item = &Job> {
        self.queue.iter()
    }
    pub fn submit(&mut self, command: Command) -> Result<RequestId, TransitionError> {
        self.transitions.submit(command)
    }
    pub fn cancel(&mut self, id: RequestId) -> Result<bool, TransitionError> {
        self.transitions.cancel(id)
    }
    pub fn collect(&mut self, id: RequestId) -> Result<Option<Completion>, TransitionError> {
        self.transitions.take_completion(id)
    }
    pub fn write_state(&mut self, address: Address, value: Value) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Idle | Phase::Preparing) {
            return Err(Error::InvalidState);
        }
        self.state.write(address, value)
    }
    pub fn preview(&mut self, observations: &[Observation]) -> Result<BoundaryId, TransitionError> {
        if !matches!(self.phase, Phase::Idle | Phase::Preparing) {
            return Err(TransitionError::InvalidState);
        }
        self.phase = Phase::Preparing;
        self.transitions.preview(observations)
    }
    pub fn run_state(&mut self, boundary: BoundaryId) -> Result<(), PipelineError> {
        if self.phase != Phase::Preparing {
            return Err(PipelineError::Transition(TransitionError::InvalidState));
        }
        self.transitions
            .commit(boundary)
            .map_err(PipelineError::Transition)?;
        self.phase = Phase::Running(boundary);
        let result = self.state.run_with_loops(
            &self.program,
            &mut self.queue,
            self.transitions
                .committed(boundary)
                .map_err(PipelineError::Transition)?,
        );
        if result.is_err() {
            self.phase = Phase::Faulted;
        }
        result.map_err(PipelineError::State)
    }
    pub fn pop_job(&mut self) -> Option<Job> {
        if matches!(self.phase, Phase::Running(_)) {
            self.queue.pop()
        } else {
            None
        }
    }
    pub fn finish_jobs(
        &mut self,
        boundary: BoundaryId,
        result: Result<(), Error>,
    ) -> Result<(), PipelineError> {
        if self.phase != Phase::Running(boundary) {
            return Err(PipelineError::Transition(TransitionError::InvalidState));
        }
        if let Err(error) = result {
            self.phase = Phase::Faulted;
            return Err(PipelineError::Jobs(error));
        }
        if !self.queue.is_empty() {
            return Err(PipelineError::Jobs(Error::InvalidState));
        }
        self.transitions
            .finish_quantum(boundary)
            .map_err(PipelineError::Transition)?;
        self.phase = Phase::Idle;
        Ok(())
    }
    pub fn run_jobs(
        &mut self,
        boundary: BoundaryId,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), PipelineError> {
        if self.phase != Phase::Running(boundary) {
            return Err(PipelineError::Transition(TransitionError::InvalidState));
        }
        if jobs.quantum() != self.program.quantum as usize {
            return self.finish_jobs(boundary, Err(Error::InvalidShape));
        }
        let result = jobs.run(&mut self.queue, memory, inputs, outputs);
        self.finish_jobs(boundary, result)
    }
    pub fn run(
        &mut self,
        observations: &[Observation],
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), PipelineError> {
        let boundary = self
            .preview(observations)
            .map_err(PipelineError::Transition)?;
        self.run_state(boundary)?;
        self.run_jobs(boundary, jobs, memory, inputs, outputs)
    }
}
