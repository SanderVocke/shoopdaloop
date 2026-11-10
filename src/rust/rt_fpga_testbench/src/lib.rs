//! Architectural values and quantum traces for software/RTL comparison.
#![forbid(unsafe_code)]

use rt_fpga_reference::{
    compiler::{Binding, Compiled, Graph},
    jobs::{Job, JobQueue},
    state::{Address, Instruction, Program, StateProcessor, Value},
    transitions::OnWrap,
    Error,
};
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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
