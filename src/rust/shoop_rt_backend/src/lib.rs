//! The application's `Backend` over the new engine's protocol.
//!
//! [`RtBackend`] owns a [`ProtocolClient`] over any carrier and an [`EngineClock`]; the
//! composition root chooses both, so the adapter does not know where the engine runs. It
//! speaks the application's vocabulary on one side and engine terms (strips, loop slots,
//! host ports, routes) on the other.
//!
//! Every mutation is asynchronous. Identities are reserved on the control side and returned
//! at once; the request is sent as credits allow, and its outcome confirms the change in the
//! snapshot or reports a mutation failure. `poll` assembles the snapshot from outcomes and
//! telemetry and never waits. `wait_idle` waits through the clock until every request has
//! its outcome and newer telemetry has arrived.
//!
//! Loop modes, positions, lengths and planned transitions in the snapshot describe the state
//! at the start of the last rendered callback, one callback behind the legacy backends, which
//! report the state of the next frame to render. Delays the application derives from a
//! position and the sync loop's length can therefore come out one cycle later when a request
//! falls within one callback of a wrap.
#![forbid(unsafe_code)]

mod backend;
mod clock;
mod incoming;
mod mirror;
mod outgoing;
mod reads;
mod requests;
mod session;

#[cfg(not(target_arch = "wasm32"))]
pub use clock::Sleep;
pub use clock::{EngineClock, NoWait};

use anyhow::{anyhow, Result};
use shoop_app_api::BackendCapability;
use shoop_backend::{
    BackendConnectionFailure, BackendMixerFailure, BackendMutationFailure,
    BackendSessionReplacement,
};
use shoop_rt_protocol::carrier::{Carrier, CarrierError};
use shoop_rt_protocol::client::{ClientError, ProtocolClient};
use shoop_rt_protocol::{Capabilities, DriverStatus, Limits, StateRecord};

use crate::mirror::Mirror;
use crate::reads::Reads;
use crate::requests::Requests;

/// Waits through the clock before `connect` or `wait_idle` gives up.
const MAX_WAITS: usize = 10_000;
/// Rounds of sending and receiving in one pump.
const MAX_PUMP_ROUNDS: usize = 1_000;

#[derive(Clone, Debug, PartialEq)]
enum Link {
    Connecting,
    Resyncing,
    Ready,
    Disconnected(String),
}

#[derive(Clone, Copy, Debug, Default)]
struct Status {
    frame: u64,
    xruns: u32,
    load: f32,
}

/// A session replacement whose requests are not all answered yet.
struct Replacement {
    mapping: BackendSessionReplacement,
    first: u64,
    last: u64,
    refused: Vec<String>,
}

/// The application backend over a protocol client. See the crate documentation.
pub struct RtBackend<C, K> {
    client: ProtocolClient<C>,
    clock: K,
    link: Link,
    capabilities: Capabilities,
    limits: Option<Limits>,
    driver: Option<DriverStatus>,
    status: Status,
    mirror: Mirror,
    requests: Requests,
    reads: Reads,
    resync: Option<Vec<StateRecord>>,
    mutation_failures: Vec<BackendMutationFailure>,
    connection_failures: Vec<BackendConnectionFailure>,
    mixer_failures: Vec<BackendMixerFailure>,
    failure_sequence: u64,
    /// Telemetry arrived after the latest outcome.
    telemetry_fresh: bool,
    replacement: Option<Replacement>,
}

