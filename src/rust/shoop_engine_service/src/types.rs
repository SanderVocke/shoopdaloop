use crate::{dsp, Client, Render};
use rt_runtime::{
    executor,
    graph::PreparedPlan,
    transport::{Lane, Outcome},
    Epoch, Error, Frame,
};
use shoop_loop_engine::{
    runtime::{LoopPlanner, Node, Operation, Reply},
    LoopId,
};
use std::sync::Arc;

pub const NORMAL_SLOTS: usize = 16;
pub const DOMAIN_SLOTS: usize = 8;
pub const BULK_SLOTS: usize = 8;
pub const RECOVERY_SLOTS: usize = 1;
pub(crate) const USER_SLOTS: usize = NORMAL_SLOTS + RECOVERY_SLOTS;
pub(crate) const INTERNAL_SLOT: usize = USER_SLOTS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureTap {
    PostDsp,
    Live,
    DryWet,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub max_frames: usize,
    pub resource_pages: usize,
    pub export_slots: usize,
    pub capture: CaptureTap,
    pub dsp: dsp::Config,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_frames: 64,
            resource_pages: 4096,
            export_slots: 4,
            capture: CaptureTap::PostDsp,
            dsp: dsp::Config::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InitialMidi {
    pub frame: u64,
    pub bytes: Vec<u8>,
}

/// Owned startup content. MIDI state is strictly before the first original packet.
#[derive(Clone, Debug)]
pub struct InitialContent {
    pub audio: [Vec<f32>; 2],
    pub midi: Vec<InitialMidi>,
    pub midi_initial: rt_runtime::midi_state::MidiState,
}

pub struct Prepared {
    pub client: Client,
    pub render: Render,
    pub route: PreparedRoute,
    pub target: LoopId,
}

pub struct PreparedRoute {
    pub(crate) plan: PreparedPlan,
    pub(crate) identity: Arc<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(pub(crate) u64);
impl RequestId {
    pub fn get(self) -> u64 {
        self.0
    }
}

pub enum Request {
    Loop {
        at: Frame,
        operation: Operation,
    },
    Read {
        cursor: rt_runtime::resource::Cursor,
        bytes: Box<[u8]>,
    },
    StageRoute {
        route: PreparedRoute,
        expected_epoch: Epoch,
    },
    ReleaseRoute {
        id: rt_runtime::graph::activation::StageId,
    },
    Cancel {
        target: RequestId,
    },
    Stop,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Loop { .. } => "Loop",
            Self::Read { .. } => "Read",
            Self::StageRoute { .. } => "StageRoute",
            Self::ReleaseRoute { .. } => "ReleaseRoute",
            Self::Cancel { .. } => "Cancel",
            Self::Stop => "Stop",
        })
    }
}

pub type EngineCommand = executor::Command<Node<dsp::Dsp>, LoopPlanner>;
pub type Retired = executor::Retired<Node<dsp::Dsp>>;
pub type LoopExecution = rt_runtime::schedule::Execution<Operation, Reply>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryPhase {
    Runtime,
    ForwardRefused,
    CancelledBeforeForward,
    ServiceHandled,
}

/// Owning return, not a settled loop/DSP state transaction.
pub struct Delivery {
    pub id: RequestId,
    pub lane: Lane,
    pub phase: DeliveryPhase,
    pub frame: Frame,
    pub epoch: Epoch,
    pub outcome: Outcome,
    /// No separate semantic execution is expected when this is true.
    pub terminal: bool,
    pub request: Option<Request>,
    pub command: Option<EngineCommand>,
    pub retired: Option<Retired>,
}

pub enum ExecutionStatus {
    Executed(LoopExecution),
    /// Explicit service teardown/fault; never manufactured as a loop execution.
    Aborted {
        decision_frame: Frame,
        error: Error,
    },
}
pub struct Execution {
    /// None denotes the separately reserved render-owned disconnect stop.
    pub id: Option<RequestId>,
    pub status: ExecutionStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Running,
    Disconnecting,
    Quiescent,
    Faulted(Error),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub forwarded: usize,
    pub owning_returns: usize,
    pub executions: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub next_frame: Frame,
    pub phase: Phase,
    pub recovery_packets: usize,
    pub work: Work,
}
