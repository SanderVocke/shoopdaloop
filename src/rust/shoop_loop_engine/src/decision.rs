//! Immutable interval decisions and synthetic boundary observations.

use crate::command::Transition;
use crate::state::State;
use crate::{Error, Frame, LoopId, RequestId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interval {
    pub start: Frame,
    pub end: Frame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decision {
    pub state: State,
    pub origin: Option<RequestId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    CaptureExhausted { target: LoopId },
    Refused { request: RequestId, reason: Error },
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
