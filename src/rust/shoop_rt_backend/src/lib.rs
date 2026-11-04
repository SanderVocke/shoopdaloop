//! The application's `Backend` over the new engine's protocol.
//!
//! [`RtBackend`] owns a [`ProtocolClient`] over any carrier and an [`EngineClock`]; the
//! composition root chooses both, so the adapter does not know where the engine runs. It
//! speaks the application's vocabulary on one side and engine terms (strips, loop slots,
//! host ports, routes) on the other.
//!
//! Every mutation is asynchronous. Identities are reserved on the control side and returned
//! at once; requests are queued and sent together when the backend pumps (`poll`,
//! `advance`, `wait_idle`), so the engine can apply the structural ones of one application
//! tick as one transaction. The outcome confirms the change in the snapshot or reports a
//! mutation failure. `poll` assembles the snapshot from outcomes and
//! telemetry and never waits. `wait_idle` waits through the clock until every request has
//! its outcome and newer telemetry has arrived.
//!
//! Loop modes, positions, lengths and planned transitions in the snapshot describe the state
//! at the start of the last rendered callback, one callback behind the legacy backends, which
//! report the state of the next frame to render. Delays the application derives from a
//! position and the sync loop's length can therefore come out one cycle later when a request
//! falls within one callback of a wrap.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

mod backend;
mod capture;
mod clock;
mod family;
mod import;
mod incoming;
mod mirror;
mod outgoing;
mod persistence;
mod prepared;
mod preview;
mod reads;
mod requests;
mod session;

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub use clock::Sleep;
pub use clock::{EngineClock, NoWait};

use anyhow::{anyhow, Result};
use shoop_app_api::BackendCapability;
use shoop_backend::{
    BackendConnectionFailure, BackendMixerFailure, BackendMutationCompletion,
    BackendMutationFailure, BackendSessionReplacement,
};
use shoop_rt_protocol::{Capabilities, DriverStatus, Limits, StateRecord};
use shoop_rt_transport::carrier::{Carrier, CarrierError};
use shoop_rt_transport::client::{ClientError, ProtocolClient};

use crate::mirror::Mirror;
use crate::reads::Reads;
use crate::requests::Requests;

/// Waits through the clock before `connect` or `wait_idle` gives up.
const MAX_WAITS: usize = 10_000;
pub const MAX_TRACKED_MUTATIONS: usize = 4096;
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
    callbacks: u64,
    load: f32,
    /// The engine's xrun count when last reported, from the first engine record
    /// of this connection on.
    xruns_reported: Option<u32>,
    xruns: u32,
    /// Peaks since the previous snapshot, held while no record arrives.
    input_peak: f32,
    output_peak: f32,
    peaks_fresh: bool,
}

impl Status {
    /// Xruns since the previous call. A count below the last one means the
    /// engine's counter restarted, and all of it is new.
    fn take_xruns(&mut self) -> u32 {
        let Some(reported) = self.xruns_reported else {
            return 0;
        };
        let delta = if self.xruns >= reported {
            self.xruns - reported
        } else {
            self.xruns
        };
        self.xruns_reported = Some(self.xruns);
        delta
    }
}

/// A session replacement whose requests are not all answered yet.
struct Replacement {
    mapping: BackendSessionReplacement,
    candidate: Option<Mirror>,
    job: persistence::Job,
    content: alloc::collections::BTreeMap<
        shoop_backend::BackendLoopId,
        alloc::sync::Arc<shoop_backend::BackendLoopContent>,
    >,
    connection_failures: Vec<BackendConnectionFailure>,
    loop_target: Option<shoop_backend::BackendLoopId>,
    published: bool,
}

/// The application backend over a protocol client. See the crate documentation.
pub struct RtBackend<C, K> {
    profiling: shoop_rt_protocol::profiling::Collector,
    client: ProtocolClient<C>,
    clock: K,
    link: Link,
    capabilities: Capabilities,
    limits: Option<Limits>,
    driver: Option<DriverStatus>,
    driver_profile: Option<shoop_app_api::ResolvedAudioDriverConfig>,
    status: Status,
    mirror: Mirror,
    requests: Requests,
    reads: Reads,
    read_cancel_needed: bool,
    resync: Option<Vec<StateRecord>>,
    mutation_failures: Vec<BackendMutationFailure>,
    mutation_completions: Vec<BackendMutationCompletion>,
    connection_failures: Vec<BackendConnectionFailure>,
    mixer_failures: Vec<BackendMixerFailure>,
    failure_sequence: u64,
    /// Telemetry arrived after the latest outcome.
    telemetry_fresh: bool,
    replacement: Option<Replacement>,
    preview: Option<preview::Preview>,
    preview_playing: u64,
    next_persistence: u64,
    canonical_content: alloc::collections::BTreeMap<
        shoop_backend::BackendLoopId,
        alloc::sync::Arc<shoop_backend::BackendLoopContent>,
    >,
    canonical_imports: alloc::collections::BTreeSet<shoop_backend::BackendLoopId>,
    prepared_content:
        alloc::collections::BTreeMap<shoop_backend::BackendLoopId, prepared::Prepared>,
    capture_validation: Option<prepared::Validation>,
}

