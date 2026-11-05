//! Requests waiting for credits and requests waiting for their outcome.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;

use shoop_backend::{
    BackendBusControl, BackendBusId, BackendLoopId, BackendMixerLink, BackendPortId,
    BackendTrackControl, BackendTrackId,
};
use shoop_rt_protocol::Request;

/// What a request is for, so that its outcome updates the mirror or becomes a failure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Purpose {
    Composite(u64),
    CompositeControl(u64),
    RepeatSync(BackendLoopId, bool),
    RemoveComposite(BackendLoopId),
    ResetComposite(BackendLoopId, shoop_rt_protocol::LoopSlotId),
    CreateBus(BackendBusId),
    RemoveBus(BackendBusId),
    CreateStrip(BackendTrackId),
    RemoveStrip(BackendTrackId),
    CreateLoop(BackendLoopId),
    StripControl(BackendTrackId, BackendTrackControl),
    AutomaticMonitoring(BackendTrackId, bool),
    LoopSync(BackendLoopId, Option<BackendLoopId>),
    Transition(BackendLoopId),
    Clear(BackendLoopId),
    LoopGain(BackendLoopId, f32),
    LoopBalance(BackendLoopId, f32),
    Connect {
        port: BackendPortId,
        host: String,
        connected: bool,
    },
    Route {
        link: BackendMixerLink,
        connected: bool,
    },
    BusControl(BackendBusId, BackendBusControl),
    InjectMidi(BackendTrackId),
    Smoothing(u32),
    Profiling,
    Preview,
    Persistence,
    CancelReads,
}

#[derive(Clone, Debug)]
pub(crate) struct Pending {
    pub sequence: u64,
    pub request: Request,
    pub purpose: Purpose,
}

#[derive(Default)]
pub(crate) struct Requests {
    /// Not yet sent, in order.
    pub outgoing: VecDeque<Pending>,
    /// Sent, by correlation.
    pub in_flight: BTreeMap<u64, Pending>,
    next_sequence: u64,
}

impl Requests {
    pub fn reserve_mutation(&mut self) -> u64 {
        self.next_sequence += 1;
        self.next_sequence
    }

    pub fn push_internal(&mut self, request: Request, purpose: Purpose) {
        self.outgoing.push_back(Pending {
            sequence: 0,
            request,
            purpose,
        });
    }

    pub fn push(&mut self, request: Request, purpose: Purpose) -> u64 {
        self.reserve_mutation();
        self.outgoing.push_back(Pending {
            sequence: self.next_sequence,
            request,
            purpose,
        });
        self.next_sequence
    }

    pub fn is_empty(&self) -> bool {
        self.outgoing.is_empty() && self.in_flight.is_empty()
    }

    pub fn outstanding(&self) -> usize {
        self.outgoing.len() + self.in_flight.len()
    }

    /// The sequence of the most recently pushed request.
    pub fn last_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Whether every request up to and including `sequence` has its outcome.
    pub fn answered_through(&self, sequence: u64) -> bool {
        self.outgoing
            .iter()
            .chain(self.in_flight.values())
            .all(|pending| {
                pending.sequence == 0
                    || pending.purpose == Purpose::Preview
                    || pending.sequence > sequence
            })
    }

    pub fn clear(&mut self) {
        self.outgoing.clear();
        self.in_flight.clear();
    }
}
