//! Candidate ABI-16 codecs, bounded transport and reset-bound program preparation.
mod command;
mod device;
mod preparation;
mod result;
mod snapshot;

pub use command::{BoundTarget, CommandRecord, GroupHeader, GroupIdentity, GroupPacket};
pub use device::{
    AllocationOwnership, Device, DeviceRecovery, MAX_PREPARATION_BUS_OPS, MAX_PUBLICATION_BUS_OPS,
    MAX_RESULT_BUS_OPS, MAX_SNAPSHOT_BUS_OPS, PreparationProgress, ProgramRecovery, ProgramSession,
    PublicationStep, ResultStep, SNAPSHOT_COPY_WORDS, SelectionStep, SnapshotBuffers,
    SnapshotDevice, SnapshotStep,
};
pub use preparation::{Preparation, PreparationStep};
pub use result::{GroupResult, ResultHeader, SwitchOutcome};
pub use snapshot::{ObservationRow, Snapshot, SnapshotFormat, SnapshotHeader, SnapshotPhase};

use super::semantic_registers::*;

pub const HEADER_WORDS: usize = 16;
pub const COMMAND_WORDS: usize = 12;
pub const MAX_COMMANDS: usize = SEMANTIC_COMMANDS_PER_GROUP as usize;
pub const MAX_GROUP_WORDS: usize = HEADER_WORDS + MAX_COMMANDS * COMMAND_WORDS;
pub const MAX_RESULT_WORDS: usize = HEADER_WORDS + MAX_COMMANDS;
pub const ROW_WORDS: usize = SEMANTIC_ROW_WORDS as usize * 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    Length,
    Format,
    Identity,
    Binding,
    Refusal,
    Geometry,
}
impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl core::error::Error for CodecError {}

fn join(words: &[u32]) -> u64 {
    u64::from(words[0]) | (u64::from(words[1]) << 32)
}
fn split(value: u64) -> [u32; 2] {
    [value as u32, (value >> 32) as u32]
}
