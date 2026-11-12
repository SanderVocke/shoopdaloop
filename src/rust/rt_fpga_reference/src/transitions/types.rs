//! Architectural logical-loop values. No audio-node or sample-store identities.

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub struct $name {
            pub slot: u32,
            pub generation: u32,
        }
    };
}
identity!(LoopId);
identity!(RequestId);

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
pub enum Timing {
    At,
    AfterTicks { source: LoopId, skip: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Command {
    pub target: LoopId,
    pub sequence: u64,
    pub at: u64,
    pub timing: Timing,
    pub operation: Operation,
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
pub struct Completion {
    pub id: RequestId,
    pub sequence: u64,
    pub requested: u64,
    pub effective: u64,
    pub outcome: Outcome,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RequestSnapshot {
    pub id: RequestId,
    pub command: Command,
    pub effective: u64,
    pub remaining_ticks: Option<u64>,
    pub outcome: Option<Outcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Record {
    pub id: RequestId,
    pub command: Command,
    pub effective: u64,
    pub remaining: u64,
    pub outcome: Option<Outcome>,
}
impl Record {
    pub fn snapshot(self) -> RequestSnapshot {
        RequestSnapshot {
            id: self.id,
            command: self.command,
            effective: self.effective,
            outcome: self.outcome,
            remaining_ticks: if self.outcome.is_none()
                && matches!(self.command.timing, Timing::AfterTicks { .. })
            {
                Some(self.remaining)
            } else {
                None
            },
        }
    }
    pub fn completion(self) -> Option<Completion> {
        self.outcome.map(|outcome| Completion {
            id: self.id,
            sequence: self.command.sequence,
            requested: self.command.at,
            effective: self.effective,
            outcome,
        })
    }
}
