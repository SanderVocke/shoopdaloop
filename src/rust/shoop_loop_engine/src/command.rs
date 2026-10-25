//! Plain-data musical operations; transport ownership stays with the host runtime.

use crate::{Error, Frame, LoopId, RequestId, Revision, Sequence};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Timing {
    At,
    AfterTicks { source: LoopId, skip: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transition {
    PlayIteration {
        iteration: u64,
        retrigger: bool,
    },
    SeekIteration {
        iteration: u64,
        retrigger: bool,
    },
    Play {
        position: u64,
        retrigger: bool,
    },
    Record {
        monitor_previous: bool,
    },
    Stop,
    Seek {
        position: u64,
        retrigger: bool,
    },
    Replace {
        length: u64,
        revision: Revision,
        preserve_position: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub target: LoopId,
    pub sequence: Sequence,
    pub at: Frame,
    pub timing: Timing,
    pub transition: Transition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Applied { frame: Frame },
    Superseded { frame: Frame },
    Cancelled { frame: Frame },
    Failed { frame: Frame, reason: Error },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Completion {
    pub id: RequestId,
    pub sequence: Sequence,
    pub requested: Frame,
    pub effective: Frame,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelResult {
    Cancelled,
    AlreadyTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Record {
    pub id: RequestId,
    pub request: Request,
    pub effective: Frame,
    pub remaining: u64,
    pub terminal: Option<Outcome>,
}
