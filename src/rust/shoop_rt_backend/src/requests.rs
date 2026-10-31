//! Requests waiting for credits and requests waiting for their outcome.

use std::collections::{BTreeMap, VecDeque};

use shoop_backend::{
    BackendBusControl, BackendLoopId, BackendMixerLink, BackendPortId, BackendTrackControl,
    BackendTrackId,
};
use shoop_rt_protocol::Request;

/// What a request is for, so that its outcome updates the mirror or becomes a failure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Purpose {
    CreateStrip(BackendTrackId),
    RemoveStrip(BackendTrackId),
    CreateLoop(BackendLoopId),
    StripControl(BackendTrackId, BackendTrackControl),
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
    BusControl(BackendBusControl),
    InjectMidi(BackendTrackId),
    Smoothing(u32),
    /// Removal of engine structure the application does not know.
    Cleanup,
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
    pub fn push(&mut self, request: Request, purpose: Purpose) -> u64 {
        self.next_sequence += 1;
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
            .all(|pending| pending.sequence > sequence)
    }

    pub fn clear(&mut self) {
        self.outgoing.clear();
        self.in_flight.clear();
    }
}
