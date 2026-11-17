//! Architectural logical-loop values. No audio-node or sample-store identities.

pub use crate::records::Identity as LoopId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    InstantaneousDependencyCycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Mode {
    Stopped,
    Playing,
    Recording,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Operation {
    Stop,
    Play { position: u64, retrigger: bool },
    Record,
    Seek { position: u64, retrigger: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Outcome {
    Applied { frame: u64 },
    Superseded { frame: u64 },
    Cancelled { frame: u64 },
    Failed { frame: u64, reason: TransitionError },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
impl LoopState {
    pub fn plays(self) -> bool {
        self.mode == Mode::Playing
            && self.content_length != 0
            && self.position < self.content_length
    }
    pub fn at_end(self) -> bool {
        self.mode == Mode::Playing
            && self.content_length != 0
            && self.position == self.content_length
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EventKind {
    Transition(Operation),
    Wrap,
    Pulse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Event {
    pub frame: u64,
    pub target: LoopId,
    pub kind: EventKind,
}
