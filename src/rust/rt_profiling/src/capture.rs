use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub const MAX_CAPTURE_TRACES: usize = 8192;
pub const DETAIL_NODES: usize = 256;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceSummary {
    pub sequence: u64,
    pub callback: u64,
    pub frame: u64,
    pub callback_frame: u64,
    pub epoch: u64,
    pub frames: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub budget_ns: u64,
    pub flags: u32,
}
impl TraceSummary {
    pub(crate) fn from_packet(p: crate::Packet) -> Self {
        Self {
            sequence: p.sequence,
            callback: p.callback,
            frame: p.frame,
            callback_frame: p.callback_frame,
            epoch: p.epoch,
            frames: p.frames,
            start_ns: p.start_ns,
            end_ns: p.end_ns,
            budget_ns: p.budget_ns,
            flags: p.flags,
        }
    }
    pub fn contains(&self, other: &Self) -> bool {
        self.start_ns <= other.start_ns && other.end_ns <= self.end_ns && self.epoch == other.epoch
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub connection: u64,
    pub generation: u64,
    pub capture: u64,
    pub callback: u64,
    pub tick: u64,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeTiming {
    pub key: u64,
    pub start_ns: u64,
    pub end_ns: u64,
    pub ready_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    pub phase: u64,
    pub at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CapturePage {
    pub request: CaptureRequest,
    pub callback: TraceSummary,
    pub tick: Option<TraceSummary>,
    pub nodes: Vec<NodeTiming>,
    pub phases: Vec<Boundary>,
    pub total: u32,
    pub incomplete: bool,
}
