//! Architectural logical-loop values. No audio-node or sample-store identities.

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        #[derive(serde::Serialize, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        #[derive(schemars::JsonSchema)]
        pub struct $name {
            pub slot: u32,
            pub generation: u32,
        }
    };
}
identity!(LoopId);
identity!(RequestId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum TransitionError {
    InvalidConfig,
    Capacity,
    InvalidId,
    DuplicateId,
    InvalidState,
    InvalidFrame,
    MissingSync,
    Exhausted,
    EmptyContent,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum Mode {
    Stopped,
    Playing,
    Recording,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub enum Operation {
    Stop,
    Play { position: u64, retrigger: bool },
    Record,
    Seek { position: u64, retrigger: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub enum Timing {
    At,
    AfterTicks { source: LoopId, skip: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct Command {
    pub target: LoopId,
    pub sequence: u64,
    pub at: u64,
    pub timing: Timing,
    pub operation: Operation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub enum Outcome {
    Applied { frame: u64 },
    Superseded { frame: u64 },
    Cancelled { frame: u64 },
    Failed { frame: u64, reason: TransitionError },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct Completion {
    pub id: RequestId,
    pub sequence: u64,
    pub requested: u64,
    pub effective: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct LoopState {
    pub id: LoopId,
    pub mode: Mode,
    pub content_length: u64,
    pub position: u64,
    pub capture_frames: u64,
    pub cycles: u64,
    pub content_revision: u64,
    pub capture_revision: u64,
    pub revision_watermark: u64,
    pub last_pulse: Option<u64>,
    pub repeat_sync: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(schemars::JsonSchema)]
pub enum EventKind {
    Transition(Operation),
    Wrap,
    Pulse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct Event {
    pub frame: u64,
    pub target: LoopId,
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct RequestSnapshot {
    pub id: RequestId,
    pub command: Command,
    pub effective: u64,
    pub remaining_ticks: Option<u64>,
    pub outcome: Option<Outcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum LoopKind {
    Primitive,
    Composite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct LoopSpec {
    pub id: LoopId,
    pub kind: LoopKind,
    pub length: u64,
    pub sync_source: Option<LoopId>,
    pub repeat_sync: bool,
    pub timeline: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct PlanDescription {
    pub quantum: u32,
    pub loops: Vec<LoopSpec>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct TransitionLimits {
    pub loop_slots: u32,
    pub pending_requests: u32,
    pub events: u32,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(schemars::JsonSchema)]
pub struct TransitionPlanImage {
    pub format_version: u32,
    pub description: PlanDescription,
    pub limits: TransitionLimits,
    pub dependency_order: Vec<LoopId>,
}
