//! Resolver-owned positions, not renderer-discovered peer state.

use crate::{Frame, LoopId, Mode, Revision};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompositeState {
    pub iteration: u64,
    pub passes: u64,
    pub entered_at: Option<Frame>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct State {
    pub id: LoopId,
    pub mode: Mode,
    pub content_length: u64,
    pub content_revision: Revision,
    pub position: u64,
    pub cycles: u64,
    pub capture_frames: u64,
    pub capture_revision: Revision,
    pub capture_incomplete: bool,
    pub monitor_previous: bool,
    pub last_pulse: Option<Frame>,
    pub composite: Option<CompositeState>,
    pub(crate) revision_watermark: Revision,
}

impl State {
    pub(crate) fn new(id: LoopId, content_length: u64) -> Self {
        Self {
            id,
            mode: Mode::Stopped,
            content_length,
            content_revision: Revision(0),
            position: 0,
            cycles: 0,
            capture_frames: 0,
            capture_revision: Revision(0),
            capture_incomplete: false,
            monitor_previous: false,
            last_pulse: None,
            composite: None,
            revision_watermark: Revision(0),
        }
    }

    pub fn plays(self) -> bool {
        self.composite.is_none()
            && self.content_length != 0
            && (self.mode == Mode::Playing
                || (self.mode == Mode::Recording && self.monitor_previous))
    }
}