pub(crate) fn unavailable(capability: BackendCapability) -> anyhow::Error {
    anyhow!(capability.unavailable_reason())
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    /// Starts the handshake over an unconnected client. The backend refuses operations until
    /// [`RtBackend::is_ready`]; pumping it, as `poll` does, completes the handshake.
    pub fn begin(mut client: ProtocolClient<C>, clock: K) -> Result<Self> {
        client
            .connect()
            .map_err(|error| anyhow!("could not reach the engine: {error:?}"))?;
        tracing::debug!("backend.rt.connecting");
        Ok(Self {
            client,
            clock,
            link: Link::Connecting,
            capabilities: Capabilities::default(),
            limits: None,
            driver: None,
            status: Status::default(),
            mirror: Mirror::new(),
            requests: Requests::default(),
            reads: Reads::default(),
            resync: None,
            mutation_failures: Vec::new(),
            connection_failures: Vec::new(),
            mixer_failures: Vec::new(),
            failure_sequence: 0,
            telemetry_fresh: false,
            replacement: None,
        })
    }

    /// Connects and waits through the clock until the engine's state is mirrored.
    pub fn connect(client: ProtocolClient<C>, clock: K) -> Result<Self> {
        let mut backend = Self::begin(client, clock)?;
        for _ in 0..MAX_WAITS {
            backend.pump();
            match &backend.link {
                Link::Ready => return Ok(backend),
                Link::Disconnected(reason) => {
                    return Err(anyhow!("the engine refused the connection: {reason}"))
                }
                Link::Connecting | Link::Resyncing => {}
            }
            if !backend.clock.wait() {
                return Err(anyhow!(
                    "the engine did not answer and this environment cannot wait for it"
                ));
            }
        }
        Err(anyhow!("the engine did not complete the connection"))
    }

    /// Whether the handshake and the initial state transfer are complete.
    pub fn is_ready(&self) -> bool {
        self.link == Link::Ready
    }

    pub fn clock(&self) -> &K {
        &self.clock
    }

    pub fn clock_mut(&mut self) -> &mut K {
        &mut self.clock
    }

    /// Ends the connection. The engine keeps its state; this backend refuses further
    /// operations.
    pub fn disconnect(&mut self) {
        self.client.disconnect();
        self.lose("the application disconnected".to_owned());
    }

    /// Sends queued requests and reads as credits allow and handles every engine message that
    /// has arrived. Never waits.
    pub fn pump(&mut self) {
        for _ in 0..MAX_PUMP_ROUNDS {
            let sent = self.send();
            let mut received = 0;
            while let Some(message) = self.client.poll() {
                self.handle(message);
                received += 1;
            }
            if sent == 0 && received == 0 {
                break;
            }
        }
    }

    fn ready(&self) -> Result<()> {
        match &self.link {
            Link::Ready => Ok(()),
            Link::Connecting | Link::Resyncing => Err(anyhow!("the engine is not connected yet")),
            Link::Disconnected(reason) => Err(anyhow!("the engine is disconnected: {reason}")),
        }
    }

    fn limits(&self) -> Limits {
        self.limits.expect("limits are known once connected")
    }

    fn idle(&self) -> bool {
        match self.link {
            Link::Ready => self.requests.is_empty() && self.reads.idle() && self.telemetry_fresh,
            Link::Disconnected(_) => true,
            Link::Connecting | Link::Resyncing => false,
        }
    }

    pub(crate) fn wait_until_idle(&mut self) {
        for _ in 0..MAX_WAITS {
            self.pump();
            if self.idle() {
                return;
            }
            if !self.clock.wait() {
                return;
            }
        }
        tracing::warn!(
            outstanding = self.requests.outstanding(),
            "backend.rt.wait_idle_timeout"
        );
    }

    fn send(&mut self) -> usize {
        if self.link != Link::Ready {
            return 0;
        }
        let mut sent = 0;
        while let Some(pending) = self.requests.outgoing.front() {
            match self.client.request(pending.request) {
                Ok(correlation) => {
                    let pending = self.requests.outgoing.pop_front().expect("front exists");
                    tracing::trace!(
                        correlation,
                        request = ?pending.request,
                        "backend.rt.request"
                    );
                    self.requests.in_flight.insert(correlation, pending);
                    sent += 1;
                }
                Err(ClientError::Busy | ClientError::Carrier(CarrierError::Full)) => break,
                Err(error) => {
                    self.lose(format!("sending failed: {error:?}"));
                    return sent;
                }
            }
        }
        while let Some((loop_id, midi, read)) = self.reads.next() {
            match self.client.bulk(read) {
                Ok(()) => {
                    self.reads.sent(loop_id, midi, read.id);
                    sent += 1;
                }
                Err(ClientError::Busy | ClientError::Carrier(CarrierError::Full)) => break,
                Err(error) => {
                    self.reads
                        .fail(loop_id, midi, format!("the read failed: {error:?}"));
                    break;
                }
            }
        }
        sent
    }

    fn lose(&mut self, reason: String) {
        if matches!(self.link, Link::Disconnected(_)) {
            return;
        }
        tracing::error!(reason = %reason, "backend.rt.disconnected");
        self.link = Link::Disconnected(reason.clone());
        self.requests.clear();
        self.reads.clear();
        if let Some(replacement) = &mut self.replacement {
            replacement.refused.push(reason);
        }
        self.mirror.connection_revision += 1;
    }
}