pub(crate) fn unavailable(capability: BackendCapability) -> anyhow::Error {
    anyhow!(capability.unavailable_reason())
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    /// Attach host-resolved metadata; runtime telemetry remains authoritative about the active kind.
    pub fn set_driver_profile(&mut self, profile: shoop_app_api::ResolvedAudioDriverConfig) {
        self.driver_profile = Some(profile);
    }

    /// Starts the handshake over an unconnected client. The backend refuses operations until
    /// [`RtBackend::is_ready`]; pumping it, as `poll` does, completes the handshake.
    pub fn begin(mut client: ProtocolClient<C>, clock: K) -> Result<Self> {
        client
            .connect()
            .map_err(|error| anyhow!("could not reach the engine: {error:?}"))?;
        tracing::debug!("backend.rt.connecting");
        Ok(Self {
            profiling: Default::default(),
            client,
            clock,
            link: Link::Connecting,
            capabilities: Capabilities::default(),
            limits: None,
            driver: None,
            driver_profile: None,
            status: Status::default(),
            mirror: Mirror::new(),
            requests: Requests::default(),
            reads: Reads::default(),
            read_cancel_needed: false,
            resync: None,
            mutation_failures: Vec::new(),
            mutation_completions: Vec::new(),
            connection_failures: Vec::new(),
            mixer_failures: Vec::new(),
            failure_sequence: 0,
            telemetry_fresh: false,
            replacement: None,
            preview: None,
            preview_playing: 0,
            next_persistence: 1,
            canonical_content: Default::default(),
            canonical_imports: Default::default(),
            prepared_content: Default::default(),
            capture_validation: None,
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
    /// The confirmed synchronization source of a live loop.
    pub fn loop_sync_source(
        &self,
        id: shoop_backend::BackendLoopId,
    ) -> Option<shoop_backend::BackendLoopId> {
        self.mirror.live_loop(id).and_then(|entry| entry.sync)
    }

    pub fn clock_mut(&mut self) -> &mut K {
        &mut self.clock
    }

    /// Ends the connection. The engine keeps its state; this backend refuses further
    /// operations.
    pub fn disconnect(&mut self) {
        self.client.disconnect();
        self.lose("the application disconnected".to_owned());
        while self.client.poll().is_some() {}
    }

    /// The client and the clock, for example to connect a new backend after
    /// [`RtBackend::disconnect`].
    pub fn into_parts(self) -> (ProtocolClient<C>, K) {
        (self.client, self.clock)
    }

    /// Sends queued requests and reads as credits allow and handles every engine message that
    /// has arrived. Never waits.
    pub fn pump(&mut self) {
        self.reads.advance();
        if let Some(replacement) = self.replacement.as_mut() {
            if let Err(error) = replacement.job.prepare() {
                replacement.job.fail(error.to_string());
            }
        }
        for _ in 0..MAX_PUMP_ROUNDS {
            if self.read_cancel_needed
                && self.is_ready()
                && self.require_mutation_capacity(1).is_ok()
                && !self.read_cancel_outstanding()
            {
                self.requests.push(
                    shoop_rt_protocol::Request::CancelBulk,
                    crate::requests::Purpose::CancelReads,
                );
                self.read_cancel_needed = false;
            }
            self.advance_preview();
            self.advance_persistence();
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
        if self.replacement.as_ref().is_some_and(|replacement| {
            !replacement.job.finished && replacement.job.family.is_none()
        }) {
            return Err(anyhow!("a session replacement is in progress"));
        }
        match &self.link {
            Link::Ready => self.require_mutation_capacity(1),
            Link::Connecting | Link::Resyncing => Err(anyhow!("the engine is not connected yet")),
            Link::Disconnected(reason) => Err(anyhow!("the engine is disconnected: {reason}")),
        }
    }

    fn require_mutation_capacity(&self, count: usize) -> Result<()> {
        let used = self.requests.outstanding() + self.mutation_completions.len();
        if count > MAX_TRACKED_MUTATIONS.saturating_sub(used)
            || self
                .requests
                .last_sequence()
                .checked_add(count as u64)
                .is_none()
        {
            return Err(anyhow!("engine mutation outcome tracking is full"));
        }
        Ok(())
    }

    fn limits(&self) -> Limits {
        self.limits.expect("limits are known once connected")
    }

    fn idle(&self) -> bool {
        match self.link {
            Link::Ready => {
                self.requests.is_empty()
                    && !self.read_cancel_needed
                    && self.reads.idle()
                    && self.telemetry_fresh
                    && self
                        .replacement
                        .as_ref()
                        .is_none_or(|replacement| replacement.job.finished)
            }
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
                    if let shoop_rt_protocol::Request::ContentWrite { id, offset, data } =
                        pending.request
                    {
                        if offset % (1024 * 1024) == 0 {
                            tracing::debug!(
                                correlation,
                                id,
                                offset,
                                bytes = data.as_bytes().len(),
                                "backend.rt.content_write"
                            );
                        }
                    } else {
                        tracing::trace!(correlation, request = ?pending.request, "backend.rt.request");
                    }
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
        if self.read_cancel_needed || self.read_cancel_outstanding() {
            return sent;
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

    fn read_cancel_outstanding(&self) -> bool {
        self.requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
            .any(|pending| pending.purpose == crate::requests::Purpose::CancelReads)
    }

    fn lose(&mut self, reason: String) {
        if matches!(self.link, Link::Disconnected(_)) {
            return;
        }
        tracing::error!(reason = %reason, "backend.rt.disconnected");
        self.link = Link::Disconnected(reason.clone());
        self.profiling = Default::default();
        for pending in self
            .requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
        {
            self.mutation_completions.push(BackendMutationCompletion {
                sequence: pending.sequence,
                result: Err(reason.clone()),
            });
        }
        self.requests.clear();
        self.reads.clear();
        if let Some(preview) = &mut self.preview {
            preview.job.disconnected(reason.clone());
        }
        if let Some(replacement) = &mut self.replacement {
            replacement.job.disconnected(reason);
        }
        self.mirror.connection_revision += 1;
    }
}
