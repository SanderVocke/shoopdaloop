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

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

mod backend;
mod capture;
mod clock;
mod composites;
mod family;
mod import;
mod incoming;
mod input;
mod mirror;
mod outgoing;
mod persistence;
mod prepared;
mod preview;
mod processor_controls;
mod processors;
mod reads;
mod requests;
mod session;
mod timing;

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
const MAX_PUMP_ROUNDS: usize = 8;
const MAX_MESSAGES_PER_PUMP_ROUND: usize = 64;

#[derive(Clone, Debug, PartialEq)]
enum Link {
    Connecting,
    Resyncing,
    Ready,
    Disconnected(String),
}

#[derive(Clone, Copy, Debug, Default)]
struct Status {
    memory: Option<shoop_rt_protocol::MemoryReport>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
/// A persistent physical identity bound to one prepared engine port.
pub struct HostPortAlias {
    pub slot: alloc::string::String,
    pub id: alloc::string::String,
    pub name: alloc::string::String,
    pub available: bool,
}

/// Polled on the control thread; updates replace the complete alias inventory.
pub trait HostPortInventory: Send {
    fn poll(&mut self) -> Option<Vec<HostPortAlias>>;
}

fn profiling_connection() -> Option<u64> {
    use core::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .ok()
        .map(u64::from)
}

/// The application backend over a protocol client. See the crate documentation.
pub struct RtBackend<C, K> {
    host_port_inventory: Option<alloc::boxed::Box<dyn HostPortInventory>>,
    inventory_reconcile: bool,
    retired_host_ports:
        alloc::collections::BTreeMap<shoop_rt_protocol::HostPortId, alloc::string::String>,
    resource_limits: shoop_app_api::HostResourceLimits,
    profiling: rt_profiling::Collector,
    profiling_groups: (
        Option<alloc::sync::Arc<rt_profiling::Topology>>,
        alloc::sync::Arc<[rt_profiling::Group]>,
        bool,
    ),
    client: ProtocolClient<C>,
    clock: K,
    link: Link,
    capabilities: Capabilities,
    processors: Option<shoop_rt_protocol::ProcessorLimits>,
    processor_presets: Vec<shoop_rt_protocol::ProcessorPreset>,

