//! Immutable interval decisions and synthetic boundary observations.

use crate::command::Transition;
use crate::state::State;
use crate::{Error, Frame, LoopId, RequestId, Revision};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interval {
    pub start: Frame,
    pub end: Frame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decision {
    pub state: State,
    /// Restart media DSP/state at this boundary, independently of journal space.
    /// False when the final state has no playback; consumers then stop/clean up.
    pub playback_discontinuity: bool,
    pub origin: Option<RequestId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    /// Close the source window normally and keep the current media until its tail settles.
    DelayCapture {
        target: LoopId,
    },
    /// The delayed take has reached its previously settled source cut.
    CaptureReady {
        target: LoopId,
        revision: Revision,
    },
    /// Discard the candidate capture and retain previously committed media.
    CaptureAborted {
        target: LoopId,
        reason: Error,
    },
    CaptureExhausted {
        target: LoopId,
    },
    /// Seal the captured prefix as current content and mark it incomplete.
    CapturePrefixExhausted {
        target: LoopId,
    },
    /// Refuse capture starts/restarts for this primitive at the preview boundary.
    /// Use `Refused` instead when an entire request-origin family must fail.
    CaptureUnavailable {
        target: LoopId,
        reason: Error,
    },
    Refused {
        request: RequestId,
        reason: Error,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventKind {
    Transition(Transition),
    Wrap,
    Pulse,
    CaptureExhausted,
    Iteration { iteration: u64, passes: u64 },
    ProgramEnd,
    Refused(Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub frame: Frame,
    pub target: LoopId,
    pub kind: EventKind,
}
