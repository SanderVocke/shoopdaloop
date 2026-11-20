//! Explicit read-only inputs. No scheduling or logical clock advancement.
use crate::Error;
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Identity {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Mode {
    Stopped,
    Playing,
    Recording,
    Invalid,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LoopRecord {
    pub id: Identity,
    pub mode: Mode,
    pub position: u64,
    pub content_length: u64,
    pub begin_capture: bool,
}
impl LoopRecord {
    pub fn plays(self) -> bool {
        self.mode == Mode::Playing
    }
}
#[derive(Clone, Copy)]
pub struct SettledRecords<'a>(pub &'a [LoopRecord]);
impl SettledRecords<'_> {
    pub fn read(self, id: Identity) -> Result<LoopRecord, Error> {
        self.0
            .iter()
            .find(|r| r.id == id && r.mode != Mode::Invalid)
            .copied()
            .ok_or(Error::InvalidState)
    }
}
