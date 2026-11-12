//! Logical sessions are shared inputs; hardware addresses appear only in images.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub format_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Arithmetic::is_default")]
    pub arithmetic: Arithmetic,
    pub quantum: u32,
    pub chunk_frames: u32,
    pub input_ports: u16,
    pub output_ports: u16,
    pub request_capacity: u16,
    pub loops: Vec<Loop>,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
    pub media: Vec<Media>,
    pub steps: Vec<Step>,
}
pub use rt_fpga_adapter::image::Identity;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Loop {
    pub id: Identity,
    pub channels: u16,
    pub sync_source: Option<Identity>,
    pub repeat_sync: bool,
    pub content: Option<Content>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub media: String,
    pub start_chunks: u64,
    pub length_chunks: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub name: String,
    pub channels: Vec<Vec<i32>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: Identity,
    pub operation: NodeKind,
}
pub use rt_fpga_adapter::image::NodeKind;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub node: Option<Identity>,
    pub port: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub source: Endpoint,
    pub destination: Endpoint,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub inputs: Vec<Vec<i32>>,
    pub actions: Vec<Action>,
    pub collect_completed: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Transition {
        name: String,
        target: Identity,
        at: u64,
        timing: Timing,
        operation: Transition,
    },
    Gain {
        name: String,
        node: Identity,
        at: u64,
        target: i32,
        duration: u32,
    },
    Cancel {
        name: String,
        request: String,
    },
    Export {
        name: String,
        target: Identity,
        selection: Selection,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Timing {
    At,
    AfterTicks { source: Identity, skip: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Transition {
    Stop,
    Record,
    Play { position: u64, retrigger: bool },
    Seek { position: u64, retrigger: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    Playback,
    Latest,
    Recording,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Numeric {
    Float,
    Q824,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "encoding",
    content = "channels",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Samples {
    Float(
        #[serde(
            deserialize_with = "finite_channels",
            serialize_with = "serialize_finite_channels"
        )]
        Vec<Vec<f32>>,
    ),
    Q824(Vec<Vec<i32>>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Result {
    pub format_version: u32,
    pub name: String,
    pub numeric: Numeric,
    pub quanta: Vec<QuantumResult>,
    pub media: Vec<MediaResult>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuantumResult {
    pub frame: u64,
    pub next_frame: u64,
    pub outputs: Samples,
    pub states: Vec<LoopState>,
    pub outcomes: Vec<ActionResult>,
    pub events: Vec<Event>,
    pub pending: Vec<PendingRequest>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopState {
    pub id: Identity,
    pub mode: String,
    pub content_length: u64,
    pub position: u64,
    pub capture_frames: u64,
    pub cycles: u64,
    pub content_revision: u64,
    pub capture_revision: u64,
    pub last_pulse: Option<u64>,
    pub repeat_sync: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActionResult {
    pub name: String,
    pub requested: u64,
    pub delivered: Option<u64>,
    pub terminal: u64,
    pub outcome: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaResult {
    pub name: String,
    pub frames: u64,
    pub samples: Samples,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub frame: u64,
    pub target: Identity,
    pub kind: EventKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EventKind {
    Transition(Transition),
    Wrap,
    Pulse,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PendingRequest {
    pub name: String,
    pub requested: u64,
    pub effective: u64,
    pub remaining_ticks: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Arithmetic {
    #[default]
    Bounded,
    IntentionalClipping,
}
impl Arithmetic {
    pub fn is_default(&self) -> bool {
        *self == Self::Bounded
    }
}

fn finite_channels<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<Vec<f32>>, D::Error> {
    let values = Vec::<Vec<f32>>::deserialize(deserializer)?;
    if values.iter().flatten().any(|v| !v.is_finite()) {
        return Err(serde::de::Error::custom("nonfinite float sample"));
    }
    Ok(values)
}
fn serialize_finite_channels<S: serde::Serializer>(
    values: &Vec<Vec<f32>>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if values.iter().flatten().any(|v| !v.is_finite()) {
        return Err(serde::ser::Error::custom("nonfinite float sample"));
    }
    values.serialize(serializer)
}
