//! Architectural image shared by runtime lowering and behavioral fixtures.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Playback { target: Identity },
    Capture { target: Identity },
    Gain { channels: u16, initial: i32 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub format_version: u32,
    pub name: String,
    pub transition: TransitionImage,
    pub program: Program,
    pub initial_state: Memory,
    pub buffers: u32,
    pub input_ports: u16,
    pub output_ports: u16,
    pub media: MediaImage,
    pub loop_views: Vec<LoopViews>,
    pub gains: Vec<GainBinding>,
    pub node_order: Vec<Identity>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionImage {
    pub description: Description,
    pub limits: Limits,
    pub dependency_order: Vec<Identity>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub quantum: u32,
    pub loops: Vec<LoopSpec>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopSpec {
    pub id: Identity,
    pub kind: String,
    pub length: u64,
    pub sync_source: Option<Identity>,
    pub repeat_sync: bool,
    pub timeline: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub loop_slots: u32,
    pub pending_requests: u32,
    pub events: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub quantum: u32,
    pub chunk_frames: u32,
    pub jobs_per_quantum: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub memory: Vec<Value>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Word(u64),
    Ramp(Ramp),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    pub start: i32,
    pub target: i32,
    pub duration: u32,
    pub elapsed: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[repr(u16)]
pub enum ArithmeticMode {
    Multiply = 0,
    LeftBalance = 1,
    RightBalance = 2,
    OneMinus = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Instruction {
    ProcessSmoothing {
        loop_id: Identity,
        control: u16,
        source: u16,
    },
    ProcessConstant {
        value: i32,
        destination: u16,
    },
    ProcessArithmetic {
        left: u16,
        right: u16,
        destination: u16,
        mode: ArithmeticMode,
    },
    ProcessInput {
        port: u16,
        destination: u16,
    },
    ProcessLogicalPlayback {
        loop_id: Identity,
        view: u16,
        destination: u16,
    },
    ProcessLogicalCapture {
        loop_id: Identity,
        view: u16,
        source: u16,
    },
    ProcessGain {
        ramp: u16,
        source: u16,
        destination: u16,
    },
    ProcessMix {
        left: u16,
        right: u16,
        destination: u16,
    },
    ProcessOutput {
        source: u16,
        port: u16,
    },
    ProcessSilence {
        destination: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaImage {
    pub chunk_frames: u32,
    pub samples: Vec<i32>,
    pub free_chunks: Vec<u32>,
    pub views: Vec<View>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub chunks: Vec<u32>,
    pub frames: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopViews {
    pub id: Identity,
    pub views: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GainBinding {
    pub id: Identity,
    pub ramps: Vec<u16>,
}