    limits: Option<Limits>,
    driver: Option<DriverStatus>,
    audio_backend: shoop_app_api::AudioBackendKind,
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
    input: Option<input::Job>,
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
    composites: composites::Composites,
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
            host_port_inventory: None,
            inventory_reconcile: false,
            retired_host_ports: alloc::collections::BTreeMap::new(),
            resource_limits: shoop_app_api::HostResourceLimits::default(),
            profiling: Default::default(),
            profiling_groups: Default::default(),
            client,
            clock,
            link: Link::Connecting,
            capabilities: Capabilities::default(),
            processors: None,
            processor_presets: Vec::new(),
            limits: None,
            driver: None,
            audio_backend: Default::default(),
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
            input: None,
            replacement: None,
            preview: None,
            preview_playing: 0,
            next_persistence: 1,
            canonical_content: Default::default(),
            canonical_imports: Default::default(),
            prepared_content: Default::default(),
            capture_validation: None,
            composites: Default::default(),
        })
    }

    pub fn with_host_port_inventory(
        mut self,
        inventory: alloc::boxed::Box<dyn HostPortInventory>,
    ) -> Self {
        self.host_port_inventory = Some(inventory);
        self
    }

    fn refresh_host_ports(&mut self) -> Result<()> {
        if self.link != Link::Ready {
            return Ok(());
        }
        let Some(aliases) = self
            .host_port_inventory
            .as_mut()
            .and_then(|inventory| inventory.poll())
        else {
            return Ok(());
        };
        if aliases.len() > 256 {
            return Err(anyhow!("host port inventory exceeds capacity"));
        }
        let mut mapped = alloc::collections::BTreeMap::new();
        let mut identities = alloc::collections::BTreeSet::new();
        for mut alias in aliases {
            if alias.id.len() > 256 || alias.name.len() > 128 {
                return Err(anyhow!("host port identity or label exceeds capacity"));
            }
            let slot = self
                .mirror
                .host_ports
                .iter()
                .find(|port| port.name.as_str() == alias.slot)
                .ok_or_else(|| anyhow!("unknown prepared host port {}", alias.slot))?;
            if let Some(retired) = self.retired_host_ports.get(&slot.id) {
                if alias.id == *retired && !alias.available {
                    alias.id.clear();
                    alias.name.clear();
                } else {
                    self.retired_host_ports.remove(&slot.id);
                }
            }
            if (!alias.id.is_empty() && !identities.insert(alias.id.clone()))
                || mapped.insert(slot.id, alias).is_some()
            {
                return Err(anyhow!("duplicate host port identity or slot"));
            }
        }
        for port in &self.mirror.host_ports {
            let previous = self.mirror.host_key(port);
            let current = mapped
                .get(&port.id)
                .map_or(port.name.as_str(), |alias| alias.id.as_str());
            if current != previous && !previous.is_empty() && self.host_identity_in_use(previous) {
                return Err(anyhow!(
                    "a connected host port cannot change device identity"
                ));
            }
        }
        self.mirror.host_aliases = mapped;
        self.inventory_reconcile = true;
        Ok(())
    }

    fn host_identity_in_use(&self, id: &str) -> bool {
        let references = |image: &mirror::Mirror| {
            image
                .links
                .iter()
                .chain(image.desired_links.iter())
                .any(|link| link.host_port_id == id)
        };
        references(&self.mirror)
            || self.replacement.as_ref().and_then(|replacement| replacement.candidate.as_ref())
                .is_some_and(references)
            || self.requests.outgoing.iter().chain(self.requests.in_flight.values()).any(|pending|
                matches!(&pending.purpose, requests::Purpose::Connect { host, .. } if host == id))
    }

    fn reconcile_host_ports(&mut self) -> Result<()> {
        if !self.inventory_reconcile || self.ready().is_err() {
            return Ok(());
        }
        let links: Vec<_> = self
            .mirror
            .desired_links
            .iter()
            .filter(|link| {
                if self.mirror.links.contains(link) {
                    return false;
                }
                let Some(host) = self.mirror.host_by_name(&link.host_port_id) else {
                    return false;
                };
                let Some(port) = self.mirror.live_port(link.application_port_id) else {
                    return false;
                };
                self.mirror.host_available(host)
                    && mirror::host_data_type(host.kind) == port.descriptor.data_type
                    && mirror::host_direction(host.direction) != port.descriptor.direction
                    && !self
                        .requests
                        .outgoing
                        .iter()
                        .chain(self.requests.in_flight.values())
                        .any(|pending| {
                            matches!(&pending.purpose, requests::Purpose::Connect { port, host, .. }
                        if *port == link.application_port_id && host == &link.host_port_id)
                        })
            })
            .take(16)
            .cloned()
            .collect();
        self.inventory_reconcile = !links.is_empty();
        for link in links {
            if self.ready().is_err() {
                break;
            }
            self.queue_connection(link.application_port_id, &link.host_port_id, true)?;
        }
        Ok(())
    }

    /// Selects host-local admission ceilings before media operations start.
    pub fn with_resource_limits(
        mut self,
        limits: shoop_app_api::HostResourceLimits,
    ) -> Result<Self> {
        if !limits.is_valid()
            || !self.mirror.tracks.is_empty()
            || !self.requests.is_empty()
            || !self.canonical_content.is_empty()
            || self.replacement.is_some()
            || !self.reads.idle()
        {
            return Err(anyhow!("resource limits require an idle fresh backend"));
        }
        self.resource_limits = limits;
        self.reads.resource_limits = limits;
        Ok(self)
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
    /// Refresh reliable configuration while preserving accepted operations and live playback.
    pub fn request_resync(&mut self) -> Result<()> {
        self.ready()?;
        self.client
            .resync()
            .map_err(|error| anyhow!("resynchronization failed: {error:?}"))?;
        self.link = Link::Resyncing;
        self.resync = Some(Vec::new());
        Ok(())
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

    /// Makes a bounded pass through queued requests, reads and engine messages. Never waits.
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
            self.advance_input();
            self.advance_composites();
            self.advance_preview();
            self.advance_persistence();
            let sent = self.send();
            let mut received = 0;
            for _ in 0..MAX_MESSAGES_PER_PUMP_ROUND {
                let Some(message) = self.client.poll() else {
                    break;
                };
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
        let used = self.requests.outstanding()
            + self.mutation_completions.len()
            + self.composites.jobs.len()
            + self.composites.controls.len()
            + self
                .composites
                .jobs
                .iter()
                .map(|job| job.cancel_waiters.len())
                .sum::<usize>()
            + usize::from(self.input.is_some());
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
                self.composites.controls.is_empty()
                    && self.composites.jobs.is_empty()
                    && self.requests.is_empty()
                    && self.input.is_none()
                    && !self.read_cancel_needed
                    && self.reads.idle()
                    && self.telemetry_fresh
                    && self.mirror.loops.values().all(|entry| {
                        entry.view.mode.records()
                            || entry.timing.content_revision == entry.rt_content_revision
                    })
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
        let waiting = self.requests.outgoing.len();
        for _ in 0..waiting {
            let Some(pending) = self.requests.outgoing.front() else {
                break;
            };
            if self.input.as_ref().is_some_and(|job| job.blocks(pending)) {
                let pending = self.requests.outgoing.pop_front().expect("front exists");
                self.requests.outgoing.push_back(pending);
                continue;
            }
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
        self.status.memory = None;
        for pending in self
            .requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
        {
            if pending.sequence == 0 {
                continue;
            }
            self.mutation_completions.push(BackendMutationCompletion {
                sequence: pending.sequence,
                result: Err(reason.clone()),
            });
        }
        if let Some(job) = self.input.take() {
            self.mutation_completions.push(BackendMutationCompletion {
                sequence: job.sequence,
                result: Err(reason.clone()),
            });
        }
        self.disconnect_composites();
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
