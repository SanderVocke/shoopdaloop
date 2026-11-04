//! Bounded, precompiled composite programs. Preparation belongs to the client;
//! the engine validates every span and installs one owning version atomically.

use crate::{LoopMode, LoopSlotId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositeKind {
    Regular,
    Explicit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompositeDescription {
    pub id: u64,
    pub loop_id: LoopSlotId,
    pub sync: Option<LoopSlotId>,
    /// An empty primitive being converted in the same physical slot. Conversion
    /// uses a fresh generation; ordinary edits leave this absent.
    pub previous: Option<LoopSlotId>,
    pub expected_version: u64,
    pub version: u64,
    pub kind: CompositeKind,
    pub unit_frames: u64,
    pub iterations: u64,
    pub entries: u32,
    pub segments: u32,
    pub sample_rate: u32,
    pub quantum: u32,
}

/// Spans are ordered by target identity, then start, without overlaps per target.
/// Positions are in prepared units. Contiguous spans of the same mode are merged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompositeSpan {
    pub target: LoopSlotId,
    pub start: u64,
    pub end: u64,
    pub occurrence_start: u64,
    pub ordinal: u32,
    pub mode: LoopMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositeCommand {
    Begin(CompositeDescription),
    Span {
        id: u64,
        index: u32,
        span: CompositeSpan,
    },
    Commit {
        id: u64,
    },
    Cancel {
        id: u64,
    },
}
