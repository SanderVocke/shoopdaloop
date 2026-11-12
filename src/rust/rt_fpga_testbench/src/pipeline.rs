//! Combined shared-loop quantum traces for the Rust and RTL coordinators.
use crate::{
    transitions::{Collection, Response, TransitionPlanInput, TransitionResult, TransitionStep},
    AudioResult, ExecutionStage, ExecutionStatus, StateSnapshot, StateWrite,
};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, Job, JobProcessor},
    pipeline::{Pipeline, PipelineError},
    state::Program,
    storage::{MediaSnapshot, MediaStore},
    transitions::TransitionPlan,
    Error,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineCase {
    pub name: String,
    pub plan: TransitionPlanInput,
    pub program: Program,
    pub initial_state: StateSnapshot,
    pub queue_capacity: u32,
    pub buffers: u32,
    pub output_ports: u32,
    pub media: Option<MediaSnapshot>,
    pub steps: Vec<PipelineStep>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineStep {
    pub transition: TransitionStep,
    pub writes: Vec<StateWrite>,
    pub work: PipelineWork,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PipelineWork {
    Audio { inputs: Vec<Vec<Fixed>> },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineResult {
    pub transition: TransitionResult,
    pub jobs: Vec<Job>,
    pub state: StateSnapshot,
    pub error: Option<PipelineError>,
    pub audio: Option<AudioResult>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineTrace {
    pub case: PipelineCase,
    pub results: Vec<PipelineResult>,
}

pub fn run_pipeline_case(case: PipelineCase) -> Result<PipelineTrace, PipelineError> {
    let plan = TransitionPlan::compile(case.plan.description.clone(), case.plan.limits)
        .map_err(PipelineError::Transition)?;
    let mut p = Pipeline::prepare(
        plan,
        case.program.clone(),
        case.initial_state.memory.clone(),
        case.queue_capacity as usize,
    )?;
    let mut jobs = JobProcessor::new(case.program.quantum as usize, case.buffers as usize);
    let mut media = match &case.media {
        Some(s) => MediaStore::from_snapshot(s.clone()),
        None => MediaStore::new(case.program.chunk_frames as usize),
    }
    .map_err(PipelineError::Jobs)?;
    let mut outputs =
        vec![vec![Fixed::ZERO; case.program.quantum as usize]; case.output_ports as usize];
    p.trace_jobs(true);
    let mut results = Vec::new();
    for step in &case.steps {
        let frame = p.transitions().frame();
        let automatic: Vec<_> = if step.transition.collect_completed {
            p.transitions()
                .request_slots()
                .flatten()
                .filter(|r| r.outcome.is_some())
                .map(|r| r.id)
                .collect()
        } else {
            Vec::new()
        };
        let collected = automatic
            .iter()
            .chain(&step.transition.collect)
            .map(|&request| Collection {
                request,
                result: p.collect(request).into(),
            })
            .collect();
        let cancelled = step
            .transition
            .cancel
            .iter()
            .map(|&id| p.cancel(id).into())
            .collect();
        let submitted = step
            .transition
            .commands
            .iter()
            .map(|&c| p.submit(c).into())
            .collect();
        if p.fault().is_none() {
            for write in &step.writes {
                p.write_state(write.address, write.value)
                    .map_err(PipelineError::State)?;
            }
        }
        let PipelineWork::Audio { inputs } = &step.work;
        let input_slices: Vec<_> = inputs.iter().map(Vec::as_slice).collect();
        let mut output_slices: Vec<_> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
        let error = p
            .process(&mut jobs, &mut media, &input_slices, &mut output_slices)
            .err();
        let emitted = p.emitted_jobs().to_vec();
        let status = match error {
            None => ExecutionStatus::Completed,
            Some(PipelineError::Jobs(error) | PipelineError::State(error)) => {
                ExecutionStatus::Failed { error }
            }
            _ => ExecutionStatus::Failed {
                error: Error::InvalidState,
            },
        };
        let processed = match error {
            Some(PipelineError::Transition(error)) => Response::Rejected(error),
            _ => Response::Accepted(()),
        };
        let audio = Some(AudioResult {
            jobs: emitted.clone(),
            remaining_jobs: p.queued_jobs().copied().collect(),
            state: StateSnapshot::from(p.state()),
            buffers: (0..case.buffers)
                .map(|i| {
                    jobs.buffer(BufferId(i as usize))
                        .ok()
                        .map(<[Fixed]>::to_vec)
                })
                .collect(),
            outputs: outputs.clone(),
            stage: ExecutionStage::Jobs,
            status,
            media: case.media.as_ref().map(|_| media.snapshot()),
        });
        results.push(PipelineResult {
            transition: TransitionResult {
                frame,
                collected,
                cancelled,
                submitted,
                processed,
                states: p.transitions().states().collect(),
                requests: p.transitions().request_slots().collect(),
                events: p.transitions().events().to_vec(),
                next_frame: p.transitions().frame(),
            },
            jobs: emitted,
            state: StateSnapshot::from(p.state()),
            error,
            audio,
        });
    }
    Ok(PipelineTrace { case, results })
}
