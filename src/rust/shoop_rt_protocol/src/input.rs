//! Bounded input-edit transactions and their retained request identity.

use crate::{LoopMode, LoopSlotId, Refusal};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputIntent {
    Mode {
        mode: LoopMode,
        position: Option<u64>,
    },
    Grab {
        sync: Option<LoopSlotId>,
        reverse_start_cycle: Option<u32>,
        cycles_length: Option<u32>,
        go_to_cycle: Option<u32>,
        mode: LoopMode,
        /// Complete prefix plus continuation capacity; zero selects available reserved
        /// capacity for Recording and is required for other modes.
        maximum_frames: u64,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEdit {
    pub loop_id: LoopSlotId,
    pub intent: InputIntent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPhase {
    Receiving,
    Preparing,
    Ready,
    Committing,
    Cancelling,
    Applied,
    Cancelled,
    Failed(Refusal),
}
impl InputPhase {
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Applied | Self::Cancelled | Self::Failed(_))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputProgress {
    pub id: u64,
    pub phase: InputPhase,
    pub received: u32,
    pub expected: u32,
    pub requested_frame: Option<u64>,
    pub effective_frame: Option<u64>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputCapacity {
    pub retention_frames: u64,
    pub writer_slots: u32,
    pub events_per_source: u32,
    pub bytes_per_source: u64,
    pub events_per_tick: u32,
    pub bytes_per_tick: u32,
    pub maximum_packet: u32,
}
