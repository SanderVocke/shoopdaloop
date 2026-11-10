//! Architectural values and quantum traces for software/RTL comparison.
#![forbid(unsafe_code)]

use rt_fpga_reference::{
    compiler::{Binding, Compiled, Graph},
    fixed::Fixed,
    jobs::{BufferId, Job, JobProcessor, JobQueue},
    state::{Address, Instruction, Program, StateProcessor, Value},
    storage::{MediaSnapshot, MediaStore, ViewId},
    transitions::OnWrap,
    Error,
};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub mod pipeline;
pub mod transitions;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct FormatVersion;

impl TryFrom<u32> for FormatVersion {
    type Error = &'static str;
    fn try_from(version: u32) -> Result<Self, Self::Error> {
        if version == FORMAT_VERSION {
            Ok(Self)
        } else {
            Err("unsupported FPGA testbench format version")
        }
    }
}
impl From<FormatVersion> for u32 {
    fn from(_: FormatVersion) -> Self {
        FORMAT_VERSION
    }
}
impl JsonSchema for FormatVersion {
    fn schema_name() -> Cow<'static, str> {
        "FormatVersion".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> Schema {
        schemars::json_schema!({ "type": "integer", "const": FORMAT_VERSION })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: FormatVersion,
    pub payload: Payload,
}
impl Document {
    pub fn new(payload: Payload) -> Self {
        Self {
            format_version: FormatVersion,
            payload,
        }
    }
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let document: Self = serde_json::from_str(json)?;
        // Serde also accepts explicit null content for unit variants. Require
        // the same field shapes as the generated schema and serialized output.
        if serde_json::from_str::<serde_json::Value>(json)? != serde_json::to_value(&document)? {
            return Err(<serde_json::Error as serde::de::Error>::custom(
                "document fields do not match the canonical variant shape",
            ));
        }
        Ok(document)
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Payload {
    Graph(Graph),
    Compiled(Compiled),
    Program(Program),
    Instructions(Vec<Instruction>),
    State(StateSnapshot),
    Jobs(Vec<Job>),
    Bindings(Vec<Binding>),
    StateWrites(Vec<StateWrite>),
    OnWrap(OnWrap),
    Status(ExecutionStatus),
    Case(StateCase),
    Trace(StateTrace),
    AudioCase(AudioCase),
    AudioTrace(AudioTrace),
    Media(MediaSnapshot),
    TransitionPlan(transitions::TransitionPlanInput),
    TransitionPlanImage(rt_fpga_reference::transitions::TransitionPlanImage),
    TransitionCase(transitions::TransitionCase),
    TransitionTrace(transitions::TransitionTrace),
    PipelineCase(pipeline::PipelineCase),
    PipelineTrace(pipeline::PipelineTrace),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateSnapshot {
    pub memory: Vec<Value>,
}
impl From<&StateProcessor> for StateSnapshot {
    fn from(state: &StateProcessor) -> Self {
        Self {
            memory: state.values().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateWrite {
    pub address: Address,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateCase {
    pub name: String,
    pub program: Program,
    pub initial_state: StateSnapshot,
    pub queue_capacity: u32,
    pub steps: Vec<StateStep>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateStep {
    pub writes: Vec<StateWrite>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ExecutionStatus {
    Completed,
    Failed { error: Error },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateResult {
    pub jobs: Vec<Job>,
    pub state: StateSnapshot,
    pub status: ExecutionStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateTrace {
    pub case: StateCase,
    pub results: Vec<StateResult>,
}

/// State writes stand in for settled transitions and host controls. No loop
/// clocks or sample jobs execute here. The first failed step ends the trace.
pub fn run_case(case: StateCase) -> StateTrace {
    let mut state = StateProcessor::new(case.initial_state.memory.clone());
    let mut queue = JobQueue::new(case.queue_capacity as usize);
    let mut results = Vec::with_capacity(case.steps.len());
    for step in &case.steps {
        let outcome = step
            .writes
            .iter()
            .try_for_each(|write| state.write(write.address, write.value))
            .and_then(|()| state.run(&case.program, &mut queue));
        let jobs = queue.iter().copied().collect();
        while queue.pop().is_some() {}
        results.push(StateResult {
            jobs,
            state: StateSnapshot::from(&state),
            status: match outcome {
                Ok(()) => ExecutionStatus::Completed,
                Err(error) => ExecutionStatus::Failed { error },
            },
        });
        if outcome.is_err() {
            break;
        }
    }
    StateTrace { case, results }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioCase {
    pub name: String,
    pub program: Program,
    pub initial_state: StateSnapshot,
    pub queue_capacity: u32,
    pub buffers: u32,
    pub output_ports: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "MediaSnapshot")]
    pub media: Option<MediaSnapshot>,
    pub steps: Vec<AudioStep>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioStep {
    pub writes: Vec<StateWrite>,
    pub inputs: Vec<Vec<Fixed>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<MediaAction>")]
    pub media_actions: Option<Vec<MediaAction>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaAction {
    Clear {
        view: ViewId,
    },
    Remove {
        view: ViewId,
    },
    Slice {
        source: ViewId,
        start: u64,
        frames: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStage {
    Media,
    State,
    Jobs,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioResult {
    pub jobs: Vec<Job>,
    pub remaining_jobs: Vec<Job>,
    pub state: StateSnapshot,
    pub buffers: Vec<Option<Vec<Fixed>>>,
    pub outputs: Vec<Vec<Fixed>>,
    pub stage: ExecutionStage,
    pub status: ExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "MediaSnapshot")]
    pub media: Option<MediaSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioTrace {
    pub case: AudioCase,
    pub results: Vec<AudioResult>,
}

/// Execute both processors against elastic media. Media controls run at drained
/// quantum boundaries. Omitted media preserves the empty-store trace format.
pub fn run_audio_case(case: AudioCase) -> Result<AudioTrace, Error> {
    let quantum = case.program.quantum as usize;
    let mut state = StateProcessor::new(case.initial_state.memory.clone());
    let mut queue = JobQueue::new(case.queue_capacity as usize);
    let mut processor = JobProcessor::new(quantum, case.buffers as usize);
    let mut outputs = vec![vec![Fixed::ZERO; quantum]; case.output_ports as usize];
    let mut memory = match &case.media {
        Some(snapshot) => MediaStore::from_snapshot(snapshot.clone())?,
        None => MediaStore::new(1)?,
    };
    let include_media =
        case.media.is_some() || case.steps.iter().any(|step| step.media_actions.is_some());
    let mut results = Vec::with_capacity(case.steps.len());
    for step in &case.steps {
        let mut stage = ExecutionStage::Media;
        let mut outcome = step
            .media_actions
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .try_for_each(|action| match *action {
                MediaAction::Clear { view } => memory.clear(view),
                MediaAction::Remove { view } => memory.remove(view),
                MediaAction::Slice {
                    source,
                    start,
                    frames,
                } => memory.slice(source, start, frames).map(|_| ()),
            });
        if outcome.is_ok() {
            stage = ExecutionStage::State;
            outcome = step
                .writes
                .iter()
                .try_for_each(|write| state.write(write.address, write.value))
                .and_then(|()| state.run(&case.program, &mut queue));
        }
        let jobs = queue.iter().copied().collect();
        if outcome.is_ok() {
            stage = ExecutionStage::Jobs;
            let inputs: Vec<_> = step.inputs.iter().map(Vec::as_slice).collect();
            let mut output_slices: Vec<_> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
            outcome = processor.run(&mut queue, &mut memory, &inputs, &mut output_slices);
        }
        results.push(AudioResult {
            jobs,
            remaining_jobs: queue.iter().copied().collect(),
            state: StateSnapshot::from(&state),
            buffers: (0..case.buffers)
                .map(|index| {
                    processor
                        .buffer(BufferId(index as usize))
                        .ok()
                        .map(<[Fixed]>::to_vec)
                })
                .collect(),
            outputs: outputs.clone(),
            stage,
            status: match outcome {
                Ok(()) => ExecutionStatus::Completed,
                Err(error) => ExecutionStatus::Failed { error },
            },
            media: include_media.then(|| memory.snapshot()),
        });
        if outcome.is_err() {
            break;
        }
    }
    Ok(AudioTrace { case, results })
}

pub fn schema() -> Schema {
    use schemars::transform::{RecursiveTransform, Transform};
    let mut schema = schemars::schema_for!(Document);
    RecursiveTransform(|schema: &mut Schema| {
        let bounds = match schema.get("format").and_then(serde_json::Value::as_str) {
            Some("uint32") => Some((0.into(), u32::MAX.into())),
            Some("uint64") => Some((0.into(), u64::MAX.into())),
            Some("int32") => Some((i32::MIN.into(), i32::MAX.into())),
            _ => None,
        };
        if let Some((minimum, maximum)) = bounds {
            schema.insert("minimum".into(), minimum);
            schema.insert("maximum".into(), maximum);
        }
    })
    .transform(&mut schema);
    schema
}
