//! Prepared composite contracts shared by the session service and backend adapters.

use crate::{
    BackendCompositeId, BackendCompositeKind, BackendCompositeTarget, BackendLoopId,
    BackendLoopMode,
};
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCompositeLimits {
    pub entries: u32,
    pub segments: u32,
    pub relations: u32,
    pub quantum: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendCompositeSpan {
    pub target: BackendCompositeTarget,
    pub start: u64,
    pub end: u64,
    pub occurrence_start: u64,
    pub ordinal: u32,
    pub mode: BackendLoopMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendPreparedCompositePlan {
    pub kind: BackendCompositeKind,
    pub sync_source: Option<BackendLoopId>,
    pub unit_frames: u64,
    pub iterations: u64,
    pub entries: u32,
    pub spans: Vec<BackendCompositeSpan>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendSessionComposite {
    pub source_id: u64,
    #[serde(default)]
    pub version: u64,
    pub plan: BackendPreparedCompositePlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendCompositePending {
    Queued,
    Staging,
    AwaitingRestart,
    Confirming,
    Cancelling,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendCompositeOutcome {
    Applied { frame: u64 },
    Cancelled,
    Superseded,
    Refused(String),
    ConnectionLost,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendCompositeReceipt {
    pub sequence: u64,
    pub composite: BackendCompositeId,
    pub version: u64,
    pub outcome: BackendCompositeOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCompositeOperationState {
    pub composite: BackendCompositeId,
    pub version: u64,
    pub stage: BackendCompositePending,
}
