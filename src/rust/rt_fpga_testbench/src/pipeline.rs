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
    transitions::{TransitionError, TransitionPlan},
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
    External(ExecutionStatus),
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
    let mut results = Vec::new();
    for step in &case.steps {
        if step.transition.previews.is_empty() {
            return Err(PipelineError::Transition(TransitionError::InvalidConfig));
        }
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
        for write in &step.writes {
            p.write_state(write.address, write.value)
                .map_err(PipelineError::State)?;
        }
        let mut boundary = None;
        let previews = step
            .transition
            .previews
            .iter()
            .map(|observations| {
                let result = p.preview(observations);
                boundary = result.as_ref().ok().copied();
                Response::from(
                    result.and_then(|id| p.transitions().decisions(id).map(<[_]>::to_vec)),
                )
            })
            .collect();
        let mut error = None;
        let committed = boundary.is_some();
        if let Some(boundary) = boundary {
            error = p
                .run_state_with_intents(boundary, |intent| intent.prefetch(&mut media))
                .err();
        }
        let states = p.transitions().states().collect();
        let requests = p.transitions().request_slots().collect();
        let events = p.transitions().events().to_vec();
        let emitted: Vec<_> = p.queued_jobs().copied().collect();
        let mut audio = None;
        if let Some(boundary) = boundary.filter(|_| error.is_none()) {
            match &step.work {
                PipelineWork::External(status) => {
                    while p.pop_job().is_some() {}
                    error = p
                        .finish_jobs(
                            boundary,
                            match status {
                                ExecutionStatus::Completed => Ok(()),
                                ExecutionStatus::Failed { error } => Err(*error),
                            },
                        )
                        .err();
                }
                PipelineWork::Audio { inputs } => {
                    let inputs: Vec<_> = inputs.iter().map(Vec::as_slice).collect();
                    let mut slices: Vec<_> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
                    error = p
                        .run_jobs(boundary, &mut jobs, &mut media, &inputs, &mut slices)
                        .err();
                    let status = match error {
                        None => ExecutionStatus::Completed,
                        Some(PipelineError::Jobs(error)) => ExecutionStatus::Failed { error },
                        _ => ExecutionStatus::Failed {
                            error: Error::InvalidState,
                        },
                    };
                    audio = Some(AudioResult {
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
                }
            }
        }
        let failed = error.is_some();
        results.push(PipelineResult {
            transition: TransitionResult {
                frame,
                collected,
                cancelled,
                submitted,
                previews,
                committed,
                states,
                requests,
                events,
                after_finish: p.transitions().states().collect(),
                next_frame: p.transitions().frame(),
            },
            jobs: emitted,
            state: StateSnapshot::from(p.state()),
            error,
            audio,
        });
        if failed {
            break;
        }
    }
    Ok(PipelineTrace { case, results })
}
