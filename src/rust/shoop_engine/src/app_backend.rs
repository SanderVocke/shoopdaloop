//! Application-facing backend handles used by the native application runtime.
//!
//! This module owns driver/session handles, port/channel/loop handles, and the
//! JACK/CPAL/midir routing glue used by the application. All loop, graph, port,
//! MIDI, and FX processing stays in the core engine modules.

use crate as engine;
use crate::graph_scheduler::{GraphScheduler, DEFAULT_WINDOW};
use crate::realtime_lock_guard::Mutex;
use anyhow::{anyhow, Result};
use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
pub use engine::{
    cpal_host_names, cpal_input_device_names, cpal_input_device_names_for_host,
    cpal_output_device_names, cpal_output_device_names_for_host, driver_type_supported,
    midir_input_port_names, midir_output_port_names, AudioDriverType, ChannelMode, FXChainType,
    LoopMode, MidiEvent, MultichannelAudio, PortConnectabilityKind, PortDataType, PortDirection,
    ProfilingReport, ProfilingReportItem,
};
use shoop_settings::CarlaHostingMode;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

pub type PortConnectability = engine::PortConnectability;
pub use engine::{CommandSequence, SendError};

/// How many control operations may be outstanding between cycles.
///
/// Sized for a burst, not for the steady state: loading a session queues a mutation per port,
/// loop, channel and connection with no cycle in between, and the queue refuses rather than
/// growing when it is full. A parked engine drains after every send, so this bound only
/// really applies once a driver is running.
const COMMAND_QUEUE_CAPACITY: usize = 4096;
const INVALID_OBJECT_INDEX: usize = usize::MAX;
static NEXT_BACKEND_SESSION_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "carla")]
static NEXT_CARLA_CHAIN_ID: AtomicU64 = AtomicU64::new(1);
static CARLA_HOSTING_MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_carla_hosting_mode(mode: CarlaHostingMode) {
    CARLA_HOSTING_MODE.store(
        match mode {
            CarlaHostingMode::InProcess => 0,
            CarlaHostingMode::Subprocess => 1,
        },
        Ordering::Release,
    );
}

pub fn carla_hosting_mode() -> CarlaHostingMode {
    match CARLA_HOSTING_MODE.load(Ordering::Acquire) {
        1 => CarlaHostingMode::Subprocess,
        _ => CarlaHostingMode::InProcess,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectLifecycle {
    Pending = 0,
    Ready = 1,
    Failed = 2,
    Closed = 3,
}

impl ObjectLifecycle {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Pending,
            1 => Self::Ready,
            2 => Self::Failed,
            3 => Self::Closed,
            _ => Self::Failed,
        }
    }
}

trait ObjectIdentity: Copy {
    fn from_index(index: usize) -> Self;
    fn index(self) -> usize;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoopId(usize);

impl ObjectIdentity for LoopId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompositeId(usize);

impl ObjectIdentity for CompositeId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AudioChannelId(usize);

impl ObjectIdentity for AudioChannelId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiChannelId(usize);

impl ObjectIdentity for MidiChannelId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AudioPortId(usize);

impl ObjectIdentity for AudioPortId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiPortId(usize);

impl ObjectIdentity for MidiPortId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

struct ObjectControl<I, M> {
    session_id: u64,
    lifecycle: std::sync::atomic::AtomicU8,
    engine_index: Arc<AtomicUsize>,
    auxiliary_index: std::sync::atomic::AtomicUsize,
    creation_sequence: AtomicU64,
    acknowledged_data_sequence: AtomicU64,
    error: Mutex<Option<String>>,
    mirror: Arc<M>,
    identity: PhantomData<I>,
}

impl<I: ObjectIdentity, M: Default> ObjectControl<I, M> {
    fn pending(session_id: u64) -> Self {
        Self::pending_with_mirror(session_id, Arc::new(M::default()))
    }
}

impl<I: ObjectIdentity, M> ObjectControl<I, M> {
    fn pending_with_mirror(session_id: u64, mirror: Arc<M>) -> Self {
        Self {
            session_id,
            lifecycle: std::sync::atomic::AtomicU8::new(ObjectLifecycle::Pending as u8),
            engine_index: Arc::new(AtomicUsize::new(INVALID_OBJECT_INDEX)),
            auxiliary_index: std::sync::atomic::AtomicUsize::new(INVALID_OBJECT_INDEX),
            creation_sequence: AtomicU64::new(CommandSequence::NONE.get()),
            acknowledged_data_sequence: AtomicU64::new(0),
            error: Mutex::new(None),
            mirror,
            identity: PhantomData,
        }
    }
}

impl<I: ObjectIdentity, M> ObjectControl<I, M> {
    fn lifecycle(&self) -> ObjectLifecycle {
        ObjectLifecycle::from_u8(self.lifecycle.load(Ordering::Acquire))
    }

    fn ready_id(&self) -> Option<I> {
        (self.lifecycle() == ObjectLifecycle::Ready)
            .then(|| I::from_index(self.engine_index.load(Ordering::Relaxed)))
    }

    fn mark_ready(&self, id: I) {
        self.engine_index.store(id.index(), Ordering::Release);
        self.lifecycle
            .store(ObjectLifecycle::Ready as u8, Ordering::Release);
    }

    fn set_auxiliary_index(&self, index: usize) {
        self.auxiliary_index.store(index, Ordering::Relaxed);
    }

    fn auxiliary_index(&self) -> Option<usize> {
        let index = self.auxiliary_index.load(Ordering::Relaxed);
        (index != INVALID_OBJECT_INDEX).then_some(index)
    }

    fn mark_failed(&self, error: impl Into<String>) {
        *crate::realtime_allow_lock!("object creation failure publication", self.error.lock())
            .unwrap_or_else(|e| e.into_inner()) = Some(error.into());
        self.lifecycle
            .store(ObjectLifecycle::Failed as u8, Ordering::Release);
    }

    fn mark_closed(&self) {
        self.lifecycle
            .store(ObjectLifecycle::Closed as u8, Ordering::Release);
    }

    fn set_creation_sequence(&self, sequence: CommandSequence) {
        self.creation_sequence
            .store(sequence.get(), Ordering::Release);
    }

    fn creation_sequence(&self) -> CommandSequence {
        CommandSequence::from_raw(self.creation_sequence.load(Ordering::Acquire))
    }

    fn acknowledged_data_sequence(&self) -> u64 {
        self.acknowledged_data_sequence.load(Ordering::Relaxed)
    }

    fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn observed_lifecycle<I: ObjectIdentity, M>(
    shared: &SharedSession,
    control: &ObjectControl<I, M>,
) -> ObjectLifecycle {
    let lifecycle = control.lifecycle();
    if lifecycle == ObjectLifecycle::Pending && !shared.engine_connected() {
        tracing::debug_span!(
            "engine.control.object_creation_failed",
            session_id = shared.session_id
        )
        .in_scope(|| {
            control.mark_failed("engine disconnected while object creation was pending");
        });
        ObjectLifecycle::Failed
    } else {
        lifecycle
    }
}

#[derive(Debug, Clone)]
pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

pub type BackendSessionState = engine::BackendSessionState;

enum JackRegisteredPort {
    AudioIn {
        control: Arc<ObjectControl<AudioPortId, engine::AudioPortStateMirror>>,
        jack: jack::Port<jack::AudioIn>,
    },
    AudioOut {
        control: Arc<ObjectControl<AudioPortId, engine::AudioPortStateMirror>>,
        jack: jack::Port<jack::AudioOut>,
    },
    MidiIn {
        control: Arc<ObjectControl<MidiPortId, engine::MidiPortStateMirror>>,
        jack: jack::Port<jack::MidiIn>,
    },
    MidiOut {
        control: Arc<ObjectControl<MidiPortId, engine::MidiPortStateMirror>>,
        jack: jack::Port<jack::MidiOut>,
    },
    DecoupledMidiIn {
        queue: Arc<Mutex<Vec<MidiEvent>>>,
        jack: jack::Port<jack::MidiIn>,
    },
    DecoupledMidiOut {
        queue: Arc<Mutex<Vec<MidiEvent>>>,
        jack: jack::Port<jack::MidiOut>,
    },
}

struct JackNotifications {
    xruns: Arc<AtomicU32>,
}
impl jack::NotificationHandler for JackNotifications {
    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        self.xruns.fetch_add(1, Ordering::Relaxed);
        jack::Control::Continue
    }
}

struct JackProcess {
    /// Owned outright, not borrowed through a mutex.
    ///
    /// This is the point of the whole exercise: the callback is the sole owner of the
    /// session while JACK is running, so it never waits on a lock that a GUI thread might
    /// be holding -- and on JACK, waiting is what gets a client zombified by the watchdog.
    /// Control work reaches the session through the engine's command queue instead.
    engine: engine::Engine,
    ports: Arc<Mutex<Vec<JackRegisteredPort>>>,
    last_processed: Arc<AtomicU32>,
    /// Published from the callback and read by `get_state`, because logging here would
    /// allocate on the realtime thread.
    stale_graph_cycles: Arc<AtomicU32>,
    sample_rate: u32,
    maybe_process_callback: Option<ProcessCallback>,
}
impl jack::ProcessHandler for JackProcess {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        crate::realtime_lock_guard::forbid_locks_if_enabled(|| self.process_inner(ps))
    }
}

impl JackProcess {
    fn process_inner(&mut self, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;
        let _driver_kind =
            shoop_tracing::realtime_span!("engine.rt.driver", value = AudioDriverType::Jack as i32);
        let _span = shoop_tracing::realtime_span!("engine.rt.driver.jack", value = n_frames);
        if let Some(callback) = self.maybe_process_callback {
            unsafe {
                callback();
            }
        }
        // Destructured so the engine, the port map and the counters are borrowed separately:
        // staging input needs the session mutably while the port list is being read.
        let Self {
            engine,
            ports,
            last_processed,
            stale_graph_cycles,
            sample_rate,
            ..
        } = self;

        // Control work first, so a mode change lands on this cycle's boundary rather than
        // part-way through the buffer, and so a queued read is answered even in a cycle that
        // would otherwise do nothing.
        engine.pump();

        let sample_rate = *sample_rate;
        let session = engine.session_mut();
        session.set_sample_rate(sample_rate);
        session.set_buffer_size(n_frames as u32);
        let mut ports = crate::realtime_allow_lock!("JACK registered port registry", ports.lock())
            .unwrap_or_else(|e| e.into_inner());

        for p in ports.iter() {
            match p {
                JackRegisteredPort::AudioIn { control, jack } => {
                    if let Some(port) = control
                        .ready_id()
                        .and_then(|id| session.port_mut(id.index()))
                        .and_then(|p| p.as_external_mut())
                    {
                        port.stage_input(jack.as_slice(ps));
                    }
                }
                JackRegisteredPort::MidiIn { control, jack } => {
                    if let Some(port) = control
                        .ready_id()
                        .and_then(|id| session.port_mut(id.index()))
                        .and_then(|p| p.as_external_midi_mut())
                    {
                        for e in jack.iter(ps) {
                            let _ = port.push_incoming(e.time, e.bytes);
                        }
                    }
                }
                JackRegisteredPort::DecoupledMidiIn { queue, jack } => {
                    let mut queue = crate::realtime_allow_lock!(
                        "JACK decoupled MIDI input queue",
                        queue.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner());
                    for e in jack.iter(ps) {
                        queue.push(MidiEvent::new(e.time as i32, e.bytes.to_vec()));
                    }
                }
                _ => {}
            }
        }

        // `run_cycle` on the engine, not `process` on the session: the engine is what updates
        // the counters and publishes the state snapshot every reader polls.
        engine.run_cycle(n_frames);
        let session = engine.session();
        stale_graph_cycles.store(session.n_stale_cycles(), Ordering::Relaxed);

        for p in ports.iter_mut() {
            match p {
                JackRegisteredPort::AudioOut { control, jack } => {
                    let out = jack.as_mut_slice(ps);
                    if let Some(port) = control
                        .ready_id()
                        .and_then(|id| session.port(id.index()))
                        .and_then(|p| p.as_external())
                    {
                        let produced = port.output(n_frames);
                        let n = produced.len().min(out.len());
                        out[..n].copy_from_slice(&produced[..n]);
                        for s in &mut out[n..] {
                            *s = 0.0;
                        }
                    } else {
                        for s in out.iter_mut() {
                            *s = 0.0;
                        }
                    }
                }
                JackRegisteredPort::MidiOut { control, jack } => {
                    let mut writer = jack.writer(ps);
                    if let Some(port) = control
                        .ready_id()
                        .and_then(|id| session.port(id.index()))
                        .and_then(|p| p.as_external_midi())
                    {
                        for e in port.outgoing() {
                            let _ = writer.write(&jack::RawMidi {
                                time: e.time,
                                bytes: e.data(),
                            });
                        }
                    }
                }
                JackRegisteredPort::DecoupledMidiOut { queue, jack } => {
                    let mut writer = jack.writer(ps);
                    let mut queue = crate::realtime_allow_lock!(
                        "JACK decoupled MIDI output queue",
                        queue.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner());
                    for e in queue.drain(..) {
                        let time = (e.time.max(0) as u32).min(n_frames.saturating_sub(1) as u32);
                        let _ = writer.write(&jack::RawMidi {
                            time,
                            bytes: &e.data,
                        });
                    }
                }
                _ => {}
            }
        }
        last_processed.store(n_frames as u32, Ordering::Relaxed);
        jack::Control::Continue
    }
}

struct JackBackend {
    client: Option<jack::Client>,
    active_client: Option<jack::AsyncClient<JackNotifications, JackProcess>>,
    ports: Arc<Mutex<Vec<JackRegisteredPort>>>,
    last_processed: Arc<AtomicU32>,
    xruns: Arc<AtomicU32>,
    stale_graph_cycles: Arc<AtomicU32>,
    maybe_process_callback: Option<ProcessCallback>,
}

struct CpalMidiInputEndpoint {
    name: String,
    capture: engine::midir_driver::MidiCapture,
    _conn: engine::midir_driver::MidiCaptureConnection,
}

#[derive(Clone)]
struct CpalDecoupledMidiPort {
    port_id: engine::PortId,
    direction: PortDirection,
    queue: Arc<Mutex<Vec<MidiEvent>>>,
}

struct CpalMidiOutputEndpoint {
    name: String,
    playback: engine::midir_driver::MidiPlayback,
}

struct CpalBackend {
    stale_graph_cycles: Arc<AtomicU32>,
    _output: Option<cpal::Stream>,
    _input: Option<cpal::Stream>,
    sample_rate: u32,
    configured_buffer_size: u32,
    last_processed: Arc<AtomicU32>,
    xruns: Arc<AtomicU32>,
}

// On macOS the CoreAudio backend of cpal 0.16 holds a non-`Send` callback
// (a `Box<dyn FnMut()>` without a `Send` bound) for property-listener
// notifications, which makes `cpal::Stream` itself not `Send`. The pinned
// cpal version (kept on 0.16 because rodio 0.21 depends on the same major
// version) prevents upgrading to 0.18, where the bound is fixed. The
// streams are always reached through a `Mutex`, so the audio thread stays
// the sole owner of the underlying CoreAudio state -- promise by hand here.
unsafe impl Send for CpalBackend {}
unsafe impl Sync for CpalBackend {}

impl JackBackend {
    fn client(&self) -> &jack::Client {
        self.active_client
            .as_ref()
            .map(|c| c.as_client())
            .or(self.client.as_ref())
            .expect("JACK client missing")
    }
    fn activate(&mut self, shared: &Arc<SharedSession>) -> Result<()> {
        if self.active_client.is_some() {
            return Ok(());
        }
        let client = self
            .client
            .take()
            .ok_or_else(|| anyhow!("JACK client already activated"))?;
        // Moved out of the session, not borrowed from it: from here on the callback is the
        // only owner, and the control side reaches the engine solely through its queues.
        // Failing loudly beats activating a client whose callback has no session to run.
        let engine = shared
            .take_engine()
            .ok_or_else(|| anyhow!("the engine has already been taken by another driver"))?;
        let notifications = JackNotifications {
            xruns: self.xruns.clone(),
        };
        let process = JackProcess {
            engine,
            ports: self.ports.clone(),
            last_processed: self.last_processed.clone(),
            stale_graph_cycles: self.stale_graph_cycles.clone(),
            sample_rate: client.sample_rate(),
            maybe_process_callback: self.maybe_process_callback,
        };
        self.active_client = Some(
            client
                .activate_async(notifications, process)
                .map_err(|e| anyhow!("Failed to activate JACK client: {e}"))?,
        );
        Ok(())
    }
}

fn cpal_device_label(device: &cpal::Device) -> String {
    device.name().unwrap_or_else(|_| "cpal".to_string())
}

fn cpal_host_label(id: cpal::HostId) -> String {
    format!("{id:?}").to_lowercase()
}

fn select_cpal_host(wanted: &str) -> Result<cpal::Host> {
    if wanted == "default" || wanted.is_empty() {
        return Ok(cpal::default_host());
    }
    let id = cpal::available_hosts()
        .into_iter()
        .find(|id| cpal_host_label(*id) == wanted.to_lowercase())
        .ok_or_else(|| anyhow!("No CPAL host named {wanted}"))?;
    cpal::host_from_id(id).map_err(|e| anyhow!("Could not open CPAL host {wanted}: {e}"))
}

fn limit_cpal_channels(current: u16, wanted: &str) -> u16 {
    if wanted == "all" || wanted.is_empty() {
        current
    } else {
        wanted
            .parse::<u16>()
            .ok()
            .map(|n| n.max(1).min(current))
            .unwrap_or(current)
    }
}

fn apply_cpal_config_options(
    mut config: cpal::StreamConfig,
    sample_rate: u32,
    buffer_size: u32,
    channels: &str,
) -> cpal::StreamConfig {
    if sample_rate > 0 {
        config.sample_rate = cpal::SampleRate(sample_rate);
    }
    if buffer_size > 0 {
        config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
    }
    config.channels = limit_cpal_channels(config.channels, channels);
    config
}

fn select_cpal_device<I>(devices: I, wanted: &str) -> Option<cpal::Device>
where
    I: IntoIterator<Item = cpal::Device>,
{
    if wanted == "default" || wanted.is_empty() {
        return None;
    }
    if let Ok(idx) = wanted.parse::<usize>() {
        return devices.into_iter().nth(idx);
    }
    devices
        .into_iter()
        .find(|d| cpal_device_label(d) == wanted || cpal_device_label(d).contains(wanted))
}

fn selector_is_all(v: &[String]) -> bool {
    v.is_empty() || v.iter().any(|s| s == "all")
}
fn selector_is_none(v: &[String]) -> bool {
    v.iter().any(|s| s == "none")
}
fn selector_matches(v: &[String], idx: usize, name: &str) -> bool {
    selector_is_all(v)
        || v.iter()
            .any(|s| s == &idx.to_string() || s == name || name.contains(s))
}

fn stage_virtual_audio_inputs(
    session: &mut engine::Session,
    connections: &[(engine::PortId, String)],
    capture_names: &[String],
    input_channels: usize,
    capture_interleaved: &[f32],
    n_frames: usize,
) {
    for (port_id, ext_name) in connections {
        let Some(session_idx) = port_id.0.checked_sub(1).map(|v| v as usize) else {
            continue;
        };
        if let Some(ch) = capture_names.iter().position(|n| n == ext_name) {
            if let Some(port) = session
                .port_mut(session_idx)
                .and_then(|p| p.as_external_mut())
            {
                if input_channels > 0 {
                    let mut plane = Vec::with_capacity(n_frames);
                    for f in 0..n_frames {
                        plane.push(
                            capture_interleaved
                                .get(f * input_channels + ch)
                                .copied()
                                .unwrap_or(0.0),
                        );
                    }
                    port.stage_input(&plane);
                }
            }
        }
    }
}

fn collect_virtual_audio_outputs(
    session: &engine::Session,
    connections: &[(engine::PortId, String)],
    playback_names: &[String],
    output_channels: usize,
    output_interleaved: &mut [f32],
    n_frames: usize,
) {
    for (port_id, ext_name) in connections {
        let Some(session_idx) = port_id.0.checked_sub(1).map(|v| v as usize) else {
            continue;
        };
        if let Some(ch) = playback_names.iter().position(|n| n == ext_name) {
            if let Some(port) = session.port(session_idx).and_then(|p| p.as_external()) {
                let produced = port.output(n_frames);
                for f in 0..n_frames {
                    output_interleaved[f * output_channels + ch] +=
                        produced.get(f).copied().unwrap_or(0.0);
                }
            }
        }
    }
}

fn route_virtual_midi_inputs(
    session: &mut engine::Session,
    connections: &[(engine::PortId, String)],
    input_name: &str,
    events: &[engine::midi_storage::MidiStorageElem],
    decoupled: &[CpalDecoupledMidiPort],
) {
    for (port_id, ext_name) in connections {
        if ext_name != input_name {
            continue;
        }
        if let Some(session_idx) = port_id.0.checked_sub(1).map(|v| v as usize) {
            if let Some(port) = session
                .port_mut(session_idx)
                .and_then(|p| p.as_external_midi_mut())
            {
                for e in events {
                    let _ = port.push_incoming(0, e.data());
                }
            }
        }
        for port in decoupled
            .iter()
            .filter(|p| p.port_id == *port_id && p.direction == PortDirection::Input)
        {
            let mut queue =
                crate::realtime_allow_lock!("CPAL decoupled MIDI input queue", port.queue.lock())
                    .unwrap_or_else(|e| e.into_inner());
            for e in events {
                queue.push(MidiEvent::new(0, e.data().to_vec()));
            }
        }
    }
}

fn drain_decoupled_midi_output_events(
    connections: &[(engine::PortId, String)],
    output_name: &str,
    decoupled: &[CpalDecoupledMidiPort],
) -> Vec<engine::midi_storage::MidiStorageElem> {
    let mut out = Vec::new();
    for port in decoupled
        .iter()
        .filter(|p| p.direction == PortDirection::Output)
    {
        if !connections
            .iter()
            .any(|(id, name)| *id == port.port_id && name == output_name)
        {
            continue;
        }
        let mut queue =
            crate::realtime_allow_lock!("CPAL decoupled MIDI output queue", port.queue.lock())
                .unwrap_or_else(|e| e.into_inner());
        for e in queue.drain(..) {
            if let Some(elem) =
                engine::midi_storage::MidiStorageElem::new(e.time.max(0) as u32, &e.data)
            {
                out.push(elem);
            }
        }
    }
    out
}

impl CpalBackend {
    fn start(
        engine: engine::Engine,
        settings: &CpalMidiAudioDriverSettings,
        external: Arc<Mutex<engine::DummyExternalConnections>>,
        decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
        maybe_process_callback: Option<ProcessCallback>,
    ) -> Result<Self> {
        let host = select_cpal_host(&settings.host)?;
        let output_device = if settings.output_device == "none" {
            return Err(anyhow!("CPAL output device is required"));
        } else {
            select_cpal_device(host.output_devices()?, &settings.output_device)
                .or_else(|| host.default_output_device())
                .ok_or_else(|| anyhow!("No CPAL output device available"))?
        };
        let output_supported_config = output_device.default_output_config()?;
        let sample_rate = if settings.sample_rate > 0 {
            settings.sample_rate
        } else {
            output_supported_config.sample_rate().0
        };
        let output_config = apply_cpal_config_options(
            output_supported_config.into(),
            settings.sample_rate,
            settings.buffer_size,
            &settings.output_channels,
        );
        let output_channels = output_config.channels as usize;
        let output_device_name = cpal_device_label(&output_device);
        let playback_names: Vec<String> = (0..output_channels)
            .map(|c| format!("cpal:{output_device_name}:playback_{}", c + 1))
            .collect();

        let mut input_stream = None;
        let mut input_ring = None;
        let mut input_channels = 0usize;
        let mut capture_names = Vec::new();
        let xruns = Arc::new(AtomicU32::new(0));
        let capture_underruns = Arc::new(AtomicU32::new(0));
        let capture_overruns = Arc::new(AtomicU32::new(0));
        if settings.input_device != "none" {
            if let Some(input_device) =
                select_cpal_device(host.input_devices()?, &settings.input_device)
                    .or_else(|| host.default_input_device())
            {
                let input_supported_config = input_device.default_input_config()?;
                let input_config = apply_cpal_config_options(
                    input_supported_config.into(),
                    sample_rate,
                    settings.buffer_size,
                    &settings.input_channels,
                );
                input_channels = input_config.channels as usize;
                let input_device_name = cpal_device_label(&input_device);
                capture_names = (0..input_channels)
                    .map(|c| format!("cpal:{input_device_name}:capture_{}", c + 1))
                    .collect();
                let cap = settings.capture_ring_frames.max(1) as usize * input_channels.max(1);
                let ring = Arc::new(Mutex::new(VecDeque::with_capacity(cap)));
                let cb_ring = ring.clone();
                let cb_xruns_in = xruns.clone();
                let cb_capture_overruns = capture_overruns.clone();
                input_stream = Some(input_device.build_input_stream(
                    &input_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        crate::realtime_lock_guard::forbid_locks_if_enabled(|| {
                            let _driver_kind = shoop_tracing::realtime_span!(
                                "engine.rt.driver",
                                value = AudioDriverType::Cpal as i32
                            );
                            let n_frames =
                                data.len().checked_div(input_channels.max(1)).unwrap_or(0);
                            let _span = shoop_tracing::realtime_span!(
                                "engine.rt.driver.cpal_input",
                                value = n_frames
                            );
                            let mut ring = crate::realtime_allow_lock!(
                                "CPAL capture ring input",
                                cb_ring.lock()
                            )
                            .unwrap_or_else(|e| e.into_inner());
                            for &s in data {
                                if ring.len() >= cap {
                                    ring.pop_front();
                                    cb_xruns_in.fetch_add(1, Ordering::Relaxed);
                                    cb_capture_overruns.fetch_add(1, Ordering::Relaxed);
                                }
                                ring.push_back(s);
                            }
                        });
                    },
                    |_| {},
                    None,
                )?);
                input_ring = Some(ring);
            }
        }

        {
            let mut ext = external.lock().unwrap_or_else(|e| e.into_inner());
            for name in &capture_names {
                ext.add_mock_port(
                    name.clone(),
                    engine::PortDirection::Output,
                    engine::PortDataType::Audio,
                );
            }
            for name in &playback_names {
                ext.add_mock_port(
                    name.clone(),
                    engine::PortDirection::Input,
                    engine::PortDataType::Audio,
                );
            }
        }

        let mut midi_inputs = Vec::new();
        if !selector_is_none(&settings.midi_inputs) {
            if let Ok(input) = midir::MidiInput::new(&settings.client_name) {
                for (idx, port) in input.ports().iter().enumerate() {
                    if let Ok(port_name) = input.port_name(port) {
                        if selector_matches(&settings.midi_inputs, idx, &port_name) {
                            if let Ok((capture, conn)) = engine::midir_driver::open_input(
                                &settings.client_name,
                                &format!("{}-in-{idx}", settings.client_name),
                                &port_name,
                            ) {
                                let name = format!("midir:{port_name}:output");
                                external
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .add_mock_port(
                                        name.clone(),
                                        engine::PortDirection::Output,
                                        engine::PortDataType::Midi,
                                    );
                                midi_inputs.push(CpalMidiInputEndpoint {
                                    name,
                                    capture,
                                    _conn: conn,
                                });
                            }
                        }
                    }
                }
            }
        }
        let mut midi_outputs = Vec::new();
        if !selector_is_none(&settings.midi_outputs) {
            if let Ok(output) = midir::MidiOutput::new(&settings.client_name) {
                for (idx, port) in output.ports().iter().enumerate() {
                    if let Ok(port_name) = output.port_name(port) {
                        if selector_matches(&settings.midi_outputs, idx, &port_name) {
                            if let Ok(playback) = engine::midir_driver::open_output(
                                &settings.client_name,
                                &format!("{}-out-{idx}", settings.client_name),
                                &port_name,
                            ) {
                                let name = format!("midir:{port_name}:input");
                                external
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .add_mock_port(
                                        name.clone(),
                                        engine::PortDirection::Input,
                                        engine::PortDataType::Midi,
                                    );
                                midi_outputs.push(CpalMidiOutputEndpoint { name, playback });
                            }
                        }
                    }
                }
            }
        }

        let input_ring_cb = input_ring.clone();
        let capture_names_cb = capture_names.clone();
        let playback_names_cb = playback_names.clone();
        let midi_inputs_cb = Arc::new(Mutex::new(midi_inputs));
        let midi_outputs_cb = Arc::new(Mutex::new(midi_outputs));
        let decoupled_cb = decoupled_midi_ports.clone();
        let external_cb = external.clone();
        let capture_underruns_cb = capture_underruns.clone();
        let capture_overruns_cb = capture_overruns.clone();
        let last_processed = Arc::new(AtomicU32::new(0));
        let last_processed_cb = last_processed.clone();
        let xruns_cb = xruns.clone();
        let stale_graph_cycles = Arc::new(AtomicU32::new(0));
        let stale_cb = stale_graph_cycles.clone();
        let mut capture_scratch = Vec::<f32>::new();
        // Mutable because the callback pumps and processes it; moved in below, after which
        // this is the only owner of the session for as long as the stream runs.
        let mut engine = engine;

        let output_stream = output_device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                crate::realtime_lock_guard::forbid_locks_if_enabled(|| {
                    let n_frames = data.len().checked_div(output_channels.max(1)).unwrap_or(0);
                    let _driver_kind = shoop_tracing::realtime_span!(
                        "engine.rt.driver",
                        value = AudioDriverType::Cpal as i32
                    );
                    let _span = shoop_tracing::realtime_span!(
                        "engine.rt.driver.cpal_output",
                        value = n_frames
                    );
                    last_processed_cb.store(n_frames as u32, Ordering::Relaxed);
                    for s in data.iter_mut() {
                        *s = 0.0;
                    }
                    if let Some(callback) = maybe_process_callback {
                        unsafe {
                            callback();
                        }
                    }
                    let connections = crate::realtime_allow_lock!(
                        "CPAL external connection registry",
                        external_cb.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner())
                    .connections();
                    let wanted = n_frames * input_channels;
                    if capture_scratch.len() < wanted {
                        capture_scratch.resize(wanted, 0.0);
                    }
                    if let Some(ring) = input_ring_cb.as_ref() {
                        let mut ring =
                            crate::realtime_allow_lock!("CPAL capture ring output", ring.lock())
                                .unwrap_or_else(|e| e.into_inner());
                        let mut underflowed = false;
                        for s in &mut capture_scratch[..wanted] {
                            match ring.pop_front() {
                                Some(value) => *s = value,
                                None => {
                                    *s = 0.0;
                                    underflowed = true;
                                }
                            }
                        }
                        if underflowed {
                            capture_underruns_cb.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    engine.stats().capture_underruns.store(
                        capture_underruns_cb.load(Ordering::Relaxed),
                        Ordering::Relaxed,
                    );
                    engine.stats().capture_overruns.store(
                        capture_overruns_cb.load(Ordering::Relaxed),
                        Ordering::Relaxed,
                    );

                    // Control work first, at the cycle boundary, and with no lock anywhere: the
                    // callback owns the engine, so a GUI thread polling state cannot make this
                    // one wait. On cpal that waiting showed up as glitching rather than as a
                    // zombied client, which made it easier to miss and no less real.
                    engine.pump();
                    let session = engine.session_mut();
                    session.set_sample_rate(sample_rate);
                    session.set_buffer_size(n_frames as u32);

                    stage_virtual_audio_inputs(
                        session,
                        &connections,
                        &capture_names_cb,
                        input_channels,
                        &capture_scratch[..wanted],
                        n_frames,
                    );
                    {
                        let decoupled = crate::realtime_allow_lock!(
                            "CPAL decoupled MIDI input registry",
                            decoupled_cb.lock()
                        )
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                        let mut inputs = crate::realtime_allow_lock!(
                            "CPAL MIDI input endpoint registry",
                            midi_inputs_cb.lock()
                        )
                        .unwrap_or_else(|e| e.into_inner());
                        for input in inputs.iter_mut() {
                            let events = input.capture.drain_pending();
                            if !events.is_empty() {
                                route_virtual_midi_inputs(
                                    session,
                                    &connections,
                                    &input.name,
                                    &events,
                                    &decoupled,
                                );
                            }
                        }
                    }

                    engine.run_cycle(n_frames);
                    let session = engine.session();
                    stale_cb.store(session.n_stale_cycles(), Ordering::Relaxed);

                    collect_virtual_audio_outputs(
                        session,
                        &connections,
                        &playback_names_cb,
                        output_channels,
                        data,
                        n_frames,
                    );
                    {
                        let decoupled = crate::realtime_allow_lock!(
                            "CPAL decoupled MIDI output registry",
                            decoupled_cb.lock()
                        )
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                        let mut outputs = crate::realtime_allow_lock!(
                            "CPAL MIDI output endpoint registry",
                            midi_outputs_cb.lock()
                        )
                        .unwrap_or_else(|e| e.into_inner());
                        for output in outputs.iter_mut() {
                            for (port_id, ext_name) in &connections {
                                if ext_name != &output.name {
                                    continue;
                                }
                                if let Some(session_idx) =
                                    port_id.0.checked_sub(1).map(|v| v as usize)
                                {
                                    if let Some(port) =
                                        session.port(session_idx).and_then(|p| p.as_external_midi())
                                    {
                                        output.playback.send_from(port);
                                    }
                                }
                            }
                            let events = drain_decoupled_midi_output_events(
                                &connections,
                                &output.name,
                                &decoupled,
                            );
                            if !events.is_empty() {
                                output.playback.send_events(&events);
                            }
                        }
                    }
                });
            },
            move |_| {
                xruns_cb.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )?;
        if let Some(stream) = input_stream.as_ref() {
            stream.play()?;
        }
        output_stream.play()?;

        Ok(Self {
            stale_graph_cycles,
            _output: Some(output_stream),
            _input: input_stream,
            sample_rate,
            configured_buffer_size: settings.buffer_size,
            last_processed,
            xruns,
        })
    }

    /// Start a CPAL backend against the software mock host rather than a real
    /// OS audio device, so the CPAL virtual port routing can be exercised on
    /// headless CI where ALSA / CoreAudio / WASAPI has no usable device.
    fn start_with_mock(
        settings: &CpalMidiAudioDriverSettings,
        external: Arc<Mutex<engine::DummyExternalConnections>>,
        _decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
        _maybe_process_callback: Option<ProcessCallback>,
    ) -> Result<Self> {
        use crate::cpal_mock::MockHost;
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = MockHost::new();
        let output_device = host.default_output_device().expect("mock output device");
        let output_config = output_device.default_output_config()?;
        let output_channels = output_config.channels() as usize;
        let sample_rate = output_config.sample_rate().0;
        let output_device_name = output_device
            .name()
            .unwrap_or_else(|_| "mock-output".to_string());
        let playback_names: Vec<String> = (0..output_channels)
            .map(|c| format!("cpal:{output_device_name}:playback_{}", c + 1))
            .collect();

        let capture_names = if settings.input_device == "none" {
            Vec::new()
        } else {
            let input_device = host.default_input_device().expect("mock input device");
            let input_channels = input_device.default_input_config()?.channels() as usize;
            let input_device_name = input_device
                .name()
                .unwrap_or_else(|_| "mock-input".to_string());
            (0..input_channels)
                .map(|c| format!("cpal:{input_device_name}:capture_{}", c + 1))
                .collect()
        };

        {
            let mut ext = external.lock().unwrap_or_else(|e| e.into_inner());
            for name in &capture_names {
                ext.add_mock_port(
                    name.clone(),
                    engine::PortDirection::Output,
                    engine::PortDataType::Audio,
                );
            }
            for name in &playback_names {
                ext.add_mock_port(
                    name.clone(),
                    engine::PortDirection::Input,
                    engine::PortDataType::Audio,
                );
            }
        }

        let last_processed = Arc::new(AtomicU32::new(0));
        let xruns = Arc::new(AtomicU32::new(0));

        Ok(Self {
            stale_graph_cycles: Arc::new(AtomicU32::new(0)),
            _output: None,
            _input: None,
            sample_rate,
            configured_buffer_size: 0,
            last_processed,
            xruns,
        })
    }
}

#[derive(Clone)]
struct CompositeConfig {
    descriptor: engine::CompositePlanDescriptor,
    sync_source: engine::LoopIdentity,
    state: Arc<engine::CompositeStateMirror>,
}

#[derive(Clone, Default)]
struct CompositeRegistry {
    configs: BTreeMap<engine::LoopIdentity, CompositeConfig>,
    metadata: BTreeMap<engine::LoopIdentity, engine::LoopTargetMetadata>,
}

#[tracing::instrument(
    name = "engine.control.compile_composite_registry",
    skip_all,
    fields(composites = registry.configs.len(), targets = registry.metadata.len())
)]
fn compile_composite_registry(
    registry: &CompositeRegistry,
) -> Result<engine::CompositeBoundaryTimeline> {
    let catalog = engine::LoopTargetCatalog::new(registry.metadata.values().copied().collect())
        .map_err(|error| anyhow!("invalid composite target catalog: {error}"))?;
    let dependencies: Vec<_> = registry
        .configs
        .values()
        .map(|config| engine::CompositeDependency {
            source: config.descriptor.source,
            composite_children: config
                .descriptor
                .timelines
                .iter()
                .flat_map(|timeline| &timeline.sections)
                .flat_map(|section| &section.entries)
                .filter_map(|entry| {
                    (entry.target.kind == engine::LoopTargetKind::Composite).then_some(entry.target)
                })
                .collect(),
        })
        .collect();
    let mut nodes = Vec::with_capacity(registry.configs.len());
    for config in registry.configs.values() {
        let installed: Vec<_> = dependencies
            .iter()
            .filter(|dependency| dependency.source != config.descriptor.source)
            .cloned()
            .collect();
        let plan = engine::compile_composite_plan(
            &config.descriptor,
            &catalog,
            &installed,
            engine::CompositePlanLimits::default(),
        )
        .map_err(|error| anyhow!("composite plan validation failed: {error}"))?;
        nodes.push(engine::CompositeTimelineNode {
            plan,
            sync_source: config.sync_source,
        });
    }
    let mut timeline =
        engine::CompositeBoundaryTimeline::new(nodes, engine::CompositeTimelineLimits::default())
            .map_err(|error| anyhow!("composite timeline validation failed: {error}"))?;
    for config in registry.configs.values() {
        if !timeline.set_state_mirror(config.descriptor.source, Arc::clone(&config.state)) {
            return Err(anyhow!(
                "compiled composite source is missing from its timeline"
            ));
        }
    }
    Ok(timeline)
}

#[derive(Clone)]
struct ConnectionCacheRequest {
    name: String,
    direction: PortDirection,
    data_type: PortDataType,
    session_index: Option<usize>,
}

struct ConnectionCache {
    requests: HashMap<(String, u32, u32), ConnectionCacheRequest>,
    states: HashMap<(String, u32, u32), HashMap<String, bool>>,
    last_refresh: Instant,
    refresh_in_flight: bool,
    generation: u64,
}

impl Default for ConnectionCache {
    fn default() -> Self {
        Self {
            requests: HashMap::new(),
            states: HashMap::new(),
            last_refresh: Instant::now() - Duration::from_secs(1),
            refresh_in_flight: false,
            generation: 0,
        }
    }
}

#[derive(Clone)]
struct PrimitiveLoopControl {
    control: Weak<ObjectControl<LoopId, engine::LoopStateMirror>>,
    /// Stable identity storage retained separately because ready engine slots outlive controls.
    engine_index: Arc<AtomicUsize>,
    desired_source: Option<Weak<ObjectControl<LoopId, engine::LoopStateMirror>>>,
    /// Source identity most recently accepted by the engine.
    applied_source_index: Arc<AtomicUsize>,
}

struct SharedSession {
    session_id: u64,
    /// The control side of the engine. Only ever touched by non-audio threads.
    ///
    /// The mutex here guards the *handle*, not the session: several GUI threads may queue
    /// work, but the session itself is reached solely through the queues inside, so no
    /// audio thread ever waits on this.
    handle: Mutex<engine::EngineHandle>,
    /// Lock-free lifecycle observation for pending object handles.
    engine_connected: Arc<AtomicBool>,
    /// The engine, for as long as no driver has taken it.
    ///
    /// Between construction and a driver activating there is no audio thread at all, and
    /// session building still has to work. While the engine sits here the control thread
    /// drives it directly, which is sound precisely because nothing else can reach it.
    parked: Mutex<Option<engine::Engine>>,
    next_composite_slot: AtomicU32,
    next_composite_version: AtomicU32,
    composite_registry: Mutex<CompositeRegistry>,
    primitive_loop_controls: Mutex<Vec<PrimitiveLoopControl>>,
    primitive_sync_sources: Mutex<Vec<Option<usize>>>,
    audio_snapshot_controls: Mutex<
        Vec<(
            Weak<ObjectControl<LoopId, engine::LoopStateMirror>>,
            Weak<ObjectControl<AudioChannelId, engine::AudioChannelStateMirror>>,
            engine::content_snapshot::AudioSnapshotControl,
        )>,
    >,
    external: Mutex<Option<Arc<Mutex<engine::DummyExternalConnections>>>>,
    jack: Mutex<Option<Arc<Mutex<JackBackend>>>>,
    cpal: Mutex<Option<Arc<Mutex<CpalBackend>>>>,
    connection_cache: Arc<Mutex<ConnectionCache>>,
    sample_rate: AtomicU32,
    buffer_size: AtomicU32,
    snapshots: engine::content_snapshot::ContentSnapshotRuntime,
    /// Rebuilds the schedule after topology changes.
    ///
    /// A `OnceLock` rather than a `Mutex`: it is set once immediately after construction
    /// -- it needs a `Weak` back to the `SharedSession` it applies to, so it cannot be
    /// built in the initialiser -- and read on every mutation thereafter.
    scheduler: OnceLock<GraphScheduler>,
}

impl SharedSession {
    fn engine_connected(&self) -> bool {
        self.engine_connected.load(Ordering::Acquire)
    }

    #[track_caller]
    fn send_control(
        &self,
        f: impl FnMut(&mut engine::Session) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        self.send_with_graph_effect(f, false)
    }

    #[track_caller]
    fn send_topology(
        &self,
        f: impl FnMut(&mut engine::Session) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        self.send_with_graph_effect(f, true)
    }

    #[track_caller]
    fn send_with_graph_effect(
        &self,
        f: impl FnMut(&mut engine::Session) + Send + 'static,
        changes_topology: bool,
    ) -> std::result::Result<CommandSequence, SendError> {
        let span = tracing::debug_span!(
            "engine.control.queue",
            session_id = self.session_id,
            changes_topology,
            sequence = tracing::field::Empty,
            retries = tracing::field::Empty,
            outcome = tracing::field::Empty
        );
        let _entered = span.enter();
        let mut f = Some(f);
        let mut warned = false;
        let mut retries = 0_u32;
        let sequence = loop {
            let attempt = {
                let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
                match handle.try_reserve() {
                    Ok(reservation) => {
                        let command = Box::new(f.take().expect("command queued once"));
                        Ok(handle.send_reserved(reservation, command))
                    }
                    Err(error) => Err(error),
                }
            };
            match attempt {
                Ok(sequence) => break sequence,
                Err(SendError::Full) => {
                    retries = retries.saturating_add(1);
                    if !warned {
                        let caller = std::panic::Location::caller();
                        log::warn!(
                            "engine command queue is full; waiting for capacity ({}:{})",
                            caller.file(),
                            caller.line()
                        );
                        warned = true;
                    }
                    self.pump_or_wait_for_capacity();
                }
                Err(error) => {
                    span.record("outcome", "disconnected");
                    span.record("retries", retries);
                    return Err(error);
                }
            }
        };
        span.record("sequence", sequence.get());
        span.record("retries", retries);
        span.record("outcome", "queued");

        if changes_topology {
            if let Some(scheduler) = self.scheduler.get() {
                scheduler.arm();
            }
        }
        self.pump_parked();
        Ok(sequence)
    }

    fn pump_or_wait_for_capacity(&self) {
        if !self.pump_parked() {
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    fn pump_parked(&self) -> bool {
        if let Some(engine) = self
            .parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            engine.pump();
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    fn query_for_test<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<T> {
        self.query_with_graph_effect(f, false)
    }

    #[cfg(test)]
    fn query_topology_for_test<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<T> {
        self.query_with_graph_effect(f, true)
    }

    #[cfg(test)]
    fn query_with_graph_effect<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
        changes_topology: bool,
    ) -> Result<T> {
        let answer = self.query_graph_scheduler_response(f);
        if changes_topology {
            if let Some(scheduler) = self.scheduler.get() {
                scheduler.arm();
            }
        }
        answer
    }

    fn queue_result<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<(CommandSequence, rtrb::Consumer<T>)> {
        let span = tracing::debug_span!(
            "engine.control.queue_result",
            session_id = self.session_id,
            sequence = tracing::field::Empty,
            retries = tracing::field::Empty,
            outcome = tracing::field::Empty
        );
        let _entered = span.enter();
        if let Some(e) = self
            .parked
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
        {
            e.pump();
        }
        let mut f = Some(f);
        let mut warned = false;
        let mut retries = 0_u32;
        let queued = loop {
            let attempt = {
                let mut handle = self
                    .handle
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                match handle.try_reserve() {
                    Ok(reservation) => Ok(handle.send_for_result_reserved(
                        reservation,
                        f.take().expect("request queued once"),
                    )),
                    Err(error) => Err(error),
                }
            };
            match attempt {
                Ok(queued) => break queued,
                Err(SendError::Full) => {
                    retries = retries.saturating_add(1);
                    if !warned {
                        log::warn!("engine command queue is full; waiting to queue request");
                        warned = true;
                    }
                    self.pump_or_wait_for_capacity();
                }
                Err(error) => {
                    span.record("outcome", "disconnected");
                    return Err(anyhow!("could not queue the request: {error}"));
                }
            }
        };
        span.record("sequence", queued.0.get());
        span.record("retries", retries);
        span.record("outcome", "queued");
        self.pump_parked();
        Ok(queued)
    }

    fn query_graph_scheduler_response<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<T> {
        let _span =
            tracing::debug_span!("engine.control.graph_query", session_id = self.session_id)
                .entered();
        if let Some(e) = self
            .parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            e.pump();
            let result = f(e.session_mut());
            e.publish_graph_staleness();
            return Ok(result);
        }

        let (_, rx) = self.queue_result(f)?;
        engine::wait_for_result(rx, engine::DEFAULT_WAIT_TIMEOUT)
            .map_err(|e| anyhow!("engine did not answer: {e}"))
    }

    fn wait_for_command(&self, sequence: CommandSequence, timeout: Duration) -> Result<()> {
        let stats = {
            let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
            Arc::clone(handle.stats())
        };
        engine::wait_for_command(&stats, sequence, timeout)
            .map_err(|e| anyhow!("engine did not reach command {}: {e}", sequence.get()))
    }

    fn poll_trace<T>(&self, f: impl FnOnce(&engine::CompositeTraceSnapshot) -> T) -> Option<T> {
        self.handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .poll_trace()
            .map(f)
    }

    /// Whether a schedule rebuild might be needed, without asking the audio thread.
    ///
    /// The scheduler runs this on every armed window, and every control operation arms one, so
    /// it has to be cheap: asking the engine directly costs a round trip, and doing that
    /// ~90 times a second kept the command queue permanently busy with questions whose answer
    /// was almost always "nothing to do" -- which starved every other control operation.
    ///
    /// A published `true` is trusted at once. A published `false` is only trusted when no
    /// command is queued or being applied: a mutation does not publish graph staleness until
    /// its complete command batch has finished. Pending work re-arms another bounded window.
    fn graph_may_need_rebuild(&self) -> bool {
        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        let pending = handle.n_pending() > 0;
        let command_batch_in_flight = handle
            .stats()
            .command_batch_in_flight
            .load(Ordering::Acquire);
        if pending || command_batch_in_flight {
            drop(handle);
            if let Some(s) = self.scheduler.get() {
                s.arm();
            }
            return false;
        }
        handle.stats().graph_stale.load(Ordering::Relaxed)
    }

    /// Hands the engine to a driver that is about to start cycling it.
    fn take_engine(&self) -> Option<engine::Engine> {
        crate::realtime_allow_lock!("dummy driver engine claim", self.parked.lock())
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Takes the engine back from a driver that has stopped.
    ///
    /// Two reasons this is not optional. A driver that stops without returning it leaves the
    /// session unreachable, so every control call afterwards waits out its timeout. And the
    /// session would then be *destroyed on the driver's thread* -- which for a session holding
    /// Carla Native hosts means tearing down plugin instances on a thread that did not create
    /// them, and those do not survive it.
    fn return_engine(&self, engine: engine::Engine) {
        *self.parked.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);
    }

    /// Waits until every queued control operation has been applied.
    ///
    /// "Settled" has to include control work, not just the schedule. A queued setter has no
    /// effect until a cycle picks it up, so a caller that configures something and then
    /// advances the driver by an exact number of frames would otherwise run those frames
    /// against the old configuration -- which is how a test that sets a port's retained window
    /// and then records came out with nothing retained.
    fn drain_queue(&self, timeout: Duration) {
        let _span = tracing::debug_span!(
            "engine.control.drain_queue",
            session_id = self.session_id,
            timeout_ms = timeout.as_millis() as u64
        )
        .entered();
        let deadline = Instant::now() + timeout;
        loop {
            let pending = self
                .handle
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .n_pending();
            if pending == 0 || Instant::now() >= deadline {
                return;
            }
            // A parked engine has nobody to apply them, so this thread does it.
            if let Some(e) = self
                .parked
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
            {
                e.pump();
                continue;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Applies any pending graph changes and returns once they have landed.
    fn flush_graph_changes(&self) {
        let _span =
            tracing::debug_span!("engine.graph.flush_session", session_id = self.session_id)
                .entered();
        if let Some(s) = self.scheduler.get() {
            s.flush_blocking();
        }
    }
    fn external(&self) -> Option<Arc<Mutex<engine::DummyExternalConnections>>> {
        self.external
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn jack(&self) -> Option<Arc<Mutex<JackBackend>>> {
        self.jack.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn connections_state(
        &self,
        name: &str,
        direction: PortDirection,
        data_type: PortDataType,
        session_index: Option<usize>,
    ) -> HashMap<String, bool> {
        let key = (name.to_string(), direction as u32, data_type as u32);
        let (cached, refresh, generation) = {
            let mut cache = self
                .connection_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            cache.requests.insert(
                key.clone(),
                ConnectionCacheRequest {
                    name: name.to_string(),
                    direction,
                    data_type,
                    session_index,
                },
            );
            let cached = cache.states.get(&key).cloned().unwrap_or_default();
            let refresh = !cache.refresh_in_flight
                && cache.last_refresh.elapsed() >= Duration::from_millis(100);
            if refresh {
                cache.refresh_in_flight = true;
                cache.last_refresh = Instant::now();
            }
            (cached, refresh, cache.generation)
        };

        if refresh {
            let cache = Arc::clone(&self.connection_cache);
            let jack = self.jack();
            let external = self.external();
            let _ = thread::Builder::new()
                .name("engine-connection-cache".to_string())
                .spawn(move || refresh_connection_cache(cache, jack, external, generation))
                .expect("spawn engine connection cache worker");
        }
        cached
    }

    fn connections_state_now(
        &self,
        name: &str,
        direction: PortDirection,
        data_type: PortDataType,
        session_index: Option<usize>,
    ) -> HashMap<String, bool> {
        let key = (name.to_string(), direction as u32, data_type as u32);
        let request = ConnectionCacheRequest {
            name: name.to_string(),
            direction,
            data_type,
            session_index,
        };
        let state = if let Some(jack) = self.jack() {
            let jack = jack.lock().unwrap_or_else(|e| e.into_inner());
            jack_connections_state_locked(&jack, name, direction, data_type)
        } else if let Some(external) = self.external() {
            let external = external.lock().unwrap_or_else(|e| e.into_inner());
            dummy_connections_state_locked(&external, &request)
        } else {
            HashMap::new()
        };
        let mut cache = self
            .connection_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.requests.insert(key.clone(), request);
        cache.states.insert(key, state.clone());
        cache.last_refresh = Instant::now();
        cache.refresh_in_flight = false;
        cache.generation = cache.generation.wrapping_add(1);
        state
    }

    fn invalidate_connection_cache(&self) {
        let mut cache = self
            .connection_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.last_refresh = Instant::now() - Duration::from_secs(1);
        cache.refresh_in_flight = false;
        cache.generation = cache.generation.wrapping_add(1);
    }

    fn set_cached_connection(
        &self,
        name: &str,
        direction: PortDirection,
        data_type: PortDataType,
        external_name: &str,
        connected: bool,
    ) {
        let key = (name.to_string(), direction as u32, data_type as u32);
        self.connection_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .states
            .entry(key)
            .or_default()
            .insert(external_name.to_string(), connected);
    }

    fn activate_jack(&self, shared: &Arc<SharedSession>) -> Result<()> {
        if let Some(j) = self.jack() {
            j.lock()
                .unwrap_or_else(|e| e.into_inner())
                .activate(shared)?;
        }
        Ok(())
    }
}
fn compat_port_id(idx: usize) -> engine::PortId {
    engine::PortId(idx as u64 + 1)
}
fn opposite_direction(direction: PortDirection) -> PortDirection {
    match direction {
        PortDirection::Input => PortDirection::Output,
        PortDirection::Output => PortDirection::Input,
        PortDirection::Any => PortDirection::Any,
    }
}
fn jack_flags(
    direction: PortDirection,
    data_type: PortDataType,
) -> (Option<&'static str>, jack::PortFlags) {
    let ty = match data_type {
        PortDataType::Audio => Some("32 bit float mono audio"),
        PortDataType::Midi => Some("8 bit raw midi"),
        PortDataType::Any => None,
    };
    let flags = match direction {
        PortDirection::Input => jack::PortFlags::IS_INPUT,
        PortDirection::Output => jack::PortFlags::IS_OUTPUT,
        PortDirection::Any => jack::PortFlags::empty(),
    };
    (ty, flags)
}
fn jack_full_name(client: &jack::Client, short_name: &str) -> String {
    format!("{}:{}", client.name(), short_name)
}
fn jack_external_ports(
    j: &JackBackend,
    direction: PortDirection,
    data_type: PortDataType,
) -> Vec<String> {
    let (ty, flags) = jack_flags(direction, data_type);
    j.client().ports(None, ty, flags)
}
fn jack_connections_state_locked(
    j: &JackBackend,
    own_short: &str,
    direction: PortDirection,
    data_type: PortDataType,
) -> HashMap<String, bool> {
    let client = j.client();
    let own = jack_full_name(client, own_short);
    let connected = client
        .port_by_name(&own)
        .map(|p| p.get_connections())
        .unwrap_or_default();
    jack_external_ports(j, opposite_direction(direction), data_type)
        .into_iter()
        .map(|name| {
            let c = connected.iter().any(|n| n == &name);
            (name, c)
        })
        .collect()
}

fn dummy_connections_state_locked(
    external: &engine::DummyExternalConnections,
    request: &ConnectionCacheRequest,
) -> HashMap<String, bool> {
    let connected = request
        .session_index
        .map(|idx| external.connection_status_of(compat_port_id(idx)))
        .unwrap_or_default();
    let mut state = HashMap::new();
    if let Ok(ports) = external.find_external_ports(
        None,
        opposite_direction(request.direction).into(),
        request.data_type.into(),
    ) {
        for port in ports {
            state.insert(
                port.name.clone(),
                *connected.get(&port.name).unwrap_or(&false),
            );
        }
    }
    state
}

fn refresh_connection_cache(
    cache: Arc<Mutex<ConnectionCache>>,
    jack: Option<Arc<Mutex<JackBackend>>>,
    external: Option<Arc<Mutex<engine::DummyExternalConnections>>>,
    generation: u64,
) {
    let span = tracing::debug_span!(
        "worker.engine.connection_cache",
        generation,
        requests = tracing::field::Empty,
        published = tracing::field::Empty
    );
    let _entered = span.enter();
    let requests = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .requests
        .clone();
    span.record("requests", requests.len());
    let mut states = HashMap::new();
    if let Some(jack) = jack {
        let jack = jack.lock().unwrap_or_else(|e| e.into_inner());
        for (key, request) in &requests {
            states.insert(
                key.clone(),
                jack_connections_state_locked(
                    &jack,
                    &request.name,
                    request.direction,
                    request.data_type,
                ),
            );
        }
    } else if let Some(external) = external {
        let external = external.lock().unwrap_or_else(|e| e.into_inner());
        for (key, request) in &requests {
            states.insert(
                key.clone(),
                dummy_connections_state_locked(&external, request),
            );
        }
    }
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.generation == generation {
        cache.states = states;
        cache.refresh_in_flight = false;
        span.record("published", true);
    } else {
        span.record("published", false);
    }
}

fn jack_connect_port(
    jack: &Arc<Mutex<JackBackend>>,
    own_short: &str,
    direction: PortDirection,
    external: &str,
) {
    let j = jack.lock().unwrap_or_else(|e| e.into_inner());
    let client = j.client();
    let own = jack_full_name(client, own_short);
    let _ = if direction == PortDirection::Input {
        client.connect_ports_by_name(external, &own)
    } else {
        client.connect_ports_by_name(&own, external)
    };
}
fn jack_disconnect_port(
    jack: &Arc<Mutex<JackBackend>>,
    own_short: &str,
    direction: PortDirection,
    external: &str,
) {
    let j = jack.lock().unwrap_or_else(|e| e.into_inner());
    let client = j.client();
    let own = jack_full_name(client, own_short);
    let _ = if direction == PortDirection::Input {
        client.disconnect_ports_by_name(external, &own)
    } else {
        client.disconnect_ports_by_name(&own, external)
    };
}

#[derive(Clone)]
pub struct BackendSession {
    shared: Arc<SharedSession>,
}
unsafe impl Send for BackendSession {}
unsafe impl Sync for BackendSession {}
impl BackendSession {
    pub fn session_id(&self) -> u64 {
        self.shared.session_id
    }

    pub fn new() -> Result<Self> {
        Self::create()
    }
    #[tracing::instrument(name = "engine.control.create_session", skip_all)]
    pub fn create() -> Result<Self> {
        Self::create_with_capacity(COMMAND_QUEUE_CAPACITY)
    }

    fn create_with_capacity(command_queue_capacity: usize) -> Result<Self> {
        let mut s = engine::Session::default();
        s.apply_graph_changes().ok();
        // Capacity bounds how many control operations may be outstanding between cycles. A
        // session load issues them in bursts, so this is sized for a burst rather than for
        // the steady state.
        let (engine, handle) = engine::split(s, command_queue_capacity);
        let engine_connected = handle.connected_flag();
        let shared = Arc::new(SharedSession {
            session_id: NEXT_BACKEND_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            handle: Mutex::new(handle),
            engine_connected,
            parked: Mutex::new(Some(engine)),
            next_composite_slot: AtomicU32::new(0x8000_0000),
            next_composite_version: AtomicU32::new(1),
            composite_registry: Mutex::new(CompositeRegistry::default()),
            primitive_loop_controls: Mutex::new(Vec::new()),
            primitive_sync_sources: Mutex::new(Vec::new()),
            audio_snapshot_controls: Mutex::new(Vec::new()),
            external: Mutex::new(None),
            jack: Mutex::new(None),
            cpal: Mutex::new(None),
            connection_cache: Arc::new(Mutex::new(ConnectionCache::default())),
            sample_rate: AtomicU32::new(48_000),
            buffer_size: AtomicU32::new(256),
            snapshots: engine::content_snapshot::ContentSnapshotRuntime::new(),
            scheduler: OnceLock::new(),
        });

        // Attached after construction because the apply closure needs a handle back to the
        // session. `Weak`, so the scheduler thread cannot keep a closed session alive.
        let weak = Arc::downgrade(&shared);
        let scheduler = GraphScheduler::start(
            DEFAULT_WINDOW,
            Box::new(move || {
                let Some(shared) = weak.upgrade() else {
                    return;
                };

                // Describing needs the session, which the audio thread owns once a driver has
                // started, so this is a queued read. Two consequences worth stating:
                //
                // - It allocates, on the audio thread, inside the permission
                //   `Engine::apply_commands` already grants. Accepted because a rebuild
                //   happens on a topology change -- a session load, adding a track -- and
                //   never per cycle. See DEAD_STACK_B.md.
                // - Being queued, it is automatically ordered behind every mutation queued
                //   before it, so it cannot describe a session that is missing one of them.
                //   The lock version had to reason about that; this gets it for free.
                //
                // `None` means the graph was already current, so an armed-but-clean window
                // costs one round trip and no rebuild. The engine answers that question
                // rather than a flag on this side, because only it knows when the mutations
                // actually landed.
                // Cheap check first: an armed window whose graph is already current must not
                // cost a round trip, because every control operation arms one.
                if !shared.graph_may_need_rebuild() {
                    return;
                }
                let Ok(Some(topology)) =
                    shared.query_graph_scheduler_response(|s: &mut engine::Session| {
                        (!s.graph_up_to_date()).then(|| s.describe_topology())
                    })
                else {
                    return;
                };

                // The expensive part, on this thread: lowering, the topological sort, and
                // sizing every buffer a cycle will need.
                let Ok(prepared) = engine::build_schedule(topology) else {
                    return;
                };

                // Installing is moves only, and waited for rather than fired off. Waiting is
                // what keeps `flush_blocking` honest: it returns once this closure has run,
                // and callers like `AudioDriver::wait_process` are entitled to assume the
                // schedule has actually landed by then. Queueing it and returning would let
                // the flush finish while the install was still sitting in the queue.
                //
                // The schedule it displaces comes back as the return value and is dropped
                // here, on this thread -- never on the audio thread, where freeing is as
                // forbidden as allocating. That is why `install_schedule` hands it back.
                match shared.query_graph_scheduler_response(move |s: &mut engine::Session| {
                    s.install_schedule(prepared)
                }) {
                    Ok(displaced) => drop(displaced),
                    Err(_) => return,
                }
            }),
        );
        let _ = shared.scheduler.set(scheduler);

        Ok(Self { shared })
    }
    #[tracing::instrument(
        name = "engine.control.attach_driver",
        skip_all,
        fields(session_id = self.session_id())
    )]
    pub fn set_audio_driver(&self, driver: &AudioDriver) -> Result<()> {
        let state = driver.get_state();
        if state.sample_rate > 0 {
            self.shared
                .sample_rate
                .store(state.sample_rate, Ordering::Relaxed);
        }
        if state.buffer_size > 0 {
            self.shared
                .buffer_size
                .store(state.buffer_size, Ordering::Relaxed);
        }
        driver.attach_session(&self.shared);
        *self
            .shared
            .external
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(driver.external());
        *self.shared.jack.lock().unwrap_or_else(|e| e.into_inner()) = driver.jack();
        // Settle the graph before any audio thread exists, so the first cycle a backend
        // runs is already against a current schedule.
        self.shared.flush_graph_changes();
        self.shared.activate_jack(&self.shared)?;
        driver.activate_cpal(&self.shared)?;
        *self.shared.cpal.lock().unwrap_or_else(|e| e.into_inner()) = driver.cpal();
        Ok(())
    }
    pub fn poll_composite_trace(&self) -> Option<Vec<engine::BoundaryTraceEntry>> {
        self.shared
            .poll_trace(|snapshot| snapshot.composite_trace.clone())
    }

    pub fn wait_for_command(&self, sequence: CommandSequence, timeout: Duration) -> Result<()> {
        self.shared.wait_for_command(sequence, timeout)
    }

    pub fn get_state(&self) -> BackendSessionState {
        let handle = self.shared.handle.lock().unwrap_or_else(|e| e.into_inner());
        let stats = handle.stats();
        BackendSessionState {
            audio_driver: std::ptr::null_mut(),
            n_audio_buffers_created: 0,
            n_audio_buffers_available: 0,
            cycles: stats.cycles.load(Ordering::Relaxed),
            frames: stats.frames.load(Ordering::Relaxed),
            pending_commands: handle.n_pending().min(u32::MAX as usize) as u32,
            commands_applied: stats.commands_applied.load(Ordering::Relaxed),
            last_applied_command: stats.last_applied_command.load(Ordering::Relaxed),
            trace_snapshots_dropped: stats.trace_snapshots_dropped.load(Ordering::Relaxed),
            capture_underruns: stats.capture_underruns.load(Ordering::Relaxed),
            capture_overruns: stats.capture_overruns.load(Ordering::Relaxed),
            graph_arms: self
                .shared
                .scheduler
                .get()
                .map(GraphScheduler::n_arms)
                .unwrap_or(0),
            graph_applies: self
                .shared
                .scheduler
                .get()
                .map(GraphScheduler::n_applies)
                .unwrap_or(0),
            callback_last_ns: stats.callback_last_ns.load(Ordering::Relaxed),
            callback_worst_ns: stats.callback_worst_ns.load(Ordering::Relaxed),
            callback_budget_overruns: stats.callback_budget_overruns.load(Ordering::Relaxed),
            schedule_request_id: stats.schedule_request_id.load(Ordering::Relaxed),
            schedule_applied_id: stats.schedule_applied_id.load(Ordering::Relaxed),
            stuck_cycles: stats.stuck_cycles.load(Ordering::Relaxed),
            stale_cycles: stats.stale_cycles.load(Ordering::Relaxed),
            sub_blocks_last_cycle: stats.sub_blocks_last_cycle.load(Ordering::Relaxed),
        }
    }
    /// Adds a loop and returns its stable handle as soon as creation is queued.
    #[tracing::instrument(
        name = "engine.control.create_loop",
        skip_all,
        fields(session_id = self.session_id())
    )]
    pub fn create_loop(&self) -> Result<Loop> {
        let control = Arc::new(ObjectControl::<LoopId, engine::LoopStateMirror>::pending(
            self.shared.session_id,
        ));
        let control_for_command = Arc::downgrade(&control);
        let sequence = self.shared.send_topology(move |s: &mut engine::Session| {
            if let Some(control) = control_for_command.upgrade() {
                let idx = s.create_loop_with_state(Arc::clone(&control.mirror));
                control.mark_ready(LoopId(idx));
            }
        })?;
        control.set_creation_sequence(sequence);
        self.shared
            .primitive_loop_controls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(PrimitiveLoopControl {
                control: Arc::downgrade(&control),
                engine_index: Arc::clone(&control.engine_index),
                desired_source: None,
                applied_source_index: Arc::new(AtomicUsize::new(INVALID_OBJECT_INDEX)),
            });
        Ok(Loop {
            shared: self.shared.clone(),
            control,
        })
    }
    #[tracing::instrument(
        name = "engine.control.create_composite",
        skip_all,
        fields(session_id = self.session_id())
    )]
    pub fn create_composite_loop(&self) -> Result<CompositeLoop> {
        let slot = self
            .shared
            .next_composite_slot
            .fetch_add(1, Ordering::Relaxed);
        if slot == u32::MAX {
            return Err(anyhow!("composite identity capacity exhausted"));
        }
        let identity = engine::LoopIdentity {
            slot,
            generation: 1,
            kind: engine::LoopTargetKind::Composite,
        };
        let control = Arc::new(
            ObjectControl::<CompositeId, engine::CompositeStateMirror>::pending_with_mirror(
                self.shared.session_id,
                Arc::new(engine::CompositeStateMirror::new(identity)),
            ),
        );
        let weak = Arc::downgrade(&control);
        let sequence = self.shared.send_control(move |_session| {
            if let Some(control) = weak.upgrade() {
                control.mark_ready(CompositeId(slot as usize));
            }
        })?;
        control.set_creation_sequence(sequence);
        Ok(CompositeLoop {
            shared: self.shared.clone(),
            control,
            desired_play_after_record: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn remove_loop(&self, loop_: &Loop) -> Result<CommandSequence> {
        if !Arc::ptr_eq(&self.shared, &loop_.shared) {
            return Err(anyhow!("loop belongs to another session"));
        }
        let control = Arc::clone(&loop_.control);
        Ok(self.shared.send_topology(move |session| {
            if let Some(index) = control.ready_id().map(ObjectIdentity::index) {
                let _ = session.remove_loop(index);
            }
            control.mark_closed();
        })?)
    }

    pub fn remove_audio_port(&self, port: &AudioPort) -> Result<CommandSequence> {
        if !Arc::ptr_eq(&self.shared, &port.shared) {
            return Err(anyhow!("audio port belongs to another session"));
        }
        let control = Arc::clone(&port.control);
        Ok(self.shared.send_topology(move |session| {
            if let Some(index) = control.ready_id().map(ObjectIdentity::index) {
                let _ = session.remove_port(index);
            }
            control.mark_closed();
        })?)
    }

    pub fn remove_midi_port(&self, port: &MidiPort) -> Result<CommandSequence> {
        if !Arc::ptr_eq(&self.shared, &port.shared) {
            return Err(anyhow!("MIDI port belongs to another session"));
        }
        let control = Arc::clone(&port.control);
        Ok(self.shared.send_topology(move |session| {
            if let Some(index) = control.ready_id().map(ObjectIdentity::index) {
                let _ = session.remove_port(index);
            }
            control.mark_closed();
        })?)
    }

    pub fn remove_processor(&self, title: &str) -> Result<CommandSequence> {
        let title = title.to_owned();
        Ok(self.shared.send_topology(move |session| {
            session.remove_processor(&title);
        })?)
    }

    pub fn remove_fx_chain(&self, chain: &FXChain) -> Result<CommandSequence> {
        if !Arc::ptr_eq(&self.shared, &chain.shared) {
            return Err(anyhow!("FX chain belongs to another session"));
        }
        let title = chain.title.clone();
        let audio = chain
            .audio_inputs
            .iter()
            .chain(&chain.audio_outputs)
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        let midi = chain
            .midi_inputs
            .iter()
            .chain(&chain.midi_outputs)
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        Ok(self.shared.send_topology(move |session| {
            session.remove_processor(&title);
            for control in &audio {
                if let Some(index) = control.ready_id().map(ObjectIdentity::index) {
                    let _ = session.remove_port(index);
                }
                control.mark_closed();
            }
            for control in &midi {
                if let Some(index) = control.ready_id().map(ObjectIdentity::index) {
                    let _ = session.remove_port(index);
                }
                control.mark_closed();
            }
        })?)
    }

    pub fn primitive_sync_sources(&self) -> Vec<Option<usize>> {
        self.primitive_sync_sources_if_ready().unwrap_or_else(|| {
            self.shared
                .primitive_sync_sources
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        })
    }

    /// Returns no topology while a live primitive handle is still pending. Callers can retry
    /// on a later refresh without blocking that refresh on an engine command fence.
    pub fn primitive_sync_sources_if_ready(&self) -> Option<Vec<Option<usize>>> {
        let controls = self
            .shared
            .primitive_loop_controls
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if controls.iter().any(|entry| {
            entry
                .control
                .upgrade()
                .is_some_and(|control| control.lifecycle() == ObjectLifecycle::Pending)
                || entry
                    .desired_source
                    .as_ref()
                    .and_then(Weak::upgrade)
                    .is_some_and(|source| source.lifecycle() == ObjectLifecycle::Pending)
        }) {
            return None;
        }
        let mut result = self
            .shared
            .primitive_sync_sources
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for entry in controls.iter() {
            let index = entry.engine_index.load(Ordering::Acquire);
            if index == INVALID_OBJECT_INDEX {
                continue;
            }
            if result.len() <= index {
                result.resize(index + 1, None);
            }
            let source_index = entry.applied_source_index.load(Ordering::Acquire);
            result[index] = (source_index != INVALID_OBJECT_INDEX).then_some(source_index);
        }
        Some(result.clone())
    }

    pub fn install_composite_timeline(
        &self,
        mut timeline: engine::CompositeBoundaryTimeline,
        primitive_sync_sources: &[Option<usize>],
    ) -> Result<u64> {
        let version = u64::from(
            self.shared
                .next_composite_version
                .fetch_add(1, Ordering::Relaxed),
        );
        timeline
            .prepare_install(version, primitive_sync_sources)
            .map_err(|error| {
                anyhow!(
                    "could not prepare composite timeline: {error}; primitive sync sources: {primitive_sync_sources:?}"
                )
            })?;
        match self.shared.query_graph_scheduler_response(move |session| {
            session.install_prepared_composite_timeline(timeline)
        })? {
            Ok(reclaimed) => {
                drop(reclaimed);
                Ok(version)
            }
            Err(rejected) => Err(anyhow!(
                "engine rejected composite timeline version {version}: {}",
                rejected.error
            )),
        }
    }
    pub fn configure_composite_loop(
        &self,
        composite: &CompositeLoop,
        descriptor: engine::CompositePlanDescriptor,
        sync_source: engine::LoopIdentity,
        metadata: Vec<engine::LoopTargetMetadata>,
        primitive_sync_sources: &[Option<usize>],
    ) -> Result<u64> {
        if descriptor.source != composite.identity()
            || !Arc::ptr_eq(&self.shared, &composite.shared)
        {
            return Err(anyhow!(
                "composite configuration belongs to another session"
            ));
        }
        let mut registry = self
            .shared
            .composite_registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut candidate = registry.clone();
        candidate.configs.insert(
            composite.identity(),
            CompositeConfig {
                descriptor,
                sync_source,
                state: Arc::clone(&composite.control.mirror),
            },
        );
        for item in metadata {
            candidate.metadata.insert(item.identity, item);
        }
        let topology: Vec<_> = candidate
            .configs
            .values()
            .map(|config| {
                (
                    config.descriptor.source,
                    config.sync_source,
                    config
                        .descriptor
                        .timelines
                        .iter()
                        .flat_map(|timeline| &timeline.sections)
                        .flat_map(|section| &section.entries)
                        .map(|entry| entry.target)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let timeline = compile_composite_registry(&candidate)?;
        let version = self
            .install_composite_timeline(timeline, primitive_sync_sources)
            .map_err(|error| anyhow!("{error}; composite topology: {topology:?}"))?;
        *registry = candidate;
        Ok(version)
    }

    /// Compiles a composite registry update off the realtime thread and queues activation.
    /// The returned acknowledgement is polled by the frontend; displaced or rejected plans
    /// are always destroyed by the acknowledgement worker, never by the engine callback.
    #[tracing::instrument(
        name = "engine.control.configure_composite",
        skip_all,
        fields(session_id = self.session_id())
    )]
    pub fn configure_composite_loop_queued(
        &self,
        composite: &CompositeLoop,
        descriptor: engine::CompositePlanDescriptor,
        sync_source: engine::LoopIdentity,
        metadata: Vec<engine::LoopTargetMetadata>,
        primitive_sync_sources: &[Option<usize>],
        play_after_record: bool,
    ) -> Result<CompositeInstallAck> {
        if descriptor.source != composite.identity()
            || !Arc::ptr_eq(&self.shared, &composite.shared)
        {
            return Err(anyhow!(
                "composite configuration belongs to another session"
            ));
        }
        let mut registry = self
            .shared
            .composite_registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut candidate = registry.clone();
        candidate.configs.insert(
            composite.identity(),
            CompositeConfig {
                descriptor,
                sync_source,
                state: Arc::clone(&composite.control.mirror),
            },
        );
        for item in metadata {
            candidate.metadata.insert(item.identity, item);
        }
        let mut timeline = compile_composite_registry(&candidate)?;
        let version = u64::from(
            self.shared
                .next_composite_version
                .fetch_add(1, Ordering::Relaxed),
        );
        timeline
            .prepare_install(version, primitive_sync_sources)
            .map_err(|error| anyhow!("could not prepare composite timeline: {error}"))?;
        let source = composite.identity();
        let desired_play_after_record = Arc::clone(&composite.desired_play_after_record);
        let (sequence, receiver) = self.shared.queue_result(move |session| {
            // A setter can be queued while this installation is pending. Keep its desired value
            // separate from the state mirror, which realtime publication may overwrite with the
            // still-current engine value before this command is accepted.
            let desired_play_after_record =
                play_after_record || desired_play_after_record.load(Ordering::Acquire);
            match session.install_prepared_composite_timeline(timeline) {
                Ok(reclaimed) => {
                    let _ = session
                        .accept_composite_play_after_record(source, desired_play_after_record);
                    Ok(reclaimed)
                }
                Err(rejected) => Err(rejected),
            }
        })?;
        *registry = candidate;
        drop(registry);

        let outcome = Arc::new(Mutex::new(None));
        let worker_outcome = Arc::clone(&outcome);
        let _ = thread::Builder::new()
            .name("engine-composite-ack".to_string())
            .spawn(move || {
                let _span = tracing::info_span!(
                    "worker.engine.composite_ack",
                    sequence = sequence.get(),
                    version
                )
                .entered();
                // This wait runs on a reclamation worker, never on the GUI or realtime thread.
                // Instrumented and packaged CI builds can take longer than the ordinary command
                // fence while the frontend settles topology, so retain a bounded but generous
                // transaction timeout here.
                let result = match engine::wait_for_result(receiver, Duration::from_secs(30)) {
                    Ok(Ok(reclaimed)) => {
                        drop(reclaimed);
                        Ok(version)
                    }
                    Ok(Err(rejected)) => {
                        let error = rejected.error.to_string();
                        drop(rejected);
                        Err(format!(
                            "engine rejected composite timeline version {version}: {error}"
                        ))
                    }
                    Err(error) => Err(format!(
                        "engine did not acknowledge composite timeline version {version}: {error}"
                    )),
                };
                *worker_outcome
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(result);
            })
            .expect("spawn composite acknowledgement worker");
        Ok(CompositeInstallAck { sequence, outcome })
    }

    pub fn adopt_audio_ringbuffers(
        &self,
        requests: Vec<engine::AudioRingbufferAdoption>,
    ) -> Result<()> {
        let shape_requests = requests.clone();
        let shape = self
            .shared
            .query_graph_scheduler_response(move |session| {
                session.describe_audio_ringbuffer_adoption(&shape_requests)
            })??;
        let shapes: Vec<_> = shape.channels().collect();
        let mut prepared: Vec<_> = shapes
            .iter()
            .map(|channel| engine::PreparedAudioRingbufferAdoptionChannel {
                loop_idx: channel.loop_idx,
                channel_idx: channel.channel_idx,
                data: engine::PreparedAudioChannelData::new(channel.chunk_size, channel.capacity),
            })
            .collect();
        let prepare_requests = requests.clone();
        let (result, mut prepared) =
            self.shared.query_graph_scheduler_response(move |session| {
                let result =
                    session.prepare_audio_ringbuffers_prepared(&prepare_requests, &mut prepared);
                (result, prepared)
            })?;
        result?;

        let registered = self
            .shared
            .audio_snapshot_controls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let controls: Vec<_> = shapes
            .iter()
            .map(|shape| {
                registered
                    .iter()
                    .find_map(|(parent, channel, snapshots)| {
                        let parent = parent.upgrade()?;
                        let channel = channel.upgrade()?;
                        (parent.ready_id().map(ObjectIdentity::index) == Some(shape.loop_idx)
                            && channel.auxiliary_index() == Some(shape.channel_idx))
                        .then(|| snapshots.clone())
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "audio snapshot control is missing for loop {} channel {}",
                            shape.loop_idx,
                            shape.channel_idx
                        )
                    })
            })
            .collect::<Result<_>>()?;
        let mut snapshots = Vec::with_capacity(prepared.len());
        for (slot, control) in prepared.iter().zip(&controls) {
            let samples = slot.data.contiguous_copy();
            let Some(snapshot) = control.prepare(
                &samples,
                engine::content_snapshot::ContentMutation::RingbufferAdoption,
            ) else {
                for control in controls.iter().take(snapshots.len()) {
                    control.cancel();
                }
                return Err(anyhow!("audio snapshot channel is busy"));
            };
            snapshots.push(snapshot);
        }
        let install = self.shared.query_graph_scheduler_response(move |session| {
            let result = session.commit_audio_ringbuffers_prepared_with_snapshots(
                &requests,
                &mut prepared,
                &snapshots,
            );
            (result, prepared)
        });
        match install {
            Ok((Ok(()), returned)) => {
                drop(returned);
                Ok(())
            }
            Ok((Err(error), returned)) => {
                drop(returned);
                for control in controls {
                    control.cancel();
                }
                Err(error.into())
            }
            Err(error) => {
                for control in controls {
                    control.cancel();
                }
                Err(error)
            }
        }
    }

    pub fn remove_composite_loop(
        &self,
        composite: &CompositeLoop,
        primitive_sync_sources: &[Option<usize>],
    ) -> Result<u64> {
        if !Arc::ptr_eq(&self.shared, &composite.shared) {
            return Err(anyhow!("composite loop belongs to another session"));
        }

        let mut registry = self
            .shared
            .composite_registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let identity = composite.identity();
        if !registry.configs.contains_key(&identity) {
            composite.control.mark_closed();
            return Ok(0);
        }

        let mut removed = BTreeSet::from([identity]);
        loop {
            let dependents: Vec<_> = registry
                .configs
                .values()
                .filter(|config| {
                    !removed.contains(&config.descriptor.source)
                        && config
                            .descriptor
                            .timelines
                            .iter()
                            .flat_map(|timeline| &timeline.sections)
                            .flat_map(|section| &section.entries)
                            .any(|entry| removed.contains(&entry.target))
                })
                .map(|config| config.descriptor.source)
                .collect();
            if dependents.is_empty() {
                break;
            }
            removed.extend(dependents);
        }

        for &identity in &removed {
            let state = registry
                .configs
                .get(&identity)
                .ok_or_else(|| anyhow!("composite registry entry disappeared"))?
                .state
                .read();
            if state.installed && state.mode != LoopMode::Stopped {
                self.shared
                    .query_graph_scheduler_response(move |session| {
                        session.accept_composite_immediate_transition(
                            identity,
                            engine::LoopMode::Stopped,
                            0,
                        )
                    })?
                    .map_err(|error| anyhow!("composite stop rejected: {error}"))?;
            }
        }
        let deadline = Instant::now() + engine::DEFAULT_WAIT_TIMEOUT;
        for &identity in &removed {
            let mirror = Arc::clone(
                &registry
                    .configs
                    .get(&identity)
                    .ok_or_else(|| anyhow!("composite registry entry disappeared"))?
                    .state,
            );
            loop {
                let state = mirror.read();
                if !state.installed || state.mode == LoopMode::Stopped {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(anyhow!("composite did not stop before removal"));
                }
                thread::sleep(Duration::from_millis(1));
            }
        }

        let mut candidate = registry.clone();
        candidate
            .configs
            .retain(|identity, _| !removed.contains(identity));
        candidate
            .metadata
            .retain(|identity, _| !removed.contains(identity));
        let timeline = compile_composite_registry(&candidate)?;
        let version = self.install_composite_timeline(timeline, primitive_sync_sources)?;
        *registry = candidate;
        composite.control.mark_closed();
        Ok(version)
    }

    pub fn set_global_fx_midi_input(&self, port: &MidiPort) -> Result<CommandSequence> {
        let control = Arc::clone(&port.control);
        Ok(self.shared.send_topology(move |session| {
            if let Some(port) = control.ready_id() {
                let _ = session.set_global_fx_midi_input(port.index());
            }
        })?)
    }

    pub fn register_external_processor(
        &self,
        title: &str,
        audio_sends: &[AudioPort],
        audio_returns: &[AudioPort],
        midi_sends: &[MidiPort],
    ) -> Result<CommandSequence> {
        let title = title.to_owned();
        let audio_sends = audio_sends
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        let audio_returns = audio_returns
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        let midi_sends = midi_sends
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        Ok(self.shared.send_topology(move |session| {
            session.set_external_processor(title.clone());
            let audio_inputs = audio_sends
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let audio_outputs = audio_returns
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let midi_inputs = midi_sends
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let _ = session.set_processor_ports(&title, audio_inputs, audio_outputs, midi_inputs);
        })?)
    }

    #[tracing::instrument(
        name = "engine.control.create_fx",
        skip_all,
        fields(session_id = self.session_id(), chain_type = chain_type as u32)
    )]
    pub fn create_fx_chain(
        &self,
        chain_type: FXChainType,
        title: &str,
        output_ringbuffer_n_samples: u32,
    ) -> Result<FXChain> {
        self.create_fx_chain_with_channels(chain_type, title, None, output_ringbuffer_n_samples)
    }

    pub fn create_tiny_synth_fx_chain(
        &self,
        title: &str,
        channel_count: usize,
        output_ringbuffer_n_samples: u32,
    ) -> Result<FXChain> {
        self.create_fx_chain_with_channels(
            FXChainType::TinySynthFx,
            title,
            Some(channel_count),
            output_ringbuffer_n_samples,
        )
    }
    pub fn create_oxisynth_chain(
        &self,
        title: &str,
        output_ringbuffer_n_samples: u32,
    ) -> Result<FXChain> {
        self.create_fx_chain_with_channels(
            FXChainType::OxiSynth,
            title,
            Some(2),
            output_ringbuffer_n_samples,
        )
    }

    fn create_fx_chain_with_channels(
        &self,
        chain_type: FXChainType,
        title: &str,
        tiny_channels: Option<usize>,
        output_ringbuffer_n_samples: u32,
    ) -> Result<FXChain> {
        let backend = match chain_type {
            FXChainType::Test2x2x1 => FXChainBackendKind::Test2x2x1,
            FXChainType::TinySynthFx => {
                let channels = tiny_channels
                    .ok_or_else(|| anyhow!("Tiny Synth/FX requires an explicit channel count"))?;
                let sample_rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
                let buffer_size = self.shared.buffer_size.load(Ordering::Relaxed).max(1);
                let control =
                    engine::tiny_synth_fx::TinySynthFxControlState::new(sample_rate as f32)?;
                let processor = control.prepare_processor(
                    sample_rate as f32,
                    channels,
                    buffer_size as usize,
                )?;
                let mut pending = Some((title.to_owned(), processor));
                self.shared
                    .send_topology(move |session: &mut engine::Session| {
                        if let Some((title, processor)) = pending.take() {
                            let _ = session.set_tiny_synth_fx_processor(title, processor);
                        }
                    })?;
                FXChainBackendKind::Tiny(Mutex::new(control))
            }
            FXChainType::OxiSynth => {
                let sample_rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
                let buffer_size = self.shared.buffer_size.load(Ordering::Relaxed).max(1);
                let processor = engine::oxisynth::OxiSynthProcessor::new(
                    sample_rate as f32,
                    buffer_size as usize,
                    engine::oxisynth::OxiSynthState::default(),
                )?;
                let mut pending = Some((title.to_owned(), processor));
                self.shared.send_topology(move |session| {
                    if let Some((title, processor)) = pending.take() {
                        let _ = session.set_oxisynth_processor(title, processor);
                    }
                })?;
                FXChainBackendKind::OxiSynth
            }
            FXChainType::CarlaRack | FXChainType::CarlaPatchbay | FXChainType::CarlaPatchbay16x => {
                #[cfg(feature = "carla")]
                {
                    let sample_rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
                    let buffer_size = self.shared.buffer_size.load(Ordering::Relaxed).max(1);
                    let host: Result<Box<dyn engine::carla_processor::CarlaProcessor>> =
                        match carla_hosting_mode() {
                            CarlaHostingMode::InProcess => {
                                engine::carla_native::CarlaNativeHost::instantiate(
                                    chain_type,
                                    sample_rate,
                                    buffer_size,
                                )
                                .map(|host| Box::new(host) as Box<_>)
                                .map_err(|error| {
                                    anyhow!(
                                        "in-process Carla host initialization failed: {error:#}"
                                    )
                                })
                            }
                            CarlaHostingMode::Subprocess => {
                                let chain_id = NEXT_CARLA_CHAIN_ID.fetch_add(1, Ordering::Relaxed);
                                std::env::current_exe()
                                    .map_err(|error| {
                                        anyhow!("could not locate Carla worker executable: {error}")
                                    })
                                    .and_then(|executable| {
                                        engine::carla_subprocess::SupervisedCarlaProcessor::launch(
                                            executable,
                                            chain_type,
                                            sample_rate,
                                            buffer_size,
                                            shoop_plugin_protocol::ChainId(chain_id),
                                        )
                                        .map(|host| Box::new(host) as Box<_>)
                                        .map_err(|error| {
                                            anyhow!(
                                                "Carla subprocess launch/handshake failed: {error:#}"
                                            )
                                        })
                                    })
                            }
                        };
                    match host.and_then(|host| {
                        engine::carla_processor::spawn_processor_bridge(
                            host,
                            sample_rate,
                            buffer_size,
                        )
                    }) {
                        Ok((control, realtime)) => {
                            let mut pending = Some((title.to_string(), realtime));
                            if let Err(error) =
                                self.shared.send_topology(move |s: &mut engine::Session| {
                                    if let Some((title, realtime)) = pending.take() {
                                        s.set_carla_fx_host(title, realtime);
                                    }
                                })
                            {
                                log::error!("could not queue Carla endpoint insertion: {error}");
                            }
                            FXChainBackendKind::Carla(control)
                        }
                        Err(error) => {
                            log::error!("could not initialize Carla FX chain {title:?}: {error:#}");
                            FXChainBackendKind::Unavailable {
                                reason: error.to_string(),
                            }
                        }
                    }
                }
                #[cfg(not(feature = "carla"))]
                {
                    FXChainBackendKind::Unavailable {
                        reason: "shoop_engine was built without Carla Native support".to_string(),
                    }
                }
            }
        };
        if matches!(backend, FXChainBackendKind::Test2x2x1) {
            let title = title.to_owned();
            self.shared.send_topology(move |session| {
                session.set_test_fx_active(title.clone(), false);
            })?;
        }
        let mut chain = FXChain {
            shared: self.shared.clone(),
            title: title.to_string(),
            backend,
            state: Arc::new(Mutex::new(FXChainState::default())),
            tiny_channels: tiny_channels.unwrap_or(0),
            output_ringbuffer_n_samples: output_ringbuffer_n_samples as usize,
            audio_inputs: Vec::new(),
            audio_outputs: Vec::new(),
            midi_inputs: Vec::new(),
            midi_outputs: Vec::new(),
        };
        chain.create_ports_once();
        chain.bind_processor_ports()?;
        Ok(chain)
    }
    pub fn get_profiling_report(&self) -> ProfilingReport {
        self.shared
            .query_graph_scheduler_response(|session| session.profiling_report())
            .unwrap_or_default()
    }
    pub fn segfault_on_process_thread(&self) {}
    pub fn abort_on_process_thread(&self) {}
}

pub struct JackAudioDriverSettings {
    pub client_name_hint: String,
    pub maybe_server_name: Option<String>,
}
impl std::fmt::Debug for JackAudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JackAudioDriverSettings")
            .field("client_name_hint", &self.client_name_hint)
            .field("maybe_server_name", &self.maybe_server_name)
            .finish()
    }
}
pub struct DummyAudioDriverSettings {
    pub client_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

#[derive(Clone)]
pub struct CpalMidiAudioDriverSettings {
    pub client_name: String,
    pub host: String,
    pub output_device: String,
    pub input_device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub output_channels: String,
    pub input_channels: String,
    pub capture_ring_frames: u32,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
}
impl std::fmt::Debug for DummyAudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DummyAudioDriverSettings")
            .field("client_name", &self.client_name)
            .field("sample_rate", &self.sample_rate)
            .field("buffer_size", &self.buffer_size)
            .finish()
    }
}
impl std::fmt::Debug for CpalMidiAudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalMidiAudioDriverSettings")
            .field("client_name", &self.client_name)
            .field("host", &self.host)
            .field("output_device", &self.output_device)
            .field("input_device", &self.input_device)
            .field("sample_rate", &self.sample_rate)
            .field("buffer_size", &self.buffer_size)
            .field("output_channels", &self.output_channels)
            .field("input_channels", &self.input_channels)
            .field("capture_ring_frames", &self.capture_ring_frames)
            .field("midi_inputs", &self.midi_inputs)
            .field("midi_outputs", &self.midi_outputs)
            .finish()
    }
}
pub enum AudioDriverSettings {
    Jack(JackAudioDriverSettings),
    Dummy(DummyAudioDriverSettings),
    Cpal(CpalMidiAudioDriverSettings),
}
impl std::fmt::Debug for AudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jack(s) => f.debug_tuple("Jack").field(s).finish(),
            Self::Dummy(s) => f.debug_tuple("Dummy").field(s).finish(),
            Self::Cpal(s) => f.debug_tuple("Cpal").field(s).finish(),
        }
    }
}

pub type AudioDriverState = engine::AudioDriverState;

type ProcessCallback = unsafe extern "C" fn();
fn driver_uses_dummy_processing(driver_type: AudioDriverType) -> bool {
    matches!(
        driver_type,
        AudioDriverType::Dummy | AudioDriverType::JackTest | AudioDriverType::CpalTest
    )
}
struct DriverInner {
    driver_type: AudioDriverType,
    /// Settings, lifecycle and cycle chunking, for every driver type.
    ///
    /// Named for the dummy driver because that is where its chunking matters, but it is
    /// the one place the settings and the active flag live regardless of backend: JACK
    /// and CPAL take their cycle sizes from their own callbacks and only read the
    /// settings back out. Sharing the type is what stopped the chunk arithmetic from
    /// existing twice, once here and once in `dummy_driver.rs` where the tests were.
    dummy: engine::DummyDriver,
    last_processed: u32,
    process_generation: u64,
    finish: Arc<AtomicBool>,
    dummy_thread: Option<thread::JoinHandle<()>>,
    session: Option<Weak<SharedSession>>,
    jack: Option<Arc<Mutex<JackBackend>>>,
    cpal: Option<Arc<Mutex<CpalBackend>>>,
    cpal_settings: Option<CpalMidiAudioDriverSettings>,
    cpal_decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
    maybe_process_callback: Option<ProcessCallback>,
}
pub struct AudioDriver {
    inner: Arc<Mutex<DriverInner>>,
}

/// One turn of the dummy driver's loop.
///
/// `engine` is the thread's own slot, empty until a session has been attached and its engine
/// claimed. Taken lazily rather than at construction because the thread is started by
/// `AudioDriver::start`, which runs before `BackendSession::set_audio_driver` exists to give
/// it anything.
fn process_dummy_driver_iteration(
    inner: &Arc<Mutex<DriverInner>>,
    engine: &mut Option<engine::Engine>,
) {
    crate::realtime_lock_guard::forbid_locks_if_enabled(|| {
        process_dummy_driver_iteration_inner(inner, engine)
    });
}

fn process_dummy_driver_iteration_inner(
    inner: &Arc<Mutex<DriverInner>>,
    engine: &mut Option<engine::Engine>,
) {
    let (session, n, sample_rate, buffer_size, callback, driver_type) = {
        let mut i = crate::realtime_allow_lock!("dummy driver iteration state", inner.lock())
            .unwrap_or_else(|e| e.into_inner());
        if !i.dummy.active() || !driver_uses_dummy_processing(i.driver_type) {
            i.last_processed = 0;
            i.process_generation = i.process_generation.wrapping_add(1);
            return;
        }
        // Chunking, including the controlled-mode bookkeeping, belongs to `DummyDriver`:
        // automatic mode hands out a whole buffer, controlled mode only what was asked
        // for and consumes it from the request.
        let n = i.dummy.next_chunk();
        i.process_generation = i.process_generation.wrapping_add(1);
        (
            i.session.as_ref().and_then(|w| w.upgrade()),
            n,
            i.dummy.sample_rate(),
            i.dummy.buffer_size(),
            i.maybe_process_callback,
            i.driver_type,
        )
    };
    let _driver_kind =
        shoop_tracing::realtime_span!("engine.rt.driver", value = driver_type as i32);
    let _span = shoop_tracing::realtime_span!("engine.rt.driver.dummy", value = n);

    if let Some(callback) = callback {
        unsafe {
            callback();
        }
    }

    if engine.is_none() {
        if let Some(shared) = session.as_ref() {
            *engine = shared.take_engine();
        }
    }
    let Some(engine) = engine.as_mut() else {
        if n == 0 {
            crate::realtime_allow_lock!("dummy driver idle state", inner.lock())
                .unwrap_or_else(|e| e.into_inner())
                .last_processed = 0;
        }
        return;
    };

    // Every turn, whether or not a cycle runs. This is what stops controlled mode from
    // starving the control side: a test that has requested no frames still expects its
    // queued reads to be answered, and without this they would sit until they timed out.
    engine.pump();

    if n == 0 {
        crate::realtime_allow_lock!("dummy driver zero-frame state", inner.lock())
            .unwrap_or_else(|e| e.into_inner())
            .last_processed = 0;
        return;
    }

    {
        let s = engine.session_mut();
        s.set_sample_rate(sample_rate);
        s.set_buffer_size(buffer_size);
    }
    // A process path, not a control path: it must not trigger a reschedule. Pending graph
    // changes are applied by the scheduler thread within its window; a caller that needs them
    // landed first calls `AudioDriver::wait_process`, which flushes.
    engine.run_cycle(n as usize);
    crate::realtime_allow_lock!("dummy driver completion state", inner.lock())
        .unwrap_or_else(|e| e.into_inner())
        .last_processed = n;
}

fn wait_for_dummy_generation(inner: &Arc<Mutex<DriverInner>>, target: u64, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .process_generation
            >= target
        {
            return;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

unsafe impl Send for AudioDriver {}
unsafe impl Sync for AudioDriver {}
impl AudioDriver {
    #[tracing::instrument(
        name = "engine.driver.create",
        skip_all,
        fields(driver_type = driver_type as u32)
    )]
    pub fn new(
        driver_type: AudioDriverType,
        _maybe_callback: Option<ProcessCallback>,
    ) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(DriverInner {
                driver_type,
                dummy: engine::DummyDriver::default(),
                last_processed: 0,
                process_generation: 0,
                finish: Arc::new(AtomicBool::new(false)),
                dummy_thread: None,
                session: None,
                jack: None,
                cpal: None,
                cpal_settings: None,
                cpal_decoupled_midi_ports: Arc::new(Mutex::new(Vec::new())),
                maybe_process_callback: _maybe_callback,
            })),
        })
    }
    fn external(&self) -> Arc<Mutex<engine::DummyExternalConnections>> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .dummy
            .external()
            .clone()
    }
    fn jack(&self) -> Option<Arc<Mutex<JackBackend>>> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .jack
            .clone()
    }
    fn cpal(&self) -> Option<Arc<Mutex<CpalBackend>>> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cpal
            .clone()
    }
    fn attach_session(&self, shared: &Arc<SharedSession>) {
        self.inner.lock().unwrap().session = Some(Arc::downgrade(shared));
    }
    fn activate_cpal(&self, shared: &Arc<SharedSession>) -> Result<()> {
        let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if i.driver_type != AudioDriverType::Cpal && i.driver_type != AudioDriverType::CpalTest
            || i.cpal.is_some()
        {
            return Ok(());
        }
        let settings = i
            .cpal_settings
            .clone()
            .ok_or_else(|| anyhow!("CPAL settings missing"))?;
        let external = i.dummy.external().clone();
        let backend = if i.driver_type == AudioDriverType::CpalTest {
            // No engine: the mock host builds no stream, so `CpalTest` is cycled by the dummy
            // thread like the other test drivers. Taking the engine here would leave the
            // session owned by something that never runs it, and every control call would
            // then wait out its timeout.
            CpalBackend::start_with_mock(
                &settings,
                external,
                i.cpal_decoupled_midi_ports.clone(),
                i.maybe_process_callback,
            )?
        } else {
            // Handed over for good: from here the output callback is the session's only owner.
            let engine = shared
                .take_engine()
                .ok_or_else(|| anyhow!("the engine has already been taken by another driver"))?;
            CpalBackend::start(
                engine,
                &settings,
                external,
                i.cpal_decoupled_midi_ports.clone(),
                i.maybe_process_callback,
            )?
        };
        // What the device actually opened at, which is only known now.
        i.dummy.settings_mut().sample_rate = backend.sample_rate;
        i.dummy.settings_mut().buffer_size =
            backend.configured_buffer_size.max(settings.buffer_size);
        i.cpal = Some(Arc::new(Mutex::new(backend)));
        Ok(())
    }
    #[tracing::instrument(name = "engine.driver.start", skip_all)]
    pub fn start(&self, settings: &AudioDriverSettings) -> Result<()> {
        let mut i = self.inner.lock().unwrap();
        // Settled here as a local and handed to the driver once at the end, rather than
        // patched in place: the JACK branch has to open a client to learn the name and
        // rate it actually got, so the values arriving here are provisional either way.
        let mut resolved = match settings {
            AudioDriverSettings::Dummy(s) => engine::DriverSettings {
                sample_rate: s.sample_rate,
                buffer_size: s.buffer_size,
                client_name: s.client_name.clone(),
            },
            AudioDriverSettings::Jack(s) => engine::DriverSettings {
                client_name: s.client_name_hint.clone(),
                ..Default::default()
            },
            AudioDriverSettings::Cpal(s) => engine::DriverSettings {
                client_name: s.client_name.clone(),
                ..Default::default()
            },
        };
        if i.driver_type == AudioDriverType::Jack {
            let (client, _status) =
                jack::Client::new(&resolved.client_name, jack::ClientOptions::NO_START_SERVER)
                    .map_err(|e| anyhow!("Failed to open JACK client: {e}"))?;
            resolved.client_name = client.name().to_string();
            resolved.sample_rate = client.sample_rate();
            resolved.buffer_size = client.buffer_size();
            i.jack = Some(Arc::new(Mutex::new(JackBackend {
                client: Some(client),
                active_client: None,
                ports: Arc::new(Mutex::new(Vec::new())),
                last_processed: Arc::new(AtomicU32::new(0)),
                xruns: Arc::new(AtomicU32::new(0)),
                stale_graph_cycles: Arc::new(AtomicU32::new(0)),
                maybe_process_callback: i.maybe_process_callback,
            })));
        } else {
            i.jack = None;
        }
        if matches!(
            i.driver_type,
            AudioDriverType::Cpal | AudioDriverType::CpalTest
        ) {
            let cpal_settings = match settings {
                AudioDriverSettings::Cpal(s) => s.clone(),
                _ => return Err(anyhow!("CPAL driver requires CPAL settings")),
            };
            // Unknown until the device opens; `activate_cpal` fills them in.
            resolved.sample_rate = 0;
            resolved.buffer_size = 0;
            resolved.client_name = cpal_settings.client_name.clone();
            i.cpal_settings = Some(cpal_settings);
        } else {
            i.cpal = None;
            i.cpal_settings = None;
        }
        if i.driver_type == AudioDriverType::JackTest {
            i.dummy.remove_all_external_mock_ports();
            for client in [
                "test_client_1",
                "test_client_2",
                resolved.client_name.as_str(),
            ] {
                i.dummy.add_external_mock_port(
                    format!("{client}:audio_in"),
                    engine::PortDirection::Input,
                    engine::PortDataType::Audio,
                );
                i.dummy.add_external_mock_port(
                    format!("{client}:audio_out"),
                    engine::PortDirection::Output,
                    engine::PortDataType::Audio,
                );
                i.dummy.add_external_mock_port(
                    format!("{client}:midi_in"),
                    engine::PortDirection::Input,
                    engine::PortDataType::Midi,
                );
                i.dummy.add_external_mock_port(
                    format!("{client}:midi_out"),
                    engine::PortDirection::Output,
                    engine::PortDataType::Midi,
                );
            }
        }
        i.dummy.start(resolved);
        if driver_uses_dummy_processing(i.driver_type) && i.dummy_thread.is_none() {
            i.finish.store(false, Ordering::Relaxed);
            let inner = self.inner.clone();
            let finish = i.finish.clone();
            i.dummy_thread = Some(
                thread::Builder::new()
                    .name("engine-dummy-driver".to_string())
                    .spawn(move || {
                        shoop_tracing::prewarm_realtime_thread("engine-dummy-driver");
                        let _span = tracing::info_span!("worker.engine.dummy_driver").entered();
                        // Claimed on the first turn after a session is attached, and owned by this
                        // thread from then on.
                        let mut engine: Option<engine::Engine> = None;
                        while !finish.load(Ordering::Relaxed) {
                            let (sample_rate, buffer_size) = {
                                let i = inner.lock().unwrap_or_else(|e| e.into_inner());
                                (i.dummy.sample_rate().max(1), i.dummy.buffer_size().max(1))
                            };
                            let micros = ((buffer_size as f64 / sample_rate as f64) * 1_000_000.0)
                                .ceil()
                                .max(1.0) as u64;
                            let started = Instant::now();
                            process_dummy_driver_iteration(&inner, &mut engine);
                            let interval = Duration::from_micros(micros);

                            // Sleep in slices, draining control work between cycles rather than
                            // through them. A blocking read from the application thread would otherwise
                            // wait out the whole cycle interval. Only pump when something is actually
                            // queued, so an idle driver still sleeps.
                            const SLICE: Duration = Duration::from_micros(100);
                            while started.elapsed() < interval {
                                if finish.load(Ordering::Relaxed) {
                                    break;
                                }
                                if let Some(e) = engine.as_mut() {
                                    if e.has_pending_commands() {
                                        e.pump();
                                    }
                                }
                                let left = interval.saturating_sub(started.elapsed());
                                thread::sleep(SLICE.min(left));
                            }
                        }

                        // Hand the engine back before this thread ends. Dropping it here would destroy
                        // the session on this thread, and a session holding Carla Native hosts does not
                        // survive being torn down off the thread that created its plugins. It would
                        // also leave the session unreachable for whatever outlives this driver.
                        if let Some(e) = engine.take() {
                            let session = inner
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .session
                                .as_ref()
                                .and_then(|w| w.upgrade());
                            if let Some(shared) = session {
                                shared.return_engine(e);
                            }
                        }
                    })
                    .expect("spawn dummy driver thread"),
            );
        }
        Ok(())
    }
    fn register_audio_port(
        &self,
        name: &str,
        direction: PortDirection,
        control: Arc<ObjectControl<AudioPortId, engine::AudioPortStateMirror>>,
    ) -> Result<()> {
        if let Some(jack) = self.jack() {
            let j = jack.lock().unwrap_or_else(|e| e.into_inner());
            match direction {
                PortDirection::Input => {
                    let p = j
                        .client()
                        .register_port(name, jack::AudioIn::default())
                        .map_err(|e| anyhow!("Failed to register JACK audio input {name}: {e}"))?;
                    j.ports.lock().unwrap_or_else(|e| e.into_inner()).push(
                        JackRegisteredPort::AudioIn {
                            control: Arc::clone(&control),
                            jack: p,
                        },
                    );
                }
                _ => {
                    let p = j
                        .client()
                        .register_port(name, jack::AudioOut::default())
                        .map_err(|e| anyhow!("Failed to register JACK audio output {name}: {e}"))?;
                    j.ports.lock().unwrap_or_else(|e| e.into_inner()).push(
                        JackRegisteredPort::AudioOut {
                            control: Arc::clone(&control),
                            jack: p,
                        },
                    );
                }
            }
        }
        Ok(())
    }
    pub fn unregister_audio_port(&self, port: &AudioPort) -> Result<()> {
        let Some(jack) = self.jack() else {
            return Ok(());
        };
        let jack = jack.lock().unwrap_or_else(|error| error.into_inner());
        let registered = {
            let mut ports = jack.ports.lock().unwrap_or_else(|error| error.into_inner());
            ports
                .iter()
                .position(|registered| match registered {
                    JackRegisteredPort::AudioIn { control, .. }
                    | JackRegisteredPort::AudioOut { control, .. } => {
                        Arc::ptr_eq(control, &port.control)
                    }
                    _ => false,
                })
                .map(|index| ports.remove(index))
        };
        match registered {
            Some(JackRegisteredPort::AudioIn { jack: port, .. }) => {
                jack.client().unregister_port(port)?;
            }
            Some(JackRegisteredPort::AudioOut { jack: port, .. }) => {
                jack.client().unregister_port(port)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn unregister_midi_port(&self, port: &MidiPort) -> Result<()> {
        let Some(jack) = self.jack() else {
            return Ok(());
        };
        let jack = jack.lock().unwrap_or_else(|error| error.into_inner());
        let registered = {
            let mut ports = jack.ports.lock().unwrap_or_else(|error| error.into_inner());
            ports
                .iter()
                .position(|registered| match registered {
                    JackRegisteredPort::MidiIn { control, .. }
                    | JackRegisteredPort::MidiOut { control, .. } => {
                        Arc::ptr_eq(control, &port.control)
                    }
                    _ => false,
                })
                .map(|index| ports.remove(index))
        };
        match registered {
            Some(JackRegisteredPort::MidiIn { jack: port, .. }) => {
                jack.client().unregister_port(port)?;
            }
            Some(JackRegisteredPort::MidiOut { jack: port, .. }) => {
                jack.client().unregister_port(port)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn register_midi_port(
        &self,
        name: &str,
        direction: PortDirection,
        control: Arc<ObjectControl<MidiPortId, engine::MidiPortStateMirror>>,
    ) -> Result<()> {
        if let Some(jack) = self.jack() {
            let j = jack.lock().unwrap_or_else(|e| e.into_inner());
            match direction {
                PortDirection::Input => {
                    let p = j
                        .client()
                        .register_port(name, jack::MidiIn::default())
                        .map_err(|e| anyhow!("Failed to register JACK MIDI input {name}: {e}"))?;
                    j.ports.lock().unwrap_or_else(|e| e.into_inner()).push(
                        JackRegisteredPort::MidiIn {
                            control: Arc::clone(&control),
                            jack: p,
                        },
                    );
                }
                _ => {
                    let p = j
                        .client()
                        .register_port(name, jack::MidiOut::default())
                        .map_err(|e| anyhow!("Failed to register JACK MIDI output {name}: {e}"))?;
                    j.ports.lock().unwrap_or_else(|e| e.into_inner()).push(
                        JackRegisteredPort::MidiOut {
                            control: Arc::clone(&control),
                            jack: p,
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn register_decoupled_midi_port(
        &self,
        name: &str,
        direction: PortDirection,
        port_id: engine::PortId,
        queue: Arc<Mutex<Vec<MidiEvent>>>,
    ) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cpal_decoupled_midi_ports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CpalDecoupledMidiPort {
                port_id,
                direction,
                queue: queue.clone(),
            });
        if let Some(jack) = self.jack() {
            let j = jack.lock().unwrap_or_else(|e| e.into_inner());
            match direction {
                PortDirection::Input => {
                    let p = j
                        .client()
                        .register_port(name, jack::MidiIn::default())
                        .map_err(|e| anyhow!("Failed to register JACK MIDI input {name}: {e}"))?;
                    j.ports
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(JackRegisteredPort::DecoupledMidiIn { queue, jack: p });
                }
                _ => {
                    let p = j
                        .client()
                        .register_port(name, jack::MidiOut::default())
                        .map_err(|e| anyhow!("Failed to register JACK MIDI output {name}: {e}"))?;
                    j.ports
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(JackRegisteredPort::DecoupledMidiOut { queue, jack: p });
                }
            }
        }
        Ok(())
    }
    pub fn get_sample_rate(&self) -> u32 {
        self.inner.lock().unwrap().dummy.sample_rate()
    }
    pub fn get_buffer_size(&self) -> u32 {
        self.inner.lock().unwrap().dummy.buffer_size()
    }
    pub fn active(&self) -> bool {
        self.inner.lock().unwrap().dummy.active()
    }
    /// Waits until the engine has caught up with everything asked of it.
    ///
    /// Flushes pending graph changes first, for all driver types. Without that, a caller
    /// that changes the topology and immediately advances the driver would run cycles
    /// against the previous schedule until the coalescing window elapsed -- fine for
    /// audio, wrong for a test asserting on the very next cycle. This is the suite's
    /// "let everything settle" call, so settling the graph belongs here.
    #[tracing::instrument(name = "engine.driver.wait_process", skip_all)]
    pub fn wait_process(&self) {
        let (is_dummy, target, session) = {
            let i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            (
                driver_uses_dummy_processing(i.driver_type),
                i.process_generation.saturating_add(2),
                i.session.as_ref().and_then(|w| w.upgrade()),
            )
        };
        if let Some(shared) = session {
            // Control work first, then the schedule that may depend on it, so that by the time
            // this returns both have landed.
            shared.drain_queue(Duration::from_millis(500));
            shared.flush_graph_changes();
        }
        if is_dummy {
            wait_for_dummy_generation(&self.inner, target, Duration::from_millis(100));
        }
    }
    pub fn get_state(&self) -> AudioDriverState {
        let i = self.inner.lock().unwrap();
        let (last_processed, xruns_since_last, stale_graph_cycles, dsp_load_percent) =
            if let Some(j) = i.jack.as_ref() {
                let j = j.lock().unwrap_or_else(|e| e.into_inner());
                (
                    j.last_processed.load(Ordering::Relaxed),
                    j.xruns.swap(0, Ordering::Relaxed),
                    j.stale_graph_cycles.load(Ordering::Relaxed),
                    // Pulled here rather than pushed from the callback: asking JACK costs
                    // a call the audio thread has no reason to make.
                    j.client().cpu_load(),
                )
            } else if let Some(c) = i.cpal.as_ref() {
                let c = c.lock().unwrap_or_else(|e| e.into_inner());
                (
                    c.last_processed.load(Ordering::Relaxed),
                    c.xruns.swap(0, Ordering::Relaxed),
                    c.stale_graph_cycles.load(Ordering::Relaxed),
                    // cpal exposes no equivalent; left unreported rather than invented.
                    0.0,
                )
            } else {
                (i.last_processed, 0, 0, 0.0)
            };
        AudioDriverState {
            dsp_load_percent,
            xruns_since_last,
            stale_graph_cycles,
            maybe_instance_name: i.dummy.client_name().to_string(),
            sample_rate: i.dummy.sample_rate(),
            buffer_size: i.dummy.buffer_size(),
            active: i.dummy.active() as u32,
            last_processed,
        }
    }
    pub fn dummy_enter_controlled_mode(&self) {
        let mut i = self.inner.lock().unwrap();
        i.dummy.enter_mode(engine::DriverMode::Controlled);
        // Explicit, because `enter_mode` keeps the request when the mode is unchanged: a
        // caller entering controlled mode wants to start from nothing requested.
        i.dummy.clear_request();
        i.last_processed = 0;
    }
    pub fn dummy_enter_automatic_mode(&self) {
        let mut i = self.inner.lock().unwrap();
        i.dummy.enter_mode(engine::DriverMode::Automatic);
        i.dummy.clear_request();
    }
    pub fn dummy_is_controlled(&self) -> bool {
        self.inner.lock().unwrap().dummy.mode() == engine::DriverMode::Controlled
    }
    pub fn dummy_wait_controlled_mode(&self) {
        // Synchronously drain all pending controlled frames by polling the
        // driver state directly.
        self.wait_process();
        while {
            let i = self.inner.lock().unwrap();
            i.last_processed != 0 || i.dummy.samples_to_process() != 0
        } {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.wait_process();
    }
    pub fn dummy_request_controlled_frames(&self, n: u32) {
        self.inner.lock().unwrap().dummy.request_samples(n);
    }
    pub fn dummy_n_requested_frames(&self) -> u32 {
        self.inner.lock().unwrap().dummy.samples_to_process()
    }
    pub fn dummy_run_requested_frames(&self) {
        self.wait_process();
        let start = Instant::now();
        while self.dummy_is_controlled()
            && self.dummy_n_requested_frames() > 0
            && start.elapsed() < Duration::from_millis(100)
        {
            thread::sleep(Duration::from_millis(1));
        }
        self.wait_process();
    }
    pub fn dummy_add_external_mock_port(&self, name: &str, direction: u32, data_type: u32) {
        let ext = self.external();
        ext.lock().unwrap_or_else(|e| e.into_inner()).add_mock_port(
            name,
            PortDirection::try_from(direction as i32)
                .unwrap_or(PortDirection::Any)
                .into(),
            PortDataType::try_from(data_type as i32)
                .unwrap_or(PortDataType::Any)
                .into(),
        );
    }
    pub fn dummy_remove_external_mock_port(&self, name: &str) {
        let ext = self.external();
        ext.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove_mock_port(name);
    }
    pub fn dummy_remove_all_external_mock_ports(&self) {
        let ext = self.external();
        ext.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove_all_mock_ports();
    }
    pub fn find_external_ports(
        &self,
        pat: Option<&str>,
        direction: u32,
        data_type: u32,
    ) -> Vec<ExternalPortDescriptor> {
        let dir = PortDirection::try_from(direction as i32).unwrap_or(PortDirection::Any);
        let dt = PortDataType::try_from(data_type as i32).unwrap_or(PortDataType::Any);
        if let Some(j) = self.jack() {
            let j = j.lock().unwrap_or_else(|e| e.into_inner());
            let (ty, flags) = jack_flags(dir, dt);
            return j
                .client()
                .ports(pat, ty, flags)
                .into_iter()
                .map(|name| ExternalPortDescriptor {
                    name,
                    direction: dir,
                    data_type: dt,
                })
                .collect();
        }
        let ext = self.external();
        let ports = ext
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .find_external_ports(pat, dir.into(), dt.into())
            .unwrap_or_default();
        ports
            .into_iter()
            .map(|d| ExternalPortDescriptor {
                name: d.name,
                direction: d.direction.into(),
                data_type: match d.data_type {
                    engine::PortDataType::Audio => PortDataType::Audio,
                    engine::PortDataType::Midi => PortDataType::Midi,
                    engine::PortDataType::Any => PortDataType::Any,
                },
            })
            .collect()
    }
}

impl Drop for AudioDriver {
    fn drop(&mut self) {
        let thread = {
            let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            i.finish.store(true, Ordering::Relaxed);
            i.dummy.close();
            i.dummy_thread.take()
        };
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub struct CompositeInstallAck {
    sequence: CommandSequence,
    outcome: Arc<Mutex<Option<std::result::Result<u64, String>>>>,
}

impl CompositeInstallAck {
    pub fn sequence(&self) -> CommandSequence {
        self.sequence
    }

    pub fn take_result(&self) -> Option<std::result::Result<u64, String>> {
        self.outcome
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeLoopState {
    pub identity: engine::LoopIdentity,
    pub active_plan_version: u64,
    pub pending_plan_version: Option<u64>,
    pub mode: LoopMode,
    pub maybe_next_mode: Option<LoopMode>,
    pub maybe_next_mode_delay: Option<u32>,
    pub iteration: u32,
    pub cycle_count: u64,
    pub length: u64,
    pub position: u64,
    pub play_after_record: bool,
    pub active_children: Vec<engine::ActiveCompositeChild>,
    pub runtime_counters: engine::CompositeRuntimeCounters,
    pub runtime_fault: engine::CompositeRuntimeFault,
}

impl CompositeLoopState {
    fn from_mirror(snapshot: &engine::CompositeStateMirrorSnapshot) -> Self {
        Self {
            identity: snapshot.identity,
            active_plan_version: snapshot.active_plan_version,
            pending_plan_version: snapshot.pending_plan_version,
            mode: snapshot.mode.into(),
            maybe_next_mode: snapshot.next_mode.map(Into::into),
            maybe_next_mode_delay: snapshot.next_mode_delay,
            iteration: snapshot.iteration,
            cycle_count: snapshot.cycle_count,
            length: snapshot.length,
            position: snapshot.position,
            play_after_record: snapshot.play_after_record,
            active_children: snapshot.active_children().collect(),
            runtime_counters: snapshot.runtime_counters,
            runtime_fault: snapshot.runtime_fault,
        }
    }
}

#[derive(Clone)]
pub struct CompositeLoop {
    shared: Arc<SharedSession>,
    control: Arc<ObjectControl<CompositeId, engine::CompositeStateMirror>>,
    desired_play_after_record: Arc<AtomicBool>,
}

impl CompositeLoop {
    /// Composite identities are reserved before their creation command is queued, so callers
    /// can safely use them without waiting. Command sequencing guarantees that any later
    /// configuration still executes after creation.
    pub fn identity_if_ready(&self) -> Option<engine::LoopIdentity> {
        matches!(
            self.lifecycle(),
            ObjectLifecycle::Pending | ObjectLifecycle::Ready
        )
        .then(|| self.control.mirror.identity())
    }

    pub fn identity(&self) -> engine::LoopIdentity {
        self.control.mirror.identity()
    }

    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    pub fn creation_error(&self) -> Option<String> {
        self.control.error()
    }

    fn ensure_usable(&self) -> Result<()> {
        match self.lifecycle() {
            // Creation, prepared timeline installation, and controls share the sequenced engine
            // queue. Frontends may therefore configure and control a newly reserved composite
            // without first fencing its lightweight creation command.
            ObjectLifecycle::Pending | ObjectLifecycle::Ready => Ok(()),
            ObjectLifecycle::Failed => Err(anyhow!(
                "composite loop creation failed: {}",
                self.creation_error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("composite loop is closed")),
        }
    }

    pub fn transition(&self, mode: LoopMode, delay: u32) -> Result<CommandSequence> {
        self.ensure_usable()?;
        if mode == LoopMode::Unknown {
            return Err(anyhow!("unknown is not a valid composite mode"));
        }
        let source = self.identity();
        let weak = Arc::downgrade(&self.control);
        self.shared
            .send_control(move |session| {
                if weak.upgrade().is_some() {
                    let _ = session.accept_composite_transition(source, mode.into(), delay);
                }
            })
            .map_err(Into::into)
    }

    pub fn transition_immediate(&self, mode: LoopMode, iteration: i64) -> Result<CommandSequence> {
        self.ensure_usable()?;
        if mode == LoopMode::Unknown {
            return Err(anyhow!("unknown is not a valid composite mode"));
        }
        let source = self.identity();
        let weak = Arc::downgrade(&self.control);
        self.shared
            .send_control(move |session| {
                if weak.upgrade().is_some() {
                    let _ = session.accept_composite_immediate_transition(
                        source,
                        mode.into(),
                        iteration,
                    );
                }
            })
            .map_err(Into::into)
    }

    pub fn set_play_after_record(&self, enabled: bool) -> Result<CommandSequence> {
        // The frontend can publish this option while creation or timeline installation is still
        // pending. Keep the desired value in the mirror so a later installation command observes
        // it even when the earlier engine-side setter has no timeline to update yet.
        self.ensure_usable()?;
        self.desired_play_after_record
            .store(enabled, Ordering::Release);
        self.control.mirror.set_play_after_record(enabled);
        let source = self.identity();
        let weak = Arc::downgrade(&self.control);
        self.shared
            .send_control(move |session| {
                if weak.upgrade().is_some() {
                    let _ = session.accept_composite_play_after_record(source, enabled);
                }
            })
            .map_err(Into::into)
    }

    pub fn poll_state(&self) -> Option<CompositeLoopState> {
        if self.lifecycle() != ObjectLifecycle::Ready {
            return None;
        }
        let snapshot = self.control.mirror.read();
        snapshot
            .installed
            .then(|| CompositeLoopState::from_mirror(&snapshot))
    }

    pub fn get_state(&self) -> Result<CompositeLoopState> {
        match self.lifecycle() {
            ObjectLifecycle::Ready => {
                let snapshot = self.control.mirror.read();
                snapshot
                    .installed
                    .then(|| CompositeLoopState::from_mirror(&snapshot))
                    .ok_or_else(|| anyhow!("composite loop is not installed"))
            }
            ObjectLifecycle::Pending => Err(anyhow!("composite loop is pending creation")),
            ObjectLifecycle::Failed => Err(anyhow!(
                "composite loop creation failed: {}",
                self.creation_error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("composite loop is closed")),
        }
    }
}

#[derive(Clone)]
pub struct Loop {
    shared: Arc<SharedSession>,
    control: Arc<ObjectControl<LoopId, engine::LoopStateMirror>>,
}
pub type LoopState = engine::LoopState;
impl Loop {
    pub fn session_id(&self) -> u64 {
        self.control.session_id
    }

    pub fn identity_if_ready(&self) -> Option<engine::LoopIdentity> {
        let index = self.control.ready_id().map(ObjectIdentity::index)?;
        Some(engine::LoopIdentity {
            slot: index as u32,
            generation: 1,
            kind: engine::LoopTargetKind::Basic,
        })
    }

    pub fn identity(&self) -> engine::LoopIdentity {
        if self.lifecycle() == ObjectLifecycle::Pending {
            let _ = self
                .shared
                .wait_for_command(self.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT);
        }
        let index = self
            .control
            .ready_id()
            .map(ObjectIdentity::index)
            .expect("loop identity requested before creation completed");
        engine::LoopIdentity {
            slot: index as u32,
            generation: 1,
            kind: engine::LoopTargetKind::Basic,
        }
    }

    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    pub fn creation_error(&self) -> Option<String> {
        self.control.error()
    }

    #[tracing::instrument(
        name = "engine.control.create_audio_channel",
        skip_all,
        fields(session_id = self.session_id(), mode = mode as u32)
    )]
    pub fn add_audio_channel(&self, mode: ChannelMode) -> Result<AudioChannel> {
        let (snapshot_writer, snapshot_control, snapshot_reader) =
            self.shared.snapshots.create_audio_channel(1024, 256);
        let mut snapshot_writer = Some(snapshot_writer);
        let control = Arc::new(ObjectControl::<
            AudioChannelId,
            engine::AudioChannelStateMirror,
        >::pending(self.shared.session_id));
        let control_for_command = Arc::downgrade(&control);
        let parent = Arc::clone(&self.control);
        let sequence = self.shared.send_topology(move |s: &mut engine::Session| {
            let Some(control) = control_for_command.upgrade() else {
                return;
            };
            let Some(loop_idx) = parent.ready_id().map(ObjectIdentity::index) else {
                control.mark_failed("parent loop was not created");
                return;
            };
            match s.add_audio_channel_with_state_and_snapshots(
                loop_idx,
                64,
                mode.into(),
                Arc::clone(&control.mirror),
                snapshot_writer.take(),
            ) {
                Ok(idx) => {
                    if let Some(mapping) = s.channel_mapping(idx) {
                        control.set_auxiliary_index(mapping.channel_idx);
                    }
                    control.mark_ready(AudioChannelId(idx));
                }
                Err(error) => control.mark_failed(error.to_string()),
            }
        })?;
        control.set_creation_sequence(sequence);
        self.shared
            .audio_snapshot_controls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((
                Arc::downgrade(&self.control),
                Arc::downgrade(&control),
                snapshot_control.clone(),
            ));
        Ok(AudioChannel {
            shared: self.shared.clone(),
            parent: Arc::clone(&self.control),
            control,
            snapshots: snapshot_reader,
            snapshot_control,
            desired_data: Arc::new(ArcSwap::from_pointee(Vec::new())),
        })
    }

    #[tracing::instrument(
        name = "engine.control.create_midi_channel",
        skip_all,
        fields(session_id = self.session_id(), mode = mode as u32)
    )]
    pub fn add_midi_channel(&self, mode: ChannelMode) -> Result<MidiChannel> {
        let (snapshot_writer, snapshot_control, snapshot_reader) =
            self.shared.snapshots.create_midi_channel(64, 64);
        let mut snapshot_writer = Some(snapshot_writer);
        let control = Arc::new(
            ObjectControl::<MidiChannelId, engine::MidiChannelStateMirror>::pending(
                self.shared.session_id,
            ),
        );
        let control_for_command = Arc::downgrade(&control);
        let parent = Arc::clone(&self.control);
        let sequence = self.shared.send_topology(move |s: &mut engine::Session| {
            let Some(control) = control_for_command.upgrade() else {
                return;
            };
            let Some(loop_idx) = parent.ready_id().map(ObjectIdentity::index) else {
                control.mark_failed("parent loop was not created");
                return;
            };
            match s.add_midi_channel_with_state_and_snapshots(
                loop_idx,
                1024,
                mode.into(),
                Arc::clone(&control.mirror),
                snapshot_writer.take(),
            ) {
                Ok(idx) => control.mark_ready(MidiChannelId(idx)),
                Err(error) => control.mark_failed(error.to_string()),
            }
        })?;
        control.set_creation_sequence(sequence);
        Ok(MidiChannel {
            shared: self.shared.clone(),
            parent: Arc::clone(&self.control),
            control,
            snapshots: snapshot_reader,
            snapshot_control,
            desired_data: Arc::new(ArcSwap::from_pointee(Vec::new())),
        })
    }

    pub fn transition(
        &self,
        to_mode: LoopMode,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) -> Result<CommandSequence> {
        let control = Arc::clone(&self.control);
        let immediate = maybe_cycles_delay < 0 && maybe_to_sync_at_cycle < 0;
        let sequence = self.shared.send_control(move |s: &mut engine::Session| {
            let Some(idx) = control.ready_id().map(ObjectIdentity::index) else {
                return;
            };
            if !immediate {
                if let Some(l) = s.loop_mut(idx) {
                    l.plan_transition(
                        to_mode.into(),
                        (maybe_cycles_delay >= 0).then_some(maybe_cycles_delay as u32),
                        (maybe_to_sync_at_cycle >= 0).then_some(maybe_to_sync_at_cycle as u32),
                    );
                }
            } else {
                let _ = s.set_loop_mode(idx, to_mode.into());
            }
        })?;
        if immediate {
            self.control.mirror.set_mode(to_mode);
        }
        Ok(sequence)
    }

    pub fn poll_state(&self) -> Option<LoopState> {
        (self.lifecycle() == ObjectLifecycle::Ready).then(|| self.control.mirror.read())
    }

    pub fn get_state(&self) -> Result<LoopState> {
        match self.lifecycle() {
            ObjectLifecycle::Ready => Ok(self.control.mirror.read()),
            ObjectLifecycle::Pending => Err(anyhow!("loop is pending creation")),
            ObjectLifecycle::Failed => Err(anyhow!(
                "loop creation failed: {}",
                self.creation_error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("loop is closed")),
        }
    }

    pub fn set_length(&self, length: u32) -> Result<CommandSequence> {
        let control = Arc::clone(&self.control);
        let sequence = self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(idx) = control.ready_id().map(ObjectIdentity::index) {
                if let Some(l) = s.loop_mut(idx) {
                    l.set_length(length);
                }
            }
        })?;
        self.control.mirror.set_length(length);
        Ok(sequence)
    }

    pub fn set_position(&self, position: u32) -> Result<CommandSequence> {
        let control = Arc::clone(&self.control);
        let sequence = self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(idx) = control.ready_id().map(ObjectIdentity::index) {
                if let Some(l) = s.loop_mut(idx) {
                    l.set_position(position);
                }
            }
        })?;
        self.control.mirror.set_position(position);
        Ok(sequence)
    }

    pub fn clear(&self, length: u32) -> Result<CommandSequence> {
        let control = Arc::clone(&self.control);
        Ok(self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(idx) = control.ready_id().map(ObjectIdentity::index) {
                if let Some(l) = s.loop_mut(idx) {
                    l.clear(length);
                    l.clear_planned_transitions();
                    l.set_mode(engine::LoopMode::Stopped);
                    l.set_position(0);
                }
            }
        })?)
    }

    pub fn set_sync_source(&self, src: Option<&Loop>) -> Result<CommandSequence> {
        if src.is_some_and(|source| source.control.session_id != self.control.session_id) {
            return Err(anyhow!("cannot sync loops from different backend sessions"));
        }
        let source = src
            .filter(|source| !Arc::ptr_eq(&source.control, &self.control))
            .map(|source| Arc::clone(&source.control));
        let applied_source_index = {
            let mut controls = self
                .shared
                .primitive_loop_controls
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let entry = controls
                .iter_mut()
                .find(|entry| {
                    entry
                        .control
                        .upgrade()
                        .is_some_and(|control| Arc::ptr_eq(&control, &self.control))
                })
                .ok_or_else(|| anyhow!("primitive target control is no longer registered"))?;
            entry.desired_source = source.as_ref().map(Arc::downgrade);
            Arc::clone(&entry.applied_source_index)
        };
        let control = Arc::clone(&self.control);
        let source_for_command = source.clone();
        Ok(self.shared.send_topology(move |s: &mut engine::Session| {
            let Some(idx) = control.ready_id().map(ObjectIdentity::index) else {
                return;
            };
            let source_idx = source_for_command
                .as_ref()
                .and_then(|source| source.ready_id())
                .map(ObjectIdentity::index);
            if s.set_loop_sync_source(idx, source_idx).is_ok() {
                applied_source_index.store(
                    source_idx.unwrap_or(INVALID_OBJECT_INDEX),
                    Ordering::Release,
                );
            }
        })?)
    }

    pub fn adopt_ringbuffer_contents(
        &self,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<()> {
        if self.lifecycle() == ObjectLifecycle::Pending {
            self.shared
                .wait_for_command(self.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT)?;
        }
        let loop_idx = self
            .control
            .ready_id()
            .map(ObjectIdentity::index)
            .ok_or_else(|| anyhow!("loop is not ready"))?;
        let request = engine::AudioRingbufferAdoption {
            loop_idx,
            reverse_start_cycle,
            cycles_length,
            go_to_cycle,
            go_to_mode: go_to_mode.into(),
        };
        BackendSession {
            shared: self.shared.clone(),
        }
        .adopt_audio_ringbuffers(vec![request])
    }
}

#[tracing::instrument(
    name = "engine.control.transition_loops",
    skip_all,
    fields(loops = loops.len(), mode = to_state as u32)
)]
pub fn transition_multiple_loops(
    loops: &[&Loop],
    to_state: LoopMode,
    maybe_cycles_delay: i32,
    maybe_to_sync_at_cycle: i32,
) -> Result<()> {
    if let Some(first) = loops.first() {
        if loops
            .iter()
            .any(|loop_| loop_.control.session_id != first.control.session_id)
        {
            return Err(anyhow!(
                "cannot transition loops from different backend sessions"
            ));
        }
    }
    for loop_ in loops {
        loop_.transition(to_state, maybe_cycles_delay, maybe_to_sync_at_cycle)?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct AudioChannel {
    shared: Arc<SharedSession>,
    parent: Arc<ObjectControl<LoopId, engine::LoopStateMirror>>,
    control: Arc<ObjectControl<AudioChannelId, engine::AudioChannelStateMirror>>,
    snapshots: engine::content_snapshot::AudioSnapshotReader,
    snapshot_control: engine::content_snapshot::AudioSnapshotControl,
    desired_data: Arc<ArcSwap<Vec<f32>>>,
}
pub type AudioChannelState = engine::AudioChannelState;
impl AudioChannel {
    pub fn session_id(&self) -> u64 {
        self.control.session_id
    }

    pub fn capture_content_epoch(&self) -> Option<u64> {
        self.shared.snapshots.capture_epoch()
    }

    pub fn validate_content_epoch(&self, captured: u64) -> bool {
        self.shared.snapshots.validate_epoch(captured)
    }

    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    fn with_mut(
        &self,
        mut f: impl FnMut(&mut engine::AudioChannel) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(idx) = control.ready_id().map(ObjectIdentity::index) {
                if let Some(channel) = s.audio_channel_mut(idx) {
                    f(channel);
                }
            }
        })
    }

    pub fn connect_input(
        &self,
        port: &AudioPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.connect_channel_input(channel.index(), port.index());
            }
        })
    }

    pub fn connect_output(
        &self,
        port: &AudioPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.connect_channel_output(channel.index(), port.index());
            }
        })
    }

    pub fn disconnect(&self, port: &AudioPort) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.disconnect_channel_port(channel.index(), port.index());
            }
        })
    }

    pub fn load_data(&self, data: &[f32]) -> std::result::Result<CommandSequence, SendError> {
        let owned = data.to_vec();
        let snapshot = self
            .snapshot_control
            .prepare(&owned, engine::content_snapshot::ContentMutation::Loading)
            .ok_or(SendError::Full)?;
        let mut prepared = engine::PreparedAudioChannelData::new(64, owned.len());
        prepared.begin_load(owned.len());
        prepared.write(0, &owned);
        let mut prepared = Some(prepared);
        let result = self.with_mut(move |channel| {
            if let Some(mut prepared) = prepared.take() {
                channel.commit_prepared_data_and_snapshot(&mut prepared, snapshot);
            }
        });
        if result.is_ok() {
            self.desired_data.store(Arc::new(owned));
        } else {
            self.snapshot_control.cancel();
        }
        result
    }

    pub fn get_data(&self) -> Vec<f32> {
        let latest = self.snapshots.latest();
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::MutationActive(
                    engine::content_snapshot::ContentMutation::Loading
                )
            )
        ) {
            return self.desired_data.load_full().as_ref().clone();
        }
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::MutationActive(_)
            )
        ) {
            // Legacy synchronous callers historically observed process writes immediately.
            // Give the off-thread publisher one poll while preserving stale-read semantics.
            std::thread::sleep(Duration::from_millis(2));
            return self.snapshots.latest().snapshot.contiguous();
        }
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::PublicationPending { .. }
            )
        ) {
            let deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < deadline {
                if let Ok(snapshot) = self.snapshots.try_current() {
                    return snapshot.contiguous();
                }
                std::thread::sleep(Duration::from_micros(100));
            }
        }
        latest.snapshot.contiguous()
    }

    pub fn get_latest_data_snapshot(
        &self,
    ) -> engine::content_snapshot::SnapshotRead<engine::content_snapshot::AudioContentSnapshot>
    {
        self.snapshots.latest()
    }

    pub fn try_get_current_data_snapshot(
        &self,
    ) -> std::result::Result<
        Arc<engine::content_snapshot::AudioContentSnapshot>,
        engine::content_snapshot::CurrentDataError,
    > {
        self.snapshots.try_current()
    }

    pub fn acknowledge_data_revision(&self, revision: engine::content_snapshot::ContentRevision) {
        self.snapshots.acknowledge(revision);
    }

    pub fn poll_state(&self) -> Option<AudioChannelState> {
        (self.lifecycle() == ObjectLifecycle::Ready).then(|| {
            let mut state = self
                .control
                .mirror
                .read(self.control.acknowledged_data_sequence());
            state.data_dirty = self.snapshots.is_dirty();
            state
        })
    }

    pub fn get_state(&self) -> Result<AudioChannelState> {
        match self.lifecycle() {
            ObjectLifecycle::Ready => {
                let mut state = self
                    .control
                    .mirror
                    .read(self.control.acknowledged_data_sequence());
                state.data_dirty = self.snapshots.is_dirty();
                Ok(state)
            }
            ObjectLifecycle::Pending => Err(anyhow!("audio channel is pending creation")),
            ObjectLifecycle::Failed => Err(anyhow!(
                "audio channel creation failed: {}",
                self.control
                    .error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("audio channel is closed")),
        }
    }

    pub fn set_gain(&self, gain: f32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_gain(gain));
        if result.is_ok() {
            self.control.mirror.set_gain(gain);
        }
        result
    }

    pub fn set_mode(&self, mode: ChannelMode) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_mode(mode.into()));
        if result.is_ok() {
            self.control.mirror.set_mode(mode);
        }
        result
    }

    pub fn set_start_offset(&self, offset: i32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_start_offset(offset));
        if result.is_ok() {
            self.control.mirror.set_start_offset(offset);
        }
        result
    }

    pub fn set_n_preplay_samples(&self, n: u32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_pre_play_samples(n));
        if result.is_ok() {
            self.control.mirror.set_n_preplay_samples(n);
        }
        result
    }

    pub fn clear_data_dirty(&self) {
        let revision = self.snapshots.latest().snapshot.revision;
        self.snapshots.acknowledge(revision);
    }

    pub fn clear(&self, length: u32) -> std::result::Result<CommandSequence, SendError> {
        if !self
            .snapshot_control
            .begin_mutation(engine::content_snapshot::ContentMutation::Clearing)
        {
            return Err(SendError::Full);
        }
        let result = self.with_mut(move |channel| channel.clear(length as usize));
        if result.is_err() {
            self.snapshot_control.cancel_mutation();
        }
        result
    }
}

fn midi_grab_window(
    reverse_start_cycle: Option<i32>,
    cycles_length: Option<i32>,
    go_to_cycle: Option<i32>,
    go_to_mode: LoopMode,
    cycle_len: u32,
    sync_pos: u32,
    data_len: usize,
) -> (usize, usize, usize) {
    let cycles = cycles_length.unwrap_or(1).max(1) as u32;
    let go_cycle = go_to_cycle.unwrap_or(0).max(0) as u32;
    let wanted = if cycle_len > 0 {
        if reverse_start_cycle == Some(0) {
            sync_pos
        } else if go_to_mode == LoopMode::Recording {
            go_cycle.saturating_mul(cycle_len).saturating_add(sync_pos)
        } else {
            cycles.saturating_mul(cycle_len)
        }
    } else {
        data_len.min(u32::MAX as usize) as u32
    } as usize;
    let end = if cycle_len > 0 {
        if let Some(reverse) = reverse_start_cycle {
            if reverse == 0 {
                data_len
            } else {
                let before = (reverse.max(0) as u32).saturating_sub(cycles);
                data_len.saturating_sub(
                    sync_pos.saturating_add(before.saturating_mul(cycle_len)) as usize
                )
            }
        } else if go_to_mode == LoopMode::Recording {
            data_len
        } else {
            data_len.saturating_sub(
                sync_pos.saturating_add(go_cycle.saturating_mul(cycle_len)) as usize,
            )
        }
    } else {
        data_len
    };
    (wanted, end.saturating_sub(wanted), end)
}

#[derive(Clone)]
pub struct MidiChannel {
    shared: Arc<SharedSession>,
    parent: Arc<ObjectControl<LoopId, engine::LoopStateMirror>>,
    control: Arc<ObjectControl<MidiChannelId, engine::MidiChannelStateMirror>>,
    snapshots: engine::content_snapshot::MidiSnapshotReader,
    snapshot_control: engine::content_snapshot::MidiSnapshotControl,
    desired_data: Arc<ArcSwap<Vec<MidiEvent>>>,
}
pub type MidiChannelState = engine::MidiChannelState;
impl MidiChannel {
    pub fn session_id(&self) -> u64 {
        self.control.session_id
    }

    pub fn capture_content_epoch(&self) -> Option<u64> {
        self.shared.snapshots.capture_epoch()
    }

    pub fn validate_content_epoch(&self, captured: u64) -> bool {
        self.shared.snapshots.validate_epoch(captured)
    }

    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    fn with_mut(
        &self,
        mut f: impl FnMut(&mut engine::MidiChannel) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(idx) = control.ready_id().map(ObjectIdentity::index) {
                if let Some(channel) = s.midi_channel_mut(idx) {
                    f(channel);
                }
            }
        })
    }

    pub fn get_all_midi_data(&self) -> Vec<MidiEvent> {
        let latest = self.snapshots.latest();
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::MutationActive(
                    engine::content_snapshot::ContentMutation::Loading
                )
            )
        ) {
            return self.desired_data.load_full().as_ref().clone();
        }
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::MutationActive(_)
            )
        ) {
            // Legacy synchronous callers historically observed process writes immediately.
            // Give the off-thread publisher one poll while preserving stale-read semantics.
            std::thread::sleep(Duration::from_millis(2));
            return self.snapshots.latest().snapshot.contiguous();
        }
        if matches!(
            latest.currentness,
            engine::content_snapshot::SnapshotCurrentness::Stale(
                engine::content_snapshot::StaleReason::PublicationPending { .. }
            )
        ) {
            let deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < deadline {
                if let Ok(snapshot) = self.snapshots.try_current() {
                    return snapshot.contiguous();
                }
                std::thread::sleep(Duration::from_micros(100));
            }
        }
        latest.snapshot.contiguous()
    }

    pub fn get_latest_data_snapshot(
        &self,
    ) -> engine::content_snapshot::SnapshotRead<engine::content_snapshot::MidiContentSnapshot> {
        self.snapshots.latest()
    }

    pub fn try_get_current_data_snapshot(
        &self,
    ) -> std::result::Result<
        Arc<engine::content_snapshot::MidiContentSnapshot>,
        engine::content_snapshot::CurrentDataError,
    > {
        self.snapshots.try_current()
    }

    pub fn acknowledge_data_revision(&self, revision: engine::content_snapshot::ContentRevision) {
        self.snapshots.acknowledge(revision);
    }

    pub fn load_all_midi_data(
        &self,
        msgs: &[MidiEvent],
    ) -> std::result::Result<CommandSequence, SendError> {
        let length = msgs
            .iter()
            .filter(|message| message.time >= 0)
            .map(|message| message.time as u32)
            .max()
            .unwrap_or(0);
        self.load_midi_data(msgs, length)
    }

    pub fn load_midi_data(
        &self,
        msgs: &[MidiEvent],
        length: u32,
    ) -> std::result::Result<CommandSequence, SendError> {
        let state: Vec<Vec<u8>> = msgs
            .iter()
            .filter(|message| message.time < 0)
            .map(|message| message.data.clone())
            .collect();
        let elements: Vec<_> = msgs
            .iter()
            .filter(|message| message.time >= 0)
            .filter_map(|message| {
                engine::midi_storage::MidiStorageElem::new(message.time as u32, &message.data)
            })
            .collect();
        let mut state_tracker = engine::MidiStateTracker::new(engine::TrackWhat::ALL);
        for message in &state {
            state_tracker.process(message);
        }
        let mut snapshot_events: Vec<MidiEvent> = if state.is_empty() {
            Vec::new()
        } else {
            state_tracker
                .state_as_messages()
                .into_iter()
                .map(|data| MidiEvent { time: -1, data })
                .collect()
        };
        snapshot_events.extend(elements.iter().map(|event| MidiEvent {
            time: event.time as i32,
            data: event.data().to_vec(),
        }));
        let snapshot = self
            .snapshot_control
            .prepare(
                &snapshot_events,
                length,
                engine::content_snapshot::ContentMutation::Loading,
            )
            .ok_or(SendError::Full)?;
        let prepared = engine::PreparedMidiChannelData::new(
            &elements,
            length,
            (!state.is_empty()).then_some(state.as_slice()),
        );
        let mut prepared = Some(prepared);
        let result = self.with_mut(move |channel| {
            if let Some(mut prepared) = prepared.take() {
                channel.commit_prepared_data_and_snapshot(&mut prepared, snapshot);
            }
        });
        if result.is_ok() {
            self.desired_data.store(Arc::new(msgs.to_vec()));
        } else {
            self.snapshot_control.cancel();
        }
        result
    }

    pub fn adopt_ringbuffer_contents(
        &self,
        port: &MidiPort,
        loop_: &Loop,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id
            || self.control.session_id != loop_.control.session_id
        {
            return Err(SendError::Disconnected);
        }
        let channel_control = Arc::clone(&self.control);
        let port_control = Arc::clone(&port.control);
        let loop_control = Arc::clone(&loop_.control);
        self.shared
            .send_control(move |session: &mut engine::Session| {
                let (Some(channel_id), Some(port_id), Some(loop_id)) = (
                    channel_control.ready_id(),
                    port_control.ready_id(),
                    loop_control.ready_id(),
                ) else {
                    return;
                };
                let Some(port) = session
                    .port(port_id.index())
                    .and_then(engine::session::Port::midi)
                else {
                    return;
                };
                let mut captured = engine::MidiStorage::with_capacity_elems(1024);
                port.snapshot_ringbuffer_into(&mut captured);
                let data_len = port.ringbuffer_n_samples() as usize;
                let Some(loop_state) = session.loop_(loop_id.index()) else {
                    return;
                };
                let sync = loop_state.sync_source();
                let cycle_len = sync.map(|state| state.length).unwrap_or(0);
                let sync_pos = sync.map(|state| state.position).unwrap_or(0);
                let (wanted, start, end) = midi_grab_window(
                    reverse_start_cycle,
                    cycles_length,
                    go_to_cycle,
                    go_to_mode,
                    cycle_len,
                    sync_pos,
                    data_len,
                );
                let messages = captured
                    .iter()
                    .filter(|message| {
                        let time = message.time as usize;
                        time >= start && time < end
                    })
                    .map(|message| message.at_time(message.time.saturating_sub(start as u32)))
                    .collect::<Vec<_>>();
                if let Some(channel) = session.midi_channel_mut(channel_id.index()) {
                    channel.set_contents(&messages, wanted as u32, None);
                }
            })
    }

    pub fn connect_input(
        &self,
        port: &MidiPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.connect_channel_input(channel.index(), port.index());
            }
        })
    }

    pub fn connect_output(
        &self,
        port: &MidiPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.connect_channel_output(channel.index(), port.index());
            }
        })
    }

    pub fn disconnect(&self, port: &MidiPort) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != port.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (control, port) = (Arc::clone(&self.control), Arc::clone(&port.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(channel), Some(port)) = (control.ready_id(), port.ready_id()) {
                let _ = s.disconnect_channel_port(channel.index(), port.index());
            }
        })
    }

    pub fn poll_state(&self) -> Option<MidiChannelState> {
        (self.lifecycle() == ObjectLifecycle::Ready).then(|| {
            let mut state = self
                .control
                .mirror
                .read(self.control.acknowledged_data_sequence());
            state.data_dirty = self.snapshots.is_dirty();
            state
        })
    }

    pub fn get_state(&self) -> Result<MidiChannelState> {
        match self.lifecycle() {
            ObjectLifecycle::Ready => {
                let mut state = self
                    .control
                    .mirror
                    .read(self.control.acknowledged_data_sequence());
                state.data_dirty = self.snapshots.is_dirty();
                Ok(state)
            }
            ObjectLifecycle::Pending => Err(anyhow!("MIDI channel is pending creation")),
            ObjectLifecycle::Failed => Err(anyhow!(
                "MIDI channel creation failed: {}",
                self.control
                    .error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("MIDI channel is closed")),
        }
    }

    pub fn set_mode(&self, mode: ChannelMode) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_mode(mode.into()));
        if result.is_ok() {
            self.control.mirror.set_mode(mode);
        }
        result
    }

    pub fn set_start_offset(&self, offset: i32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_start_offset(offset));
        if result.is_ok() {
            self.control.mirror.set_start_offset(offset);
        }
        result
    }

    pub fn set_n_preplay_samples(&self, n: u32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_mut(move |channel| channel.set_pre_play_samples(n));
        if result.is_ok() {
            self.control.mirror.set_n_preplay_samples(n);
        }
        result
    }

    pub fn clear_data_dirty(&self) {
        let revision = self.snapshots.latest().snapshot.revision;
        self.snapshots.acknowledge(revision);
    }

    pub fn clear(&self) -> std::result::Result<CommandSequence, SendError> {
        if !self
            .snapshot_control
            .begin_mutation(engine::content_snapshot::ContentMutation::Clearing)
        {
            return Err(SendError::Full);
        }
        let result = self.with_mut(move |channel| channel.clear());
        if result.is_err() {
            self.snapshot_control.cancel_mutation();
        }
        result
    }

    pub fn reset_state_tracking(&self) -> std::result::Result<CommandSequence, SendError> {
        self.with_mut(move |channel| channel.reset_state_tracking())
    }
}

pub struct LoopAudioContentUpdate<'a> {
    pub channel: &'a AudioChannel,
    pub samples: &'a [f32],
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
}

pub struct LoopMidiContentUpdate<'a> {
    pub channel: &'a MidiChannel,
    pub messages: &'a [MidiEvent],
    pub length: u32,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
}

#[tracing::instrument(
    name = "engine.control.replace_loop_content",
    skip_all,
    fields(
        session_id = loop_.session_id(),
        audio_channels = audio.len(),
        midi_channels = midi.len()
    )
)]
pub fn replace_loop_content(
    loop_: &Loop,
    audio: &[LoopAudioContentUpdate<'_>],
    midi: &[LoopMidiContentUpdate<'_>],
    length: Option<u32>,
) -> Result<CommandSequence> {
    if audio.is_empty() && midi.is_empty() {
        return Err(anyhow!("loop content update is empty"));
    }
    if matches!(
        loop_.get_state()?.mode,
        LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
    ) {
        return Err(anyhow!("loop content is changing"));
    }
    if audio.iter().any(|item| {
        item.channel.session_id() != loop_.session_id()
            || !Arc::ptr_eq(&item.channel.parent, &loop_.control)
    }) || midi.iter().any(|item| {
        item.channel.session_id() != loop_.session_id()
            || !Arc::ptr_eq(&item.channel.parent, &loop_.control)
    }) {
        return Err(anyhow!("cannot update channels from another loop"));
    }
    let audio_ids = audio
        .iter()
        .map(|item| {
            item.channel
                .control
                .ready_id()
                .map(ObjectIdentity::index)
                .ok_or_else(|| anyhow!("audio channel is not ready"))
        })
        .collect::<Result<Vec<_>>>()?;
    let midi_ids = midi
        .iter()
        .map(|item| {
            item.channel
                .control
                .ready_id()
                .map(ObjectIdentity::index)
                .ok_or_else(|| anyhow!("MIDI channel is not ready"))
        })
        .collect::<Result<Vec<_>>>()?;
    if audio_ids.iter().collect::<BTreeSet<_>>().len() != audio_ids.len()
        || midi_ids.iter().collect::<BTreeSet<_>>().len() != midi_ids.len()
    {
        return Err(anyhow!("loop content update contains a duplicate channel"));
    }

    let mut prepared_audio = Vec::with_capacity(audio.len());
    let mut prepared_midi = Vec::with_capacity(midi.len());
    let mut audio_cancellations: Vec<engine::content_snapshot::AudioSnapshotControl> =
        Vec::with_capacity(audio.len());
    let mut midi_cancellations: Vec<engine::content_snapshot::MidiSnapshotControl> =
        Vec::with_capacity(midi.len());
    for (item, channel_id) in audio.iter().zip(audio_ids) {
        let owned = item.samples.to_vec();
        let Some(snapshot) = item
            .channel
            .snapshot_control
            .prepare(&owned, engine::content_snapshot::ContentMutation::Loading)
        else {
            for control in audio_cancellations {
                control.cancel();
            }
            return Err(anyhow!("audio snapshot preparation is busy"));
        };
        audio_cancellations.push(item.channel.snapshot_control.clone());
        let mut prepared = engine::PreparedAudioChannelData::new(64, owned.len());
        prepared.begin_load(owned.len());
        prepared.write(0, &owned);
        prepared_audio.push((
            channel_id,
            prepared,
            snapshot,
            item.start_offset,
            item.preplay,
        ));
    }
    for (item, channel_id) in midi.iter().zip(midi_ids) {
        let state = item
            .messages
            .iter()
            .filter(|message| message.time < 0)
            .map(|message| message.data.clone())
            .collect::<Vec<_>>();
        let elements = match item
            .messages
            .iter()
            .filter(|message| message.time >= 0)
            .map(|message| {
                engine::midi_storage::MidiStorageElem::new(message.time as u32, &message.data)
                    .ok_or_else(|| anyhow!("invalid MIDI event"))
            })
            .collect::<Result<Vec<_>>>()
        {
            Ok(elements) => elements,
            Err(error) => {
                for control in audio_cancellations {
                    control.cancel();
                }
                for control in midi_cancellations {
                    control.cancel();
                }
                return Err(error);
            }
        };
        let Some(snapshot) = item.channel.snapshot_control.prepare(
            item.messages,
            item.length,
            engine::content_snapshot::ContentMutation::Loading,
        ) else {
            for control in audio_cancellations {
                control.cancel();
            }
            for control in midi_cancellations {
                control.cancel();
            }
            return Err(anyhow!("MIDI snapshot preparation is busy"));
        };
        midi_cancellations.push(item.channel.snapshot_control.clone());
        prepared_midi.push((
            channel_id,
            engine::PreparedMidiChannelData::new(
                &elements,
                item.length,
                (!state.is_empty()).then_some(state.as_slice()),
            ),
            snapshot,
            item.start_offset,
            item.preplay,
        ));
    }

    let loop_control = Arc::clone(&loop_.control);
    let mut prepared_audio = Some(prepared_audio);
    let mut prepared_midi = Some(prepared_midi);
    let result = loop_
        .shared
        .send_control(move |session: &mut engine::Session| {
            let Some(loop_id) = loop_control.ready_id().map(ObjectIdentity::index) else {
                return;
            };
            if session.loop_(loop_id).is_none()
                || prepared_audio.as_ref().is_none_or(|audio| {
                    audio
                        .iter()
                        .any(|(channel_id, ..)| session.audio_channel(*channel_id).is_none())
                })
                || prepared_midi.as_ref().is_none_or(|midi| {
                    midi.iter()
                        .any(|(channel_id, ..)| session.midi_channel(*channel_id).is_none())
                })
            {
                return;
            }
            let Some(mut audio) = prepared_audio.take() else {
                return;
            };
            let Some(mut midi) = prepared_midi.take() else {
                return;
            };
            for (channel_id, prepared, snapshot, offset, preplay) in &mut audio {
                let channel = session
                    .audio_channel_mut(*channel_id)
                    .expect("loop content channels were preflighted");
                let retained_offset = channel.start_offset();
                channel.commit_prepared_data_and_snapshot(prepared, *snapshot);
                channel.set_start_offset(offset.unwrap_or(retained_offset));
                if let Some(preplay) = preplay {
                    channel.set_pre_play_samples(*preplay);
                }
            }
            for (channel_id, prepared, snapshot, offset, preplay) in &mut midi {
                let channel = session
                    .midi_channel_mut(*channel_id)
                    .expect("loop content channels were preflighted");
                channel.commit_prepared_data_and_snapshot(prepared, *snapshot);
                if let Some(offset) = offset {
                    channel.set_start_offset(*offset);
                }
                if let Some(preplay) = preplay {
                    channel.set_pre_play_samples(*preplay);
                }
            }
            if let Some(loop_state) = session.loop_mut(loop_id) {
                loop_state.clear_planned_transitions();
                loop_state.set_mode(engine::LoopMode::Stopped);
                if let Some(length) = length {
                    loop_state.set_length(length);
                }
            }
        });
    let sequence = match result {
        Ok(sequence) => sequence,
        Err(error) => {
            for control in audio_cancellations {
                control.cancel();
            }
            for control in midi_cancellations {
                control.cancel();
            }
            return Err(error.into());
        }
    };
    loop_.control.mirror.set_mode(LoopMode::Stopped);
    if let Some(length) = length {
        loop_.control.mirror.set_length(length);
    }
    for item in audio {
        item.channel
            .desired_data
            .store(Arc::new(item.samples.to_vec()));
        if let Some(offset) = item.start_offset {
            item.channel.control.mirror.set_start_offset(offset);
        }
        if let Some(preplay) = item.preplay {
            item.channel.control.mirror.set_n_preplay_samples(preplay);
        }
    }
    for item in midi {
        item.channel
            .desired_data
            .store(Arc::new(item.messages.to_vec()));
        if let Some(offset) = item.start_offset {
            item.channel.control.mirror.set_start_offset(offset);
        }
        if let Some(preplay) = item.preplay {
            item.channel.control.mirror.set_n_preplay_samples(preplay);
        }
    }
    Ok(sequence)
}

#[derive(Clone)]
pub struct AudioPort {
    shared: Arc<SharedSession>,
    control: Arc<ObjectControl<AudioPortId, engine::AudioPortStateMirror>>,
    dummy_output: Arc<Mutex<Vec<f32>>>,
    direction: PortDirection,
    /// Kept here because the audio thread cannot publish it: a name is a `String`, so the
    /// snapshot carries only numbers and this side supplies the name it created the port with.
    name: String,
}
pub type AudioPortState = engine::AudioPortState;
impl AudioPort {
    #[tracing::instrument(
        name = "engine.control.create_audio_port",
        skip_all,
        fields(session_id = sess.session_id(), direction = *direction as u32)
    )]
    pub fn new_driver_port(
        sess: &BackendSession,
        driver: &AudioDriver,
        name: &str,
        direction: &PortDirection,
        ring: u32,
    ) -> Result<Self> {
        let control = Arc::new(
            ObjectControl::<AudioPortId, engine::AudioPortStateMirror>::pending(
                sess.shared.session_id,
            ),
        );
        let control_for_command = Arc::downgrade(&control);
        let dummy_output = Arc::new(Mutex::new(Vec::new()));
        let output_for_command = Arc::clone(&dummy_output);
        let (owned, dir) = (name.to_string(), *direction);
        let mut owned = Some(owned);
        let sequence = sess.shared.send_topology(move |s: &mut engine::Session| {
            let Some(control) = control_for_command.upgrade() else {
                return;
            };
            let Some(owned) = owned.take() else { return };
            if control.lifecycle() != ObjectLifecycle::Pending {
                return;
            }
            let mut external = engine::external_audio_port::ExternalAudioPort::new(
                owned,
                dir.into(),
                ring as usize,
            );
            external.set_output_capture(Arc::clone(&output_for_command));
            let port = engine::session::Port::External(external);
            match s.add_audio_port_with_state(port, Arc::clone(&control.mirror)) {
                Ok(idx) => control.mark_ready(AudioPortId(idx)),
                Err(error) => control.mark_failed(error.to_string()),
            }
        })?;
        control.set_creation_sequence(sequence);
        if let Err(error) = driver.register_audio_port(name, *direction, Arc::clone(&control)) {
            control.mark_failed(error.to_string());
            return Err(error);
        }
        Ok(Self {
            shared: sess.shared.clone(),
            control,
            dummy_output,
            direction: *direction,
            name: name.to_string(),
        })
    }

    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    pub fn creation_error(&self) -> Option<String> {
        self.control.error()
    }

    pub fn input_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability::EXTERNAL,
            PortDirection::Output => PortConnectability::INTERNAL,
            PortDirection::Any => PortConnectability::INTERNAL.with(PortConnectability::EXTERNAL),
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability::INTERNAL,
            PortDirection::Output => PortConnectability::EXTERNAL,
            PortDirection::Any => PortConnectability::INTERNAL.with(PortConnectability::EXTERNAL),
        }
    }
    /// Queues a mutation of this port's audio side.
    fn with_audio_mut(
        &self,
        mut f: impl FnMut(&mut engine::AudioPort) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(a) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.audio_mut())
            {
                f(a)
            }
        })
    }

    /// This port's state as of the independently published mirrors, without blocking.
    pub fn poll_state(&self) -> Option<AudioPortState> {
        (self.lifecycle() == ObjectLifecycle::Ready)
            .then(|| self.control.mirror.read(self.name.clone()))
    }

    pub fn get_state(&self) -> Result<AudioPortState> {
        match self.lifecycle() {
            ObjectLifecycle::Pending | ObjectLifecycle::Ready => {
                Ok(self.control.mirror.read(self.name.clone()))
            }
            ObjectLifecycle::Failed => Err(anyhow!(
                "audio port creation failed: {}",
                self.control
                    .error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("audio port is closed")),
        }
    }
    pub fn set_gain(&self, gain: f32) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_audio_mut(move |a| a.set_gain(gain));
        if result.is_ok() {
            self.control.mirror.set_gain(gain);
        }
        result
    }
    pub fn set_muted(&self, muted: bool) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_audio_mut(move |a| a.set_muted(muted));
        if result.is_ok() {
            self.control.mirror.set_muted(muted);
        }
        result
    }
    pub fn set_passthrough_muted(
        &self,
        muted: bool,
    ) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_audio_mut(move |a| a.set_passthrough_muted(muted));
        if result.is_ok() {
            self.control.mirror.set_passthrough_muted(muted);
        }
        result
    }
    pub fn connect_internal(
        &self,
        other: &AudioPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != other.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (from, to) = (Arc::clone(&self.control), Arc::clone(&other.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(from), Some(to)) = (from.ready_id(), to.ready_id()) {
                let _ = s.connect_ports_internal(from.index(), to.index());
            }
        })
    }
    pub fn dummy_queue_data(
        &self,
        data: &[f32],
    ) -> std::result::Result<CommandSequence, SendError> {
        let (control, owned) = (Arc::clone(&self.control), data.to_vec());
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(p) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.as_external_mut())
            {
                p.stage_input(&owned)
            }
        })
    }
    pub fn dummy_dequeue_data(&self, n: u32) -> Vec<f32> {
        let mut output = self
            .dummy_output
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let n = (n as usize).min(output.len());
        output.drain(..n).collect()
    }
    pub fn dummy_request_data(&self, _n: u32) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(p) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.as_external_mut())
            {
                p.clear_output_queue();
            }
        })
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        self.shared.connections_state(
            &self.name,
            self.direction,
            PortDataType::Audio,
            self.control.ready_id().map(ObjectIdentity::index),
        )
    }
    pub fn get_connections_state_now(&self) -> HashMap<String, bool> {
        self.shared.connections_state_now(
            &self.name,
            self.direction,
            PortDataType::Audio,
            self.control.ready_id().map(ObjectIdentity::index),
        )
    }
    pub fn connect_external_port(&self, name: &str) {
        self.shared.invalidate_connection_cache();
        if let Some(j) = self.shared.jack() {
            jack_connect_port(&j, &self.name, self.direction, name);
            self.shared.set_cached_connection(
                &self.name,
                self.direction,
                PortDataType::Audio,
                name,
                true,
            );
            return;
        }
        if let Some(ext) = self.shared.external() {
            if let Some(id) = self.control.ready_id() {
                let _ = ext
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .connect(compat_port_id(id.index()), name);
                self.shared.set_cached_connection(
                    &self.name,
                    self.direction,
                    PortDataType::Audio,
                    name,
                    true,
                );
                return;
            }
            let (control, name) = (Arc::clone(&self.control), name.to_string());
            if let Err(error) = self.shared.send_control(move |_: &mut engine::Session| {
                if let Some(id) = control.ready_id() {
                    let _ = crate::realtime_allow_lock!(
                        "deferred external audio connection",
                        ext.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner())
                    .connect(compat_port_id(id.index()), &name);
                }
            }) {
                log::error!("could not queue external audio connection: {error}");
            }
        }
    }
    pub fn disconnect_external_port(&self, name: &str) {
        self.shared.invalidate_connection_cache();
        if let Some(j) = self.shared.jack() {
            jack_disconnect_port(&j, &self.name, self.direction, name);
            self.shared.set_cached_connection(
                &self.name,
                self.direction,
                PortDataType::Audio,
                name,
                false,
            );
            return;
        }
        if let Some(ext) = self.shared.external() {
            if let Some(id) = self.control.ready_id() {
                let _ = ext
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .disconnect(compat_port_id(id.index()), name);
                self.shared.set_cached_connection(
                    &self.name,
                    self.direction,
                    PortDataType::Audio,
                    name,
                    false,
                );
                return;
            }
            let (control, name) = (Arc::clone(&self.control), name.to_string());
            if let Err(error) = self.shared.send_control(move |_: &mut engine::Session| {
                if let Some(id) = control.ready_id() {
                    let _ = crate::realtime_allow_lock!(
                        "deferred external audio disconnection",
                        ext.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner())
                    .disconnect(compat_port_id(id.index()), &name);
                }
            }) {
                log::error!("could not queue external audio disconnection: {error}");
            }
        }
    }
    pub fn set_ringbuffer_n_samples(
        &self,
        n: u32,
    ) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_audio_mut(move |a| a.set_ringbuffer_n_samples(n as usize));
        if result.is_ok() {
            self.control.mirror.set_ringbuffer_n_samples(n);
        }
        result
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
}

#[derive(Clone)]
pub struct MidiPort {
    shared: Arc<SharedSession>,
    control: Arc<ObjectControl<MidiPortId, engine::MidiPortStateMirror>>,
    dummy_output: Arc<Mutex<Vec<MidiEvent>>>,
    direction: PortDirection,
    /// As on `AudioPort`: the audio thread cannot publish a `String`, so this side keeps it.
    name: String,
}
pub type MidiPortState = engine::MidiPortState;
impl MidiPort {
    pub fn lifecycle(&self) -> ObjectLifecycle {
        observed_lifecycle(&self.shared, &self.control)
    }

    pub fn creation_sequence(&self) -> CommandSequence {
        self.control.creation_sequence()
    }

    pub fn creation_error(&self) -> Option<String> {
        self.control.error()
    }

    #[tracing::instrument(
        name = "engine.control.create_midi_port",
        skip_all,
        fields(session_id = sess.session_id(), direction = *direction as u32)
    )]
    pub fn new_driver_port(
        sess: &BackendSession,
        driver: &AudioDriver,
        name: &str,
        direction: &PortDirection,
        _ring: u32,
    ) -> Result<Self> {
        let control = Arc::new(
            ObjectControl::<MidiPortId, engine::MidiPortStateMirror>::pending(
                sess.shared.session_id,
            ),
        );
        let control_for_command = Arc::downgrade(&control);
        let dummy_output = Arc::new(Mutex::new(Vec::new()));
        let output_for_command = Arc::clone(&dummy_output);
        let (owned, dir) = (name.to_string(), *direction);
        let mut owned = Some(owned);
        let sequence = sess.shared.send_topology(move |s: &mut engine::Session| {
            let Some(control) = control_for_command.upgrade() else {
                return;
            };
            let Some(owned) = owned.take() else { return };
            if control.lifecycle() != ObjectLifecycle::Pending {
                return;
            }
            let mut external = engine::external_midi_port::ExternalMidiPort::new(owned, dir.into());
            external.set_output_capture(Arc::clone(&output_for_command));
            let port = engine::session::Port::ExternalMidi(external);
            match s.add_midi_port_with_state(port, Arc::clone(&control.mirror)) {
                Ok(idx) => control.mark_ready(MidiPortId(idx)),
                Err(error) => control.mark_failed(error.to_string()),
            }
        })?;
        control.set_creation_sequence(sequence);
        if let Err(error) = driver.register_midi_port(name, *direction, Arc::clone(&control)) {
            control.mark_failed(error.to_string());
            return Err(error);
        }
        Ok(Self {
            shared: sess.shared.clone(),
            control,
            dummy_output,
            direction: *direction,
            name: name.to_string(),
        })
    }
    pub fn input_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability::EXTERNAL,
            PortDirection::Output => PortConnectability::INTERNAL,
            PortDirection::Any => PortConnectability::INTERNAL.with(PortConnectability::EXTERNAL),
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability::INTERNAL,
            PortDirection::Output => PortConnectability::EXTERNAL,
            PortDirection::Any => PortConnectability::INTERNAL.with(PortConnectability::EXTERNAL),
        }
    }
    /// Queues a mutation of this port's MIDI side.
    fn with_midi_mut(
        &self,
        mut f: impl FnMut(&mut engine::MidiPort) + Send + 'static,
    ) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(m) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.midi_mut())
            {
                f(m)
            }
        })
    }

    /// This port's state as of the independently published mirrors, without blocking.
    pub fn poll_state(&self) -> Option<MidiPortState> {
        (self.lifecycle() == ObjectLifecycle::Ready)
            .then(|| self.control.mirror.read(self.name.clone()))
    }

    pub fn get_state(&self) -> Result<MidiPortState> {
        match self.lifecycle() {
            ObjectLifecycle::Pending | ObjectLifecycle::Ready => {
                Ok(self.control.mirror.read(self.name.clone()))
            }
            ObjectLifecycle::Failed => Err(anyhow!(
                "MIDI port creation failed: {}",
                self.control
                    .error()
                    .unwrap_or_else(|| "unknown error".to_string())
            )),
            ObjectLifecycle::Closed => Err(anyhow!("MIDI port is closed")),
        }
    }
    pub fn set_muted(&self, muted: bool) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_midi_mut(move |m| m.set_muted(muted));
        if result.is_ok() {
            self.control.mirror.set_muted(muted);
        }
        result
    }
    pub fn set_passthrough_muted(
        &self,
        muted: bool,
    ) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_midi_mut(move |m| m.set_passthrough_muted(muted));
        if result.is_ok() {
            self.control.mirror.set_passthrough_muted(muted);
        }
        result
    }
    pub fn connect_internal(
        &self,
        other: &MidiPort,
    ) -> std::result::Result<CommandSequence, SendError> {
        if self.control.session_id != other.control.session_id {
            return Err(SendError::Disconnected);
        }
        let (from, to) = (Arc::clone(&self.control), Arc::clone(&other.control));
        self.shared.send_topology(move |s: &mut engine::Session| {
            if let (Some(from), Some(to)) = (from.ready_id(), to.ready_id()) {
                let _ = s.connect_ports_internal(from.index(), to.index());
            }
        })
    }
    pub fn dummy_clear_queues(&self) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(p) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.as_external_midi_mut())
            {
                p.clear_queues();
            }
        })
    }
    pub fn queue_incoming_msgs(
        &self,
        msgs: Vec<MidiEvent>,
    ) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        let mut pending = Some(msgs);
        self.shared.send_control(move |s: &mut engine::Session| {
            let Some(msgs) = pending.take() else {
                return;
            };
            if let Some(p) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.as_external_midi_mut())
            {
                for m in msgs {
                    let _ = p.push_incoming(m.time.max(0) as u32, &m.data);
                }
            }
        })
    }
    pub fn dummy_queue_msg(
        &self,
        msg: &MidiEvent,
    ) -> std::result::Result<CommandSequence, SendError> {
        self.queue_incoming_msgs(vec![msg.clone()])
    }
    pub fn dummy_queue_msgs(
        &self,
        msgs: Vec<MidiEvent>,
    ) -> std::result::Result<CommandSequence, SendError> {
        self.queue_incoming_msgs(msgs)
    }
    pub fn dummy_dequeue_data(&self) -> Vec<MidiEvent> {
        let mut output = self
            .dummy_output
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        output.drain(..).collect()
    }
    pub fn dummy_request_data(&self, _n: u32) -> std::result::Result<CommandSequence, SendError> {
        let control = Arc::clone(&self.control);
        self.shared.send_control(move |s: &mut engine::Session| {
            if let Some(p) = control
                .ready_id()
                .and_then(|id| s.port_mut(id.index()))
                .and_then(|p| p.as_external_midi_mut())
            {
                p.request_output();
            }
        })
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        self.shared.connections_state(
            &self.name,
            self.direction,
            PortDataType::Midi,
            self.control.ready_id().map(ObjectIdentity::index),
        )
    }
    pub fn get_connections_state_now(&self) -> HashMap<String, bool> {
        self.shared.connections_state_now(
            &self.name,
            self.direction,
            PortDataType::Midi,
            self.control.ready_id().map(ObjectIdentity::index),
        )
    }
    pub fn connect_external_port(&self, name: &str) {
        self.shared.invalidate_connection_cache();
        if let Some(j) = self.shared.jack() {
            jack_connect_port(&j, &self.name, self.direction, name);
            self.shared.set_cached_connection(
                &self.name,
                self.direction,
                PortDataType::Midi,
                name,
                true,
            );
            return;
        }
        if let Some(ext) = self.shared.external() {
            if let Some(id) = self.control.ready_id() {
                let _ = ext
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .connect(compat_port_id(id.index()), name);
                self.shared.set_cached_connection(
                    &self.name,
                    self.direction,
                    PortDataType::Midi,
                    name,
                    true,
                );
                return;
            }
            let (control, name) = (Arc::clone(&self.control), name.to_string());
            if let Err(error) = self.shared.send_control(move |_: &mut engine::Session| {
                if let Some(id) = control.ready_id() {
                    let _ = crate::realtime_allow_lock!(
                        "deferred external MIDI connection",
                        ext.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner())
                    .connect(compat_port_id(id.index()), &name);
                }
            }) {
                log::error!("could not queue external MIDI connection: {error}");
            }
        }
    }
    pub fn disconnect_external_port(&self, name: &str) {
        self.shared.invalidate_connection_cache();
        if let Some(j) = self.shared.jack() {
            jack_disconnect_port(&j, &self.name, self.direction, name);
            self.shared.set_cached_connection(
                &self.name,
                self.direction,
                PortDataType::Midi,
                name,
                false,
            );
            return;
        }
        if let Some(ext) = self.shared.external() {
            if let Some(id) = self.control.ready_id() {
                let _ = ext
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .disconnect(compat_port_id(id.index()), name);
                self.shared.set_cached_connection(
                    &self.name,
                    self.direction,
                    PortDataType::Midi,
                    name,
                    false,
                );
                return;
            }
            let (control, name) = (Arc::clone(&self.control), name.to_string());
            if let Err(error) = self.shared.send_control(move |_: &mut engine::Session| {
                if let Some(id) = control.ready_id() {
                    let _ = crate::realtime_allow_lock!(
                        "deferred external MIDI disconnection",
                        ext.lock()
                    )
                    .unwrap_or_else(|e| e.into_inner())
                    .disconnect(compat_port_id(id.index()), &name);
                }
            }) {
                log::error!("could not queue external MIDI disconnection: {error}");
            }
        }
    }
    pub fn set_ringbuffer_n_samples(
        &self,
        n: u32,
    ) -> std::result::Result<CommandSequence, SendError> {
        let result = self.with_midi_mut(move |m| m.set_ringbuffer_n_samples(n));
        if result.is_ok() {
            self.control.mirror.set_ringbuffer_n_samples(n);
        }
        result
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
}

static NEXT_DECOUPLED_PORT_ID: AtomicU64 = AtomicU64::new(100_000);
pub struct DecoupledMidiPort {
    name: String,
    direction: PortDirection,
    port_id: engine::PortId,
    queue: Arc<Mutex<Vec<MidiEvent>>>,
    external: Arc<Mutex<engine::DummyExternalConnections>>,
    jack: Option<Arc<Mutex<JackBackend>>>,
    connections_cache: Arc<Mutex<HashMap<String, bool>>>,
    connections_last_refresh: Mutex<Instant>,
}
impl DecoupledMidiPort {
    pub fn new_driver_port(
        driver: &AudioDriver,
        name: &str,
        direction: &PortDirection,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(Vec::new()));
        let port_id = engine::PortId(NEXT_DECOUPLED_PORT_ID.fetch_add(1, Ordering::Relaxed));
        driver.register_decoupled_midi_port(name, *direction, port_id, queue.clone())?;
        Ok(Self {
            name: name.to_string(),
            direction: *direction,
            port_id,
            queue,
            external: driver.external(),
            jack: driver.jack(),
            connections_cache: Arc::new(Mutex::new(HashMap::new())),
            connections_last_refresh: Mutex::new(Instant::now() - Duration::from_secs(1)),
        })
    }
    pub fn maybe_next_message(&self) -> Option<MidiEvent> {
        self.queue.lock().unwrap().pop()
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn send_midi(&self, msg: &[u8]) {
        self.queue
            .lock()
            .unwrap()
            .push(MidiEvent::new(0, msg.to_vec()))
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        let cached = self
            .connections_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut last = self
            .connections_last_refresh
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if last.elapsed() < Duration::from_millis(100) {
            return cached;
        }
        *last = Instant::now();
        let (cache, jack, external, name, direction, port_id) = (
            Arc::clone(&self.connections_cache),
            self.jack.clone(),
            Arc::clone(&self.external),
            self.name.clone(),
            self.direction,
            self.port_id,
        );
        let _ = thread::Builder::new()
            .name("engine-midi-connection-cache".to_string())
            .spawn(move || {
                let _span = tracing::debug_span!("worker.engine.midi_connection_cache").entered();
                let state = if let Some(jack) = jack {
                    let jack = jack.lock().unwrap_or_else(|e| e.into_inner());
                    jack_connections_state_locked(&jack, &name, direction, PortDataType::Midi)
                } else {
                    external
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .connection_status_of(port_id)
                        .into_iter()
                        .collect()
                };
                *cache.lock().unwrap_or_else(|e| e.into_inner()) = state;
            })
            .expect("spawn MIDI connection cache worker");
        cached
    }
    /// Synchronously refreshes after an explicit user-triggered reconciliation.
    /// Periodic getters remain cache-only and use the asynchronous path above.
    pub fn refresh_connections_now(&self) {
        let state = if let Some(jack) = self.jack.as_ref() {
            let jack = jack.lock().unwrap_or_else(|e| e.into_inner());
            jack_connections_state_locked(&jack, &self.name, self.direction, PortDataType::Midi)
        } else {
            self.external
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .connection_status_of(self.port_id)
                .into_iter()
                .collect()
        };
        *self
            .connections_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = state;
        *self
            .connections_last_refresh
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    pub fn connect_external_port(&self, name: &str) {
        if let Some(j) = self.jack.as_ref() {
            jack_connect_port(j, &self.name, self.direction, name);
        } else {
            let _ = self
                .external
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .connect(self.port_id, name);
        }
        self.refresh_connections_now();
    }
    pub fn disconnect_external_port(&self, name: &str) {
        if let Some(j) = self.jack.as_ref() {
            jack_disconnect_port(j, &self.name, self.direction, name);
        } else {
            let _ = self
                .external
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .disconnect(self.port_id, name);
        }
        self.refresh_connections_now();
    }
}

pub type FXChainState = engine::FXChainState;

enum FXChainBackendKind {
    Test2x2x1,
    Tiny(Mutex<engine::tiny_synth_fx::TinySynthFxControlState>),
    OxiSynth,
    #[cfg(feature = "carla")]
    Carla(engine::carla_processor::CarlaControlHandle),
    Unavailable {
        reason: String,
    },
}

pub struct FXChain {
    shared: Arc<SharedSession>,
    title: String,
    backend: FXChainBackendKind,
    state: Arc<Mutex<FXChainState>>,
    tiny_channels: usize,
    output_ringbuffer_n_samples: usize,
    audio_inputs: Vec<AudioPort>,
    audio_outputs: Vec<AudioPort>,
    midi_inputs: Vec<MidiPort>,
    midi_outputs: Vec<MidiPort>,
}
impl FXChain {
    pub fn available(&self) -> bool {
        !matches!(self.backend, FXChainBackendKind::Unavailable { .. })
    }
    pub fn set_visible(&self, visible: bool) {
        self.state.lock().unwrap().visible = visible as u32;
        #[cfg(feature = "carla")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            let ok = host.set_visible(visible).is_ok();
            self.state.lock().unwrap().visible = (visible && ok) as u32;
        }
    }
    pub fn set_active(&self, active: bool) {
        self.state.lock().unwrap().active = active as u32;
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => {
                let mut pending = Some(self.title.clone());
                if let Err(error) = self.shared.send_control(move |s: &mut engine::Session| {
                    if let Some(title) = pending.take() {
                        s.set_test_fx_active(title, active);
                    }
                }) {
                    log::error!("could not queue FX active state: {error}");
                }
            }
            FXChainBackendKind::Tiny(_) => {
                let title = self.title.clone();
                if let Err(error) = self.shared.send_control(move |session| {
                    session.set_tiny_synth_fx_active(&title, active);
                }) {
                    log::error!("could not queue Tiny Synth/FX active state: {error}");
                }
            }
            FXChainBackendKind::OxiSynth => {
                let title = self.title.clone();
                if let Err(error) = self.shared.send_control(move |session| {
                    session.set_oxisynth_active(&title, active);
                }) {
                    log::error!("could not queue OxiSynth active state: {error}");
                }
            }
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.set_active(active),
            FXChainBackendKind::Unavailable { .. } => {}
        }
    }
    pub fn get_state(&self) -> Option<FXChainState> {
        let mut s = self.state.lock().unwrap().clone();
        s.ready = self.available() as u32;
        #[cfg(feature = "carla")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            s.ready = host.is_ready() as u32;
            s.active = host.is_active() as u32;
            s.visible = host.is_visible() as u32;
            self.state.lock().unwrap().visible = s.visible;
        }
        Some(s)
    }
    pub fn toggle_or_recover(&self) -> Result<()> {
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.toggle_or_recover(),
            FXChainBackendKind::Test2x2x1
            | FXChainBackendKind::Tiny(_)
            | FXChainBackendKind::OxiSynth => {
                self.set_visible(!self.get_state().is_some_and(|state| state.visible != 0));
                Ok(())
            }
            FXChainBackendKind::Unavailable { reason } => Err(anyhow!(reason.clone())),
        }
    }

    pub fn lifecycle(&self) -> engine::carla_processor::CarlaProcessorLifecycle {
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.lifecycle(),
            FXChainBackendKind::Test2x2x1
            | FXChainBackendKind::Tiny(_)
            | FXChainBackendKind::OxiSynth => {
                engine::carla_processor::CarlaProcessorLifecycle::Running
            }
            FXChainBackendKind::Unavailable { .. } => {
                engine::carla_processor::CarlaProcessorLifecycle::Unavailable
            }
        }
    }

    pub fn generation(&self) -> u64 {
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.generation(),
            _ => 0,
        }
    }

    pub fn crash_summary(&self) -> Option<String> {
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.crash_summary(),
            FXChainBackendKind::Unavailable { reason } => Some(reason.clone()),
            _ => None,
        }
    }

    pub fn generation_logs(&self) -> Vec<engine::carla_processor::CarlaGenerationLog> {
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.generation_logs(),
            _ => Vec::new(),
        }
    }

    pub fn clear_logs(&self) {
        #[cfg(feature = "carla")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            host.clear_logs();
        }
    }

    pub fn try_get_state_str(&self) -> Result<String> {
        match &self.backend {
            FXChainBackendKind::Unavailable { reason } => Err(anyhow!(reason.clone())),
            FXChainBackendKind::Tiny(control) => Ok(control.lock().unwrap().encode()),
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.save_state(),
            _ => Ok(String::new()),
        }
    }

    pub fn get_state_str(&self) -> Option<String> {
        self.try_get_state_str().ok()
    }

    pub fn try_restore_state(&self, state: &str) -> Result<()> {
        #[cfg(not(feature = "carla"))]
        let _ = state;
        match &self.backend {
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.restore_state(state),
            FXChainBackendKind::Tiny(control) => {
                let sample_rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1);
                let assignments = control.lock().unwrap().midi_cc_assignments();
                let mut replacement = engine::tiny_synth_fx::TinySynthFxControlState::from_encoded(
                    sample_rate as f32,
                    state,
                )?;
                replacement.set_midi_cc_assignments(assignments);
                let processor = replacement.prepare_processor(
                    sample_rate as f32,
                    self.tiny_channels,
                    self.shared.buffer_size.load(Ordering::Relaxed).max(1) as usize,
                )?;
                let title = self.title.clone();
                let displaced = self.shared.query_graph_scheduler_response(move |session| {
                    session.set_tiny_synth_fx_processor(title, processor)
                })?;
                drop(displaced);
                *control.lock().unwrap() = replacement;
                Ok(())
            }
            FXChainBackendKind::Test2x2x1 => Ok(()),
            FXChainBackendKind::OxiSynth => Err(anyhow!("OxiSynth has no persistent state")),
            FXChainBackendKind::Unavailable { reason } => Err(anyhow!(reason.clone())),
        }
    }

    pub fn restore_state(&self, state: &str) {
        let _ = self.try_restore_state(state);
    }

    pub fn tiny_editor_state(&self) -> Option<engine::tiny_synth_fx::TinySynthFxEditorState> {
        match &self.backend {
            FXChainBackendKind::Tiny(control) => Some(control.lock().unwrap().editor_state()),
            _ => None,
        }
    }

    pub fn tiny_select_preset(&self, id: &str) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !engine::tiny_synth_fx::available_presets().any(|(preset_id, _)| preset_id == id) {
            return Err(anyhow!("unknown Tiny Synth/FX preset {id}"));
        }
        let title = self.title.clone();
        let id = id.to_owned();
        let callback_id = id.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.select_preset(&callback_id);
            }
        })?;
        control.lock().unwrap().select_preset(&id)?;
        Ok(())
    }

    pub fn tiny_set_master_gain_db(&self, gain_db: f32) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !gain_db.is_finite()
            || !(engine::tiny_synth_fx::MIN_MASTER_GAIN_DB
                ..=engine::tiny_synth_fx::MAX_MASTER_GAIN_DB)
                .contains(&gain_db)
        {
            return Err(anyhow!("invalid Tiny Synth/FX master gain"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_master_gain_db(gain_db);
            }
        })?;
        control.lock().unwrap().set_master_gain_db(gain_db)?;
        Ok(())
    }

    pub fn tiny_set_reverb_enabled(&self, enabled: bool) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_reverb_enabled(enabled);
            }
        })?;
        control.lock().unwrap().set_reverb_enabled(enabled);
        Ok(())
    }

    pub fn tiny_set_reverb_amount(&self, amount: f32) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
            return Err(anyhow!("invalid Tiny Synth/FX reverb amount"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_reverb_amount(amount);
            }
        })?;
        control.lock().unwrap().set_reverb_amount(amount)?;
        Ok(())
    }

    pub fn tiny_set_distortion_enabled(&self, enabled: bool) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_distortion_enabled(enabled);
            }
        })?;
        control.lock().unwrap().set_distortion_enabled(enabled);
        Ok(())
    }

    pub fn tiny_set_distortion_drive(&self, drive: f32) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !drive.is_finite() || !(1.0..=20.0).contains(&drive) {
            return Err(anyhow!("invalid Tiny Synth/FX distortion drive"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_distortion_drive(drive);
            }
        })?;
        control.lock().unwrap().set_distortion_drive(drive)?;
        Ok(())
    }

    pub fn tiny_set_compressor_enabled(&self, enabled: bool) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_compressor_enabled(enabled);
            }
        })?;
        control.lock().unwrap().set_compressor_enabled(enabled);
        Ok(())
    }

    pub fn tiny_set_compressor_amount(&self, amount: f32) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
            return Err(anyhow!("invalid Tiny Synth/FX compressor amount"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_compressor_amount(amount);
            }
        })?;
        control.lock().unwrap().set_compressor_amount(amount)?;
        Ok(())
    }

    pub fn tiny_set_eq_enabled(&self, enabled: bool) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.set_eq_enabled(enabled);
            }
        })?;
        control.lock().unwrap().set_eq_enabled(enabled);
        Ok(())
    }

    pub fn tiny_set_eq_low_db(&self, gain_db: f32) -> Result<()> {
        self.tiny_set_eq_gain(
            gain_db,
            |processor, value| processor.set_eq_low_db(value),
            |control, value| control.set_eq_low_db(value),
        )
    }

    pub fn tiny_set_eq_mid_db(&self, gain_db: f32) -> Result<()> {
        self.tiny_set_eq_gain(
            gain_db,
            |processor, value| processor.set_eq_mid_db(value),
            |control, value| control.set_eq_mid_db(value),
        )
    }

    pub fn tiny_set_eq_high_db(&self, gain_db: f32) -> Result<()> {
        self.tiny_set_eq_gain(
            gain_db,
            |processor, value| processor.set_eq_high_db(value),
            |control, value| control.set_eq_high_db(value),
        )
    }

    fn tiny_set_eq_gain(
        &self,
        gain_db: f32,
        mut update_processor: impl FnMut(&mut engine::tiny_synth_fx::TinySynthFxProcessor, f32)
            + Send
            + 'static,
        update_control: impl FnOnce(
            &mut engine::tiny_synth_fx::TinySynthFxControlState,
            f32,
        ) -> Result<(), tinyviolin::ProcessError>,
    ) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if !gain_db.is_finite()
            || !(engine::tiny_synth_fx::MIN_EQ_GAIN_DB..=engine::tiny_synth_fx::MAX_EQ_GAIN_DB)
                .contains(&gain_db)
        {
            return Err(anyhow!("invalid Tiny Synth/FX EQ gain"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                update_processor(processor, gain_db);
            }
        })?;
        update_control(&mut control.lock().unwrap(), gain_db)?;
        Ok(())
    }

    pub fn tiny_assign_midi_cc(
        &self,
        assignment: engine::tiny_synth_fx::TinySynthFxMidiCcAssignment,
    ) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        if assignment.channel > 15 || assignment.controller > 127 {
            return Err(anyhow!("invalid Tiny Synth/FX MIDI CC assignment"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.assign_midi_cc(assignment);
            }
        })?;
        control.lock().unwrap().assign_midi_cc(assignment);
        Ok(())
    }

    pub fn tiny_remove_midi_cc(
        &self,
        parameter: engine::tiny_synth_fx::TinySynthFxParameter,
    ) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.remove_midi_cc(parameter);
            }
        })?;
        control.lock().unwrap().remove_midi_cc(parameter);
        Ok(())
    }

    pub fn tiny_clear_midi_cc_assignments(&self) -> Result<()> {
        let FXChainBackendKind::Tiny(control) = &self.backend else {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        };
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.clear_midi_cc_assignments();
            }
        })?;
        control.lock().unwrap().clear_midi_cc_assignments();
        Ok(())
    }

    pub fn tiny_panic(&self) -> Result<()> {
        if !matches!(&self.backend, FXChainBackendKind::Tiny(_)) {
            return Err(anyhow!("FX chain is not Tiny Synth/FX"));
        }
        let title = self.title.clone();
        self.shared.send_control(move |session| {
            if let Some(processor) = session.tiny_synth_fx_processor_mut(&title) {
                processor.panic();
            }
        })?;
        Ok(())
    }

    fn n_audio_input_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => 2,
            FXChainBackendKind::Tiny(_) => self.tiny_channels,
            FXChainBackendKind::OxiSynth => 0,
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.info().audio_inputs,
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }

    fn n_audio_output_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => 2,
            FXChainBackendKind::Tiny(_) => self.tiny_channels,
            FXChainBackendKind::OxiSynth => 2,
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.info().audio_outputs,
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }
    fn n_midi_input_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1
            | FXChainBackendKind::Tiny(_)
            | FXChainBackendKind::OxiSynth => 1,
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.info().midi_inputs,
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }

    fn n_midi_output_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1
            | FXChainBackendKind::Tiny(_)
            | FXChainBackendKind::OxiSynth => 0,
            #[cfg(feature = "carla")]
            FXChainBackendKind::Carla(host) => host.info().midi_outputs,
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }
    /// Queues an internal chain port and immediately returns its pending handle.
    fn make_audio_port(
        &self,
        name: String,
        direction: PortDirection,
        ringbuffer_n_samples: usize,
    ) -> Option<AudioPort> {
        let control = Arc::new(
            ObjectControl::<AudioPortId, engine::AudioPortStateMirror>::pending(
                self.shared.session_id,
            ),
        );
        let weak = Arc::downgrade(&control);
        let mut owned = Some(name.clone());
        let sequence = self
            .shared
            .send_topology(move |s: &mut engine::Session| {
                let Some(control) = weak.upgrade() else {
                    return;
                };
                let Some(owned) = owned.take() else { return };
                let n_frames = s.buffer_size().max(1) as usize;
                let ringbuffer_buffer_size = if ringbuffer_n_samples == 0 {
                    0
                } else {
                    ringbuffer_n_samples.div_ceil(32).max(n_frames)
                };
                let mut port = engine::InternalAudioPort::new(
                    owned,
                    n_frames,
                    engine::PortConnectability::INTERNAL,
                    engine::PortConnectability::INTERNAL,
                    ringbuffer_buffer_size,
                );
                port.audio_mut()
                    .set_ringbuffer_n_samples(ringbuffer_n_samples);
                let port = engine::session::Port::Internal(port);
                match s.add_audio_port_with_state(port, Arc::clone(&control.mirror)) {
                    Ok(idx) => control.mark_ready(AudioPortId(idx)),
                    Err(error) => control.mark_failed(error.to_string()),
                }
            })
            .ok()?;
        control.set_creation_sequence(sequence);
        Some(AudioPort {
            shared: self.shared.clone(),
            control,
            dummy_output: Arc::new(Mutex::new(Vec::new())),
            direction,
            name,
        })
    }

    fn make_midi_port(&self, name: String, direction: PortDirection) -> Option<MidiPort> {
        let control = Arc::new(
            ObjectControl::<MidiPortId, engine::MidiPortStateMirror>::pending(
                self.shared.session_id,
            ),
        );
        let weak = Arc::downgrade(&control);
        let dummy_output = Arc::new(Mutex::new(Vec::new()));
        let output_capture = Arc::clone(&dummy_output);
        let mut owned = Some(name.clone());
        let sequence = self
            .shared
            .send_topology(move |s: &mut engine::Session| {
                let Some(control) = weak.upgrade() else {
                    return;
                };
                let Some(owned) = owned.take() else { return };
                let mut external =
                    engine::external_midi_port::ExternalMidiPort::new(owned, direction.into());
                external.set_output_capture(output_capture.clone());
                let port = engine::session::Port::ExternalMidi(external);
                match s.add_midi_port_with_state(port, Arc::clone(&control.mirror)) {
                    Ok(idx) => control.mark_ready(MidiPortId(idx)),
                    Err(error) => control.mark_failed(error.to_string()),
                }
            })
            .ok()?;
        control.set_creation_sequence(sequence);
        Some(MidiPort {
            shared: self.shared.clone(),
            control,
            dummy_output,
            direction,
            name,
        })
    }
    fn bind_processor_ports(&self) -> Result<()> {
        if matches!(self.backend, FXChainBackendKind::Unavailable { .. }) {
            return Ok(());
        }
        let title = self.title.clone();
        let audio_inputs = self
            .audio_inputs
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        let audio_outputs = self
            .audio_outputs
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        let midi_inputs = self
            .midi_inputs
            .iter()
            .map(|port| Arc::clone(&port.control))
            .collect::<Vec<_>>();
        self.shared.send_topology(move |session| {
            let audio_inputs = audio_inputs
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let audio_outputs = audio_outputs
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let midi_inputs = midi_inputs
                .iter()
                .filter_map(|control| control.ready_id().map(|id| id.index()))
                .collect();
            let _ = session.set_processor_ports(&title, audio_inputs, audio_outputs, midi_inputs);
        })?;
        Ok(())
    }

    fn create_ports_once(&mut self) {
        for idx in 0..self.n_audio_input_ports() {
            if let Some(port) = self.make_audio_port(
                format!("{}:audio_in_{}", self.title, idx),
                PortDirection::Output,
                0,
            ) {
                self.audio_inputs.push(port);
            }
        }
        for idx in 0..self.n_audio_output_ports() {
            if let Some(port) = self.make_audio_port(
                format!("{}:audio_out_{}", self.title, idx),
                PortDirection::Input,
                self.output_ringbuffer_n_samples,
            ) {
                self.audio_outputs.push(port);
            }
        }
        for idx in 0..self.n_midi_input_ports() {
            if let Some(port) = self.make_midi_port(
                format!("{}:midi_in_{}", self.title, idx),
                PortDirection::Output,
            ) {
                self.midi_inputs.push(port);
            }
        }
        for idx in 0..self.n_midi_output_ports() {
            if let Some(port) = self.make_midi_port(
                format!("{}:midi_out_{}", self.title, idx),
                PortDirection::Input,
            ) {
                self.midi_outputs.push(port);
            }
        }
    }

    pub fn get_audio_input_port(&self, idx: u32) -> Option<AudioPort> {
        self.audio_inputs.get(idx as usize).cloned()
    }
    pub fn get_audio_output_port(&self, idx: u32) -> Option<AudioPort> {
        self.audio_outputs.get(idx as usize).cloned()
    }
    pub fn get_midi_input_port(&self, idx: u32) -> Option<MidiPort> {
        self.midi_inputs.get(idx as usize).cloned()
    }
    pub fn get_midi_output_port(&self, idx: u32) -> Option<MidiPort> {
        self.midi_outputs.get(idx as usize).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the schedule matches the topology, asked of the engine.
    ///
    /// A queued read rather than a peek at the session: there is no longer a lock to take, and
    /// going through the queue is also what makes this ordered behind whatever mutations the
    /// test just issued -- which is exactly what these assertions are about.
    fn graph_up_to_date(sess: &BackendSession) -> bool {
        sess.shared
            .query_for_test(|s: &mut engine::Session| s.graph_up_to_date())
            .expect("engine answered")
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_command_batch_removed_from_the_queue_keeps_the_graph_scheduler_armed() {
        let sess = BackendSession::new().expect("session");
        let scheduler = sess.shared.scheduler.get().expect("scheduler");
        let before = scheduler.n_arms();
        {
            let handle = sess.shared.handle.lock().unwrap_or_else(|e| e.into_inner());
            handle
                .stats()
                .command_batch_in_flight
                .store(true, Ordering::Release);
        }

        assert!(!sess.shared.graph_may_need_rebuild());
        assert!(scheduler.n_arms() > before);

        let handle = sess.shared.handle.lock().unwrap_or_else(|e| e.into_inner());
        handle
            .stats()
            .command_batch_in_flight
            .store(false, Ordering::Release);
    }

    /// The invariant `ControlGuard` exists to enforce.
    ///
    /// Connecting a channel to a port used to leave the graph dirty with nothing scheduled
    /// to rebuild it, because only three of the mutation sites remembered to call
    /// `apply_graph_changes`. Now the guard cannot be dropped without at least arming the
    /// rebuild, so no mutation site has to remember.
    #[shoop_wasm_test_support::shoop_test]
    fn a_connection_leaves_the_graph_scheduled_for_rebuild() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let channel = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel");
        let port = {
            let idx = sess
                .shared
                .query_topology_for_test(|s: &mut engine::Session| {
                    s.add_port(engine::session::Port::External(
                        engine::external_audio_port::ExternalAudioPort::new(
                            "out",
                            engine::PortDirection::Output,
                            0,
                        ),
                    ))
                })
                .expect("add port");
            let control = Arc::new(
                ObjectControl::<AudioPortId, engine::AudioPortStateMirror>::pending(
                    sess.shared.session_id,
                ),
            );
            control.mark_ready(AudioPortId(idx));
            AudioPort {
                shared: sess.shared.clone(),
                control,
                dummy_output: Arc::new(Mutex::new(Vec::new())),
                direction: PortDirection::Output,
                name: "out".to_string(),
            }
        };
        sess.shared.flush_graph_changes();
        assert!(graph_up_to_date(&sess));

        // The call that used to leave the session permanently stale.
        channel
            .connect_output(&port)
            .expect("queue output connection");
        assert!(
            !graph_up_to_date(&sess),
            "the connection should have marked the graph dirty"
        );

        // Not applied inline -- that is the coalescing -- but guaranteed to land.
        sess.shared.flush_graph_changes();
        assert!(
            graph_up_to_date(&sess),
            "a pending rebuild must be applied without any explicit apply_graph_changes call"
        );
    }

    /// Many mutations in a burst must not each pay for a rebuild.
    #[shoop_wasm_test_support::shoop_test]
    fn a_burst_of_changes_coalesces_into_one_rebuild() {
        let sess = BackendSession::new().expect("session");
        let before = sess.shared.scheduler.get().expect("scheduler").n_applies();

        for _ in 0..25 {
            let loop_ = sess.create_loop().expect("loop");
            loop_
                .add_audio_channel(ChannelMode::Direct)
                .expect("channel");
        }
        sess.shared.flush_graph_changes();

        let after = sess.shared.scheduler.get().expect("scheduler").n_applies();
        assert!(
            after - before < 25,
            "50 mutations should coalesce, got {} rebuilds",
            after - before
        );
        assert!(graph_up_to_date(&sess));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn scalar_control_commands_do_not_arm_graph_rebuilds() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        sess.shared.flush_graph_changes();
        let scheduler = sess.shared.scheduler.get().expect("scheduler");
        let before = scheduler.n_arms();

        let sequence = loop_.set_length(64).expect("queue length");
        sess.shared
            .wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("command fence");

        assert_eq!(scheduler.n_arms(), before);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_content_commits_atomically_without_rebuilding_the_graph() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let left = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("left channel");
        let right = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("right channel");
        let midi = loop_
            .add_midi_channel(ChannelMode::Direct)
            .expect("MIDI channel");
        left.load_data(&[1.0, 2.0]).expect("initial left");
        right.load_data(&[3.0, 4.0]).expect("initial right");
        midi.load_midi_data(&[MidiEvent::new(1, vec![0x90, 60, 100])], 2)
            .expect("initial MIDI");
        loop_.set_length(2).expect("initial length");
        loop_
            .transition(LoopMode::Playing, -1, -1)
            .expect("initial mode");
        loop_
            .transition(LoopMode::Stopped, 2, -1)
            .expect("planned stop");
        sess.shared.flush_graph_changes();
        let scheduler = sess.shared.scheduler.get().expect("scheduler");
        let graph_arms = scheduler.n_arms();
        let graph_applies = scheduler.n_applies();
        let session_id = sess.session_id();
        let mut engine = sess.shared.take_engine().expect("parked engine");

        let sequence = replace_loop_content(
            &loop_,
            &[
                LoopAudioContentUpdate {
                    channel: &left,
                    samples: &[10.0, 11.0, 12.0],
                    start_offset: Some(-1),
                    preplay: Some(5),
                },
                LoopAudioContentUpdate {
                    channel: &right,
                    samples: &[20.0, 21.0, 22.0],
                    start_offset: Some(-2),
                    preplay: Some(6),
                },
            ],
            &[LoopMidiContentUpdate {
                channel: &midi,
                messages: &[
                    MidiEvent::new(-1, vec![0xB0, 7, 99]),
                    MidiEvent::new(2, vec![0x90, 64, 127]),
                ],
                length: 3,
                start_offset: Some(-3),
                preplay: Some(7),
            }],
            Some(3),
        )
        .expect("queue atomic content");

        assert_eq!(
            sequence.get(),
            engine.stats().last_applied_command.load(Ordering::Relaxed) + 1
        );
        assert_eq!(
            engine.session().audio_channel(0).unwrap().data(),
            [1.0, 2.0]
        );
        assert_eq!(
            engine.session().audio_channel(1).unwrap().data(),
            [3.0, 4.0]
        );
        assert_eq!(
            engine.session().loop_(0).unwrap().mode(),
            engine::LoopMode::Playing
        );
        assert_eq!(
            engine.session().loop_(0).unwrap().n_planned_transitions(),
            1
        );

        engine.pump();

        assert_eq!(
            engine.session().audio_channel(0).unwrap().data(),
            [10.0, 11.0, 12.0]
        );
        assert_eq!(
            engine.session().audio_channel(1).unwrap().data(),
            [20.0, 21.0, 22.0]
        );
        assert_eq!(
            engine.session().audio_channel(0).unwrap().start_offset(),
            -1
        );
        assert_eq!(
            engine
                .session()
                .audio_channel(1)
                .unwrap()
                .pre_play_samples(),
            6
        );
        let midi_id = midi.control.ready_id().expect("MIDI identity").index();
        assert_eq!(engine.session().midi_channel(midi_id).unwrap().length(), 3);
        assert_eq!(
            engine.session().midi_channel(midi_id).unwrap().contents()[0].time,
            2
        );
        assert_eq!(engine.session().loop_(0).unwrap().length(), 3);
        assert_eq!(
            engine.session().loop_(0).unwrap().mode(),
            engine::LoopMode::Stopped
        );
        assert_eq!(
            engine.session().loop_(0).unwrap().n_planned_transitions(),
            0
        );
        assert_eq!(sess.session_id(), session_id);
        assert_eq!(scheduler.n_arms(), graph_arms);
        assert_eq!(scheduler.n_applies(), graph_applies);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prepared_content_commit_allocates_and_locks_only_off_realtime() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let audio = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("audio channel");
        let midi = loop_
            .add_midi_channel(ChannelMode::Direct)
            .expect("MIDI channel");
        let audio_samples = vec![0.5; 16_384];
        let audio_snapshot = audio
            .snapshot_control
            .prepare(
                &audio_samples,
                engine::content_snapshot::ContentMutation::Loading,
            )
            .expect("audio snapshot");
        let mut prepared_audio = engine::PreparedAudioChannelData::new(64, audio_samples.len());
        prepared_audio.begin_load(audio_samples.len());
        prepared_audio.write(0, &audio_samples);
        let midi_messages = vec![MidiEvent::new(8192, vec![0x90, 64, 127])];
        let midi_snapshot = midi
            .snapshot_control
            .prepare(
                &midi_messages,
                16_384,
                engine::content_snapshot::ContentMutation::Loading,
            )
            .expect("MIDI snapshot");
        let elements = vec![engine::MidiStorageElem::new(8192, &[0x90, 64, 127]).unwrap()];
        let mut prepared_midi = engine::PreparedMidiChannelData::new(&elements, 16_384, None);
        let audio_id = audio.control.ready_id().expect("audio identity").index();
        let midi_id = midi.control.ready_id().expect("MIDI identity").index();
        let loop_id = loop_.control.ready_id().expect("loop identity").index();
        let mut engine = sess.shared.take_engine().expect("parked engine");
        struct DisableLockGuard;
        impl Drop for DisableLockGuard {
            fn drop(&mut self) {
                crate::realtime_lock_guard::set_enabled(false);
            }
        }

        crate::realtime_lock_guard::set_enabled(true);
        let _disable_lock_guard = DisableLockGuard;
        assert_no_alloc::assert_no_alloc(|| {
            crate::realtime_lock_guard::forbid_locks_if_enabled(|| {
                engine
                    .session_mut()
                    .audio_channel_mut(audio_id)
                    .unwrap()
                    .commit_prepared_data_and_snapshot(&mut prepared_audio, audio_snapshot);
                engine
                    .session_mut()
                    .midi_channel_mut(midi_id)
                    .unwrap()
                    .commit_prepared_data_and_snapshot(&mut prepared_midi, midi_snapshot);
                let loop_ = engine.session_mut().loop_mut(loop_id).unwrap();
                loop_.clear_planned_transitions();
                loop_.set_mode(engine::LoopMode::Stopped);
                loop_.set_length(16_384);
            });
        });

        assert_eq!(
            engine.session().audio_channel(audio_id).unwrap().length(),
            16_384
        );
        assert_eq!(
            engine.session().midi_channel(midi_id).unwrap().length(),
            16_384
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn length_only_update_preserves_content_and_playback_with_modulo_position() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let audio = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel");
        audio.load_data(&[1.0, 2.0, 3.0, 4.0]).expect("content");
        loop_.set_length(32).expect("initial length");
        loop_
            .transition(LoopMode::Playing, -1, -1)
            .expect("playing");
        loop_.set_position(23).expect("position");
        sess.shared.flush_graph_changes();
        let scheduler = sess.shared.scheduler.get().expect("scheduler");
        let graph_arms = scheduler.n_arms();
        let graph_applies = scheduler.n_applies();
        let mut engine = sess.shared.take_engine().expect("parked engine");

        loop_.set_length(7).expect("shorter length");
        engine.pump();

        let state = engine.session().loop_(0).expect("engine loop");
        assert_eq!(state.length(), 7);
        assert_eq!(state.position(), 2);
        assert_eq!(state.mode(), engine::LoopMode::Playing);
        assert_eq!(
            engine.session().audio_channel(0).unwrap().data(),
            [1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(scheduler.n_arms(), graph_arms);
        assert_eq!(scheduler.n_applies(), graph_applies);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_full_parked_queue_is_drained_and_retried_without_loss() {
        let sess = BackendSession::create_with_capacity(1).expect("session");
        {
            let mut handle = sess
                .shared
                .handle
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            handle
                .send(Box::new(|_: &mut engine::Session| {}))
                .expect("fill queue");
        }
        let applied = Arc::new(AtomicU32::new(0));
        let applied_by_command = Arc::clone(&applied);

        let sequence = sess
            .shared
            .send_control(move |_: &mut engine::Session| {
                applied_by_command.fetch_add(1, Ordering::Relaxed);
            })
            .expect("retry after capacity becomes available");

        assert_eq!(sequence.get(), 2);
        assert_eq!(applied.load(Ordering::Relaxed), 1);
        assert_eq!(
            sess.shared
                .handle
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .stats()
                .commands_applied
                .load(Ordering::Relaxed),
            2
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_creation_and_followup_commands_do_not_wait_for_a_cycle() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        engine.session_mut().set_sample_rate(48_000);
        engine.session_mut().set_buffer_size(48_000);

        let loop_ = sess.create_loop().expect("pending loop");
        assert_eq!(loop_.lifecycle(), ObjectLifecycle::Pending);
        assert_eq!(loop_.creation_sequence().get(), 1);
        assert!(loop_.poll_state().is_none());
        assert!(loop_.get_state().is_err());
        assert_eq!(engine.session().n_loops(), 0);

        let setter = loop_.set_length(64).expect("queue length");
        assert_eq!(setter.get(), 2);
        assert_eq!(loop_.lifecycle(), ObjectLifecycle::Pending);

        engine.pump();
        assert_eq!(loop_.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(loop_.get_state().expect("mirrored state").length, 64);
        assert_eq!(engine.session().n_loops(), 1);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_channels_resolve_after_their_parent_and_apply_followups() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let loop_ = sess.create_loop().expect("loop");
        let audio = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("audio channel");
        let midi = loop_
            .add_midi_channel(ChannelMode::Direct)
            .expect("MIDI channel");
        assert_eq!(audio.lifecycle(), ObjectLifecycle::Pending);
        assert_eq!(midi.lifecycle(), ObjectLifecycle::Pending);
        audio.set_gain(0.25).expect("queue gain");
        audio.load_data(&[1.0, 2.0]).expect("queue audio data");
        midi.load_all_midi_data(&[MidiEvent {
            time: 3,
            data: vec![0x90, 60, 100],
        }])
        .expect("queue MIDI data");

        engine.pump();
        assert_eq!(audio.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(midi.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(audio.get_state().expect("audio state").gain, 0.25);
        let start = Instant::now();
        while audio.try_get_current_data_snapshot().is_err()
            || midi.try_get_current_data_snapshot().is_err()
        {
            assert!(start.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }
        assert_eq!(audio.get_data(), vec![1.0, 2.0]);
        assert_eq!(midi.get_all_midi_data().len(), 1);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_replacement_snapshot_matches_engine_storage() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let loop_ = sess.create_loop().expect("loop");
        let midi = loop_
            .add_midi_channel(ChannelMode::Direct)
            .expect("MIDI channel");
        engine.pump();
        midi.load_all_midi_data(&[
            MidiEvent::new(1, vec![0x90, 60, 100]),
            MidiEvent::new(2, vec![0x80, 60, 0]),
        ])
        .expect("loaded data");
        engine.pump();

        let channel = engine
            .session_mut()
            .midi_channel_mut(0)
            .expect("engine channel");
        channel.set_recording_buffer(4);
        channel.set_playback_buffer(4);
        let mut output = Vec::new();
        channel
            .process(
                LoopMode::Replacing,
                LoopMode::Unknown,
                None,
                None,
                4,
                0,
                4,
                4,
                &[
                    engine::MidiStorageElem::new(0, &[0x90, 64, 100]).unwrap(),
                    engine::MidiStorageElem::new(3, &[0x80, 64, 0]).unwrap(),
                ],
                &mut output,
            )
            .unwrap();
        channel.set_recording_buffer(1);
        channel.set_playback_buffer(1);
        channel
            .process(
                LoopMode::Stopped,
                LoopMode::Unknown,
                None,
                None,
                1,
                0,
                1,
                4,
                &[],
                &mut output,
            )
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        let actual = loop {
            if let Ok(snapshot) = midi.try_get_current_data_snapshot() {
                break snapshot.contiguous();
            }
            assert!(
                Instant::now() < deadline,
                "MIDI snapshot publication timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0], MidiEvent::new(0, vec![0x90, 64, 100]));
        assert_eq!(actual[1], MidiEvent::new(3, vec![0x80, 64, 0]));
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_creation_cancels_on_drop_and_fails_with_its_parent() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let loop_ = sess.create_loop().expect("loop");
        let dropped = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("dropped channel");
        drop(dropped);

        let failed_parent_control = Arc::new(
            ObjectControl::<LoopId, engine::LoopStateMirror>::pending(sess.shared.session_id),
        );
        failed_parent_control.mark_failed("parent failed");
        let failed_parent = Loop {
            shared: Arc::clone(&sess.shared),
            control: failed_parent_control,
        };
        let failed_channel = failed_parent
            .add_midi_channel(ChannelMode::Direct)
            .expect("pending failed channel");

        engine.pump();
        assert_eq!(engine.session().n_loops(), 1);
        assert_eq!(engine.session().n_channels(), 0);
        assert_eq!(failed_channel.lifecycle(), ObjectLifecycle::Failed);
        assert!(failed_channel.get_state().is_err());
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_channel_commands_survive_a_saturated_queue() {
        let sess = BackendSession::create_with_capacity(1).expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let channel = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel after retry");
        channel.set_gain(0.5).expect("gain after retry");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        engine.pump();

        assert_eq!(channel.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(channel.get_state().expect("channel state").gain, 0.5);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_ports_accept_configuration_and_connections_before_readiness() {
        let sess = BackendSession::new().expect("session");
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let input =
            AudioPort::new_driver_port(&sess, &driver, "pending-in", &PortDirection::Input, 0)
                .expect("input");
        let output =
            AudioPort::new_driver_port(&sess, &driver, "pending-out", &PortDirection::Output, 0)
                .expect("output");
        assert_eq!(input.lifecycle(), ObjectLifecycle::Pending);
        assert_eq!(output.lifecycle(), ObjectLifecycle::Pending);
        input.set_gain(0.25).expect("gain");
        input.connect_internal(&output).expect("connection");

        engine.pump();
        assert_eq!(input.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(output.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(input.get_state().expect("state").gain, 0.25);
        assert_eq!(engine.session().n_ports(), 2);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn port_state_reads_and_dummy_dequeues_do_not_queue_queries() {
        let sess = BackendSession::new().expect("session");
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        let audio = AudioPort::new_driver_port(&sess, &driver, "audio", &PortDirection::Output, 0)
            .expect("audio");
        let midi = MidiPort::new_driver_port(&sess, &driver, "midi", &PortDirection::Output, 0)
            .expect("MIDI");
        let engine = sess.shared.take_engine().expect("parked engine");
        let commands_before = engine.stats().commands_applied.load(Ordering::Relaxed);
        for _ in 0..100 {
            let _ = audio.get_state();
            let _ = audio.poll_state();
            let _ = audio.dummy_dequeue_data(4);
            let _ = midi.get_state();
            let _ = midi.poll_state();
            let _ = midi.dummy_dequeue_data();
        }
        assert_eq!(
            engine.stats().commands_applied.load(Ordering::Relaxed),
            commands_before
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connection_polling_uses_the_cache_without_engine_commands() {
        let sess = BackendSession::new().expect("session");
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        let audio = AudioPort::new_driver_port(
            &sess,
            &driver,
            "cached-connections",
            &PortDirection::Output,
            0,
        )
        .expect("port");
        let engine = sess.shared.take_engine().expect("parked engine");
        let commands_before = engine.stats().commands_applied.load(Ordering::Relaxed);
        let started = Instant::now();
        for _ in 0..1_000 {
            let _ = audio.get_connections_state();
        }
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(
            engine.stats().commands_applied.load(Ordering::Relaxed),
            commands_before
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn authoritative_connection_state_bypasses_and_replaces_stale_cache() {
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        driver
            .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name: "authoritative-connections".to_string(),
                sample_rate: 48_000,
                buffer_size: 128,
            }))
            .expect("start driver");
        driver.dummy_add_external_mock_port(
            "system:playback",
            PortDirection::Input as u32,
            PortDataType::Audio as u32,
        );
        let sess = BackendSession::new().expect("session");
        sess.set_audio_driver(&driver).expect("attach driver");
        let port = AudioPort::new_driver_port(
            &sess,
            &driver,
            "authoritative-output",
            &PortDirection::Output,
            0,
        )
        .expect("port");
        sess.wait_for_command(port.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT)
            .expect("port creation");
        port.connect_external_port("system:playback");
        port.shared.set_cached_connection(
            &port.name,
            port.direction,
            PortDataType::Audio,
            "system:playback",
            false,
        );

        assert_eq!(
            port.get_connections_state().get("system:playback"),
            Some(&false)
        );
        assert_eq!(
            port.get_connections_state_now().get("system:playback"),
            Some(&true)
        );
        assert_eq!(
            port.get_connections_state().get("system:playback"),
            Some(&true)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn many_pending_objects_and_repeated_handle_clones_resolve_without_aliasing() {
        let sess = BackendSession::new().expect("session");
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let mut loops = Vec::new();
        let mut channels = Vec::new();
        let mut ports = Vec::new();
        for index in 0..32u32 {
            let loop_ = sess.create_loop().expect("loop");
            let channel = loop_
                .add_audio_channel(ChannelMode::Direct)
                .expect("channel");
            let port = AudioPort::new_driver_port(
                &sess,
                &driver,
                &format!("stress-{index}"),
                &PortDirection::Output,
                0,
            )
            .expect("port");
            channel.connect_output(&port).expect("connection");
            for _ in 0..16 {
                drop(loop_.clone());
                drop(channel.clone());
                drop(port.clone());
            }
            loops.push(loop_);
            channels.push(channel);
            ports.push(port);
        }
        engine.pump();
        assert_eq!(engine.session().n_loops(), 32);
        assert_eq!(engine.session().n_channels(), 32);
        assert_eq!(engine.session().n_ports(), 32);
        assert!(loops
            .iter()
            .all(|object| object.lifecycle() == ObjectLifecycle::Ready));
        assert!(channels
            .iter()
            .all(|object| object.lifecycle() == ObjectLifecycle::Ready));
        assert!(ports
            .iter()
            .all(|object| object.lifecycle() == ObjectLifecycle::Ready));
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn concurrent_producers_and_shutdown_with_pending_work_are_safe() {
        let sess = BackendSession::create_with_capacity(8).expect("session");
        let loop_ = sess.create_loop().expect("loop");
        sess.wait_for_command(loop_.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT)
            .expect("loop creation");

        let mut workers = Vec::new();
        for producer in 0..8u32 {
            let loop_ = loop_.clone();
            workers.push(thread::spawn(move || {
                let mut last = CommandSequence::NONE;
                for value in 0..100u32 {
                    last = loop_
                        .set_length(producer * 100 + value)
                        .expect("concurrent command");
                }
                last
            }));
        }
        let last: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().expect("producer"))
            .collect();
        for sequence in last {
            sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
                .expect("concurrent command fence");
        }

        let doomed = BackendSession::new().expect("doomed session");
        let engine = doomed.shared.take_engine().expect("parked engine");
        let pending = doomed.create_loop().expect("pending loop");
        assert_eq!(pending.lifecycle(), ObjectLifecycle::Pending);
        drop(engine);
        assert_eq!(pending.lifecycle(), ObjectLifecycle::Failed);
        assert!(pending.creation_error().is_some());
        assert!(pending
            .set_length(1)
            .expect_err("disconnected command")
            .to_string()
            .contains("engine is gone"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn exact_and_stale_channel_snapshots_report_pending_and_recording_states() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let loop_ = sess.create_loop().expect("loop");
        let audio = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("audio channel");
        let midi = loop_
            .add_midi_channel(ChannelMode::Direct)
            .expect("MIDI channel");
        engine.pump();

        audio.load_data(&[1.0, 2.0]).expect("queue audio data");
        midi.load_all_midi_data(&[MidiEvent {
            time: 1,
            data: vec![0x90, 60, 100],
        }])
        .expect("queue MIDI data");
        assert!(matches!(
            audio.try_get_current_data_snapshot(),
            Err(engine::content_snapshot::CurrentDataError::MutationActive(
                engine::content_snapshot::ContentMutation::Loading
            ))
        ));
        assert!(matches!(
            midi.try_get_current_data_snapshot(),
            Err(engine::content_snapshot::CurrentDataError::MutationActive(
                engine::content_snapshot::ContentMutation::Loading
            ))
        ));
        assert!(audio
            .get_latest_data_snapshot()
            .snapshot
            .contiguous()
            .is_empty());
        assert!(midi
            .get_latest_data_snapshot()
            .snapshot
            .contiguous()
            .is_empty());

        engine.pump();
        let start = Instant::now();
        while audio.try_get_current_data_snapshot().is_err()
            || midi.try_get_current_data_snapshot().is_err()
        {
            assert!(start.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }
        assert_eq!(
            audio
                .try_get_current_data_snapshot()
                .expect("current audio")
                .contiguous(),
            vec![1.0, 2.0]
        );
        assert_eq!(
            midi.try_get_current_data_snapshot()
                .expect("current MIDI")
                .events()
                .count(),
            1
        );

        for mutation in [
            engine::content_snapshot::ContentMutation::Recording,
            engine::content_snapshot::ContentMutation::PreRecording,
            engine::content_snapshot::ContentMutation::Replacing,
            engine::content_snapshot::ContentMutation::Loading,
            engine::content_snapshot::ContentMutation::Clearing,
            engine::content_snapshot::ContentMutation::RingbufferAdoption,
        ] {
            assert!(audio.snapshot_control.begin_mutation(mutation));
            assert!(matches!(
                audio.try_get_current_data_snapshot(),
                Err(engine::content_snapshot::CurrentDataError::MutationActive(found))
                    if found == mutation
            ));
            assert_eq!(
                audio.get_latest_data_snapshot().snapshot.contiguous(),
                vec![1.0, 2.0]
            );
            audio.snapshot_control.cancel_mutation();

            assert!(midi.snapshot_control.begin_mutation(mutation));
            assert!(matches!(
                midi.try_get_current_data_snapshot(),
                Err(engine::content_snapshot::CurrentDataError::MutationActive(found))
                    if found == mutation
            ));
            assert_eq!(midi.get_latest_data_snapshot().snapshot.events().count(), 1);
            midi.snapshot_control.cancel_mutation();
        }

        loop_
            .transition(LoopMode::Recording, -1, -1)
            .expect("queue recording");
        engine.pump();
        engine
            .session_mut()
            .apply_graph_changes()
            .expect("apply graph");
        engine.process(4);
        assert!(matches!(
            audio.try_get_current_data_snapshot(),
            Err(engine::content_snapshot::CurrentDataError::MutationActive(
                engine::content_snapshot::ContentMutation::Recording
            ))
        ));
        assert_eq!(
            audio.get_latest_data_snapshot().snapshot.contiguous(),
            vec![1.0, 2.0]
        );
        assert!(matches!(
            midi.try_get_current_data_snapshot(),
            Err(engine::content_snapshot::CurrentDataError::MutationActive(
                engine::content_snapshot::ContentMutation::Recording
            ))
        ));
        assert_eq!(
            midi.get_latest_data_snapshot().snapshot.contiguous().len(),
            1
        );

        // Mutations that would overlap recording are rejected without touching content.
        assert!(matches!(audio.load_data(&[9.0]), Err(SendError::Full)));
        assert!(matches!(audio.clear(0), Err(SendError::Full)));
        assert!(matches!(midi.load_all_midi_data(&[]), Err(SendError::Full)));
        assert!(matches!(midi.clear(), Err(SendError::Full)));

        loop_
            .transition(LoopMode::Stopped, -1, -1)
            .expect("queue stop");
        engine.pump();
        engine.process(1);
        let start = Instant::now();
        while audio.try_get_current_data_snapshot().is_err()
            || midi.try_get_current_data_snapshot().is_err()
        {
            assert!(start.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_data_dirty_is_acknowledged_on_the_frontend() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let audio = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("audio channel");
        let sequence = audio.load_data(&[1.0, 2.0]).expect("queue data");
        sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("data command");
        let start = Instant::now();
        while audio.try_get_current_data_snapshot().is_err() {
            assert!(start.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }

        assert!(audio.get_state().expect("dirty state").data_dirty);
        audio.clear_data_dirty();
        assert!(!audio.get_state().expect("acknowledged state").data_dirty);
        let sequence = audio.clear(0).expect("queue clear");
        sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("clear command");
        let start = Instant::now();
        while audio.try_get_current_data_snapshot().is_err() {
            assert!(start.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }
        assert!(audio.get_state().expect("dirty again").data_dirty);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_state_reads_do_not_queue_engine_queries() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let channel = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel");
        let engine = sess.shared.take_engine().expect("parked engine");
        let commands_before = engine.stats().commands_applied.load(Ordering::Relaxed);
        for _ in 0..100 {
            assert!(channel.get_state().is_ok());
            assert!(channel.poll_state().is_some());
            let _ = channel.get_data();
        }
        assert_eq!(
            engine.stats().commands_applied.load(Ordering::Relaxed),
            commands_before
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_reads_return_the_immediate_desired_mirror_without_queueing_a_query() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let commands_before = engine.stats().commands_applied.load(Ordering::Relaxed);

        loop_.set_length(32).expect("queue length");
        assert_eq!(loop_.get_state().expect("desired state").length, 32);
        for _ in 0..100 {
            assert_eq!(loop_.poll_state().expect("poll").length, 32);
            assert_eq!(loop_.get_state().expect("state").length, 32);
        }
        assert_eq!(
            engine.stats().commands_applied.load(Ordering::Relaxed),
            commands_before
        );

        engine.pump();
        assert_eq!(loop_.get_state().expect("updated state").length, 32);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_loop_relationships_resolve_in_fifo_order() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let source = sess.create_loop().expect("source");
        let follower = sess.create_loop().expect("follower");
        source.set_length(64).expect("queue source length");
        follower
            .set_sync_source(Some(&source))
            .expect("queue sync source");
        transition_multiple_loops(&[&source, &follower], LoopMode::Playing, -1, -1)
            .expect("queue transitions");

        engine.pump();
        assert_eq!(source.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(follower.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(source.get_state().expect("source state").length, 64);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn primitive_topology_survives_dropped_ready_controls() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let source = sess.create_loop().expect("source");
        let follower = sess.create_loop().expect("follower");
        follower
            .set_sync_source(Some(&source))
            .expect("queue sync source");

        // The relationship command keeps both pending controls alive just long enough to create
        // them, but topology caching must not depend on either frontend control surviving.
        drop(source);
        drop(follower);
        engine.pump();
        assert_eq!(
            sess.primitive_sync_sources_if_ready()
                .expect("topology after control drops")[1],
            Some(0)
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_relationships_reject_cross_session_handles() {
        let first = BackendSession::new().expect("first session");
        let second = BackendSession::new().expect("second session");
        let first_loop = first.create_loop().expect("first loop");
        let second_loop = second.create_loop().expect("second loop");

        assert!(first_loop.set_sync_source(Some(&second_loop)).is_err());
        assert!(
            transition_multiple_loops(&[&first_loop, &second_loop], LoopMode::Playing, -1, -1,)
                .is_err()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dropping_a_pending_loop_cancels_creation() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let loop_ = sess.create_loop().expect("pending loop");
        assert_eq!(loop_.lifecycle(), ObjectLifecycle::Pending);
        drop(loop_);

        engine.pump();
        assert_eq!(engine.session().n_loops(), 0);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_composite_retains_desired_play_after_record() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let composite = sess.create_composite_loop().expect("pending composite");
        assert_eq!(composite.lifecycle(), ObjectLifecycle::Pending);

        let option_sequence = composite
            .set_play_after_record(true)
            .expect("queue pending option");
        assert!(option_sequence > composite.creation_sequence());
        assert!(composite.control.mirror.read().play_after_record);
        assert!(composite.desired_play_after_record.load(Ordering::Acquire));

        engine.pump();
        assert_eq!(composite.lifecycle(), ObjectLifecycle::Ready);
        assert!(composite.control.mirror.read().play_after_record);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dropping_a_pending_composite_releases_its_control() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let composite = sess.create_composite_loop().expect("pending composite");
        assert_eq!(composite.lifecycle(), ObjectLifecycle::Pending);
        let weak = Arc::downgrade(&composite.control);
        drop(composite);

        engine.pump();
        assert!(weak.upgrade().is_none());
        assert!(engine.session().composite_timeline().is_empty());
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn failed_and_closed_loop_controls_ignore_commands() {
        let sess = BackendSession::new().expect("session");
        let failed_control = Arc::new(ObjectControl::<LoopId, engine::LoopStateMirror>::pending(
            sess.shared.session_id,
        ));
        failed_control.mark_failed("creation failed");
        let failed = Loop {
            shared: Arc::clone(&sess.shared),
            control: failed_control,
        };
        let closed_control = Arc::new(ObjectControl::<LoopId, engine::LoopStateMirror>::pending(
            sess.shared.session_id,
        ));
        closed_control.mark_ready(LoopId(0));
        closed_control.mark_closed();
        let closed = Loop {
            shared: Arc::clone(&sess.shared),
            control: closed_control,
        };

        assert_eq!(failed.lifecycle(), ObjectLifecycle::Failed);
        assert!(failed
            .get_state()
            .expect_err("failed state")
            .to_string()
            .contains("creation failed"));
        assert_eq!(closed.lifecycle(), ObjectLifecycle::Closed);
        assert!(closed.get_state().is_err());
        failed.set_length(1).expect("queue failed-handle command");
        closed.set_length(2).expect("queue closed-handle command");
        assert_eq!(
            sess.shared
                .parked
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
                .expect("parked engine")
                .session()
                .n_loops(),
            0
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn object_controls_publish_failed_and_closed_without_aliasing_an_index() {
        let failed = ObjectControl::<LoopId, engine::LoopStateMirror>::pending(1);
        failed.mark_failed("creation failed");
        assert_eq!(failed.lifecycle(), ObjectLifecycle::Failed);
        assert!(failed.ready_id().is_none());
        assert_eq!(failed.error().as_deref(), Some("creation failed"));

        let ready = ObjectControl::<LoopId, engine::LoopStateMirror>::pending(1);
        ready.mark_ready(LoopId(0));
        assert_eq!(ready.ready_id(), Some(LoopId(0)));
        ready.mark_closed();
        assert_eq!(ready.lifecycle(), ObjectLifecycle::Closed);
        assert!(ready.ready_id().is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn real_jack_is_not_advanced_by_state_polling() {
        assert!(!driver_uses_dummy_processing(AudioDriverType::Jack));
        assert!(driver_uses_dummy_processing(AudioDriverType::JackTest));
        assert!(driver_uses_dummy_processing(AudioDriverType::Dummy));
        assert!(!driver_uses_dummy_processing(AudioDriverType::Cpal));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_virtual_audio_input_routes_capture_channel_into_session_port() {
        let mut s = engine::Session::default();
        let input = s.add_port(engine::session::Port::External(
            engine::external_audio_port::ExternalAudioPort::new(
                "input",
                engine::PortDirection::Input,
                0,
            ),
        ));
        let l = s.create_loop();
        let c = s
            .add_audio_channel(l, 64, engine::ChannelMode::Direct)
            .expect("channel");
        s.connect_channel_input(c, input).expect("connect");
        s.apply_graph_changes().expect("graph");
        s.set_loop_mode(l, engine::LoopMode::Recording)
            .expect("mode");

        let capture_names = vec![
            "cpal:test:capture_1".to_string(),
            "cpal:test:capture_2".to_string(),
        ];
        let connections = vec![(compat_port_id(input), capture_names[1].clone())];
        let interleaved = [10.0, 20.0, 11.0, 21.0, 12.0, 22.0, 13.0, 23.0];
        stage_virtual_audio_inputs(&mut s, &connections, &capture_names, 2, &interleaved, 4);
        s.process(4);

        let data = s
            .loop_(l)
            .and_then(|l| l.audio_channel(0))
            .map(|c| c.data().to_vec())
            .expect("recorded channel");
        assert_eq!(&data[..4], &[20.0, 21.0, 22.0, 23.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_virtual_audio_output_routes_session_port_to_playback_channel() {
        let mut s = engine::Session::default();
        let output = s.add_port(engine::session::Port::External(
            engine::external_audio_port::ExternalAudioPort::new(
                "output",
                engine::PortDirection::Output,
                0,
            ),
        ));
        let l = s.create_loop();
        let c = s
            .add_audio_channel(l, 64, engine::ChannelMode::Direct)
            .expect("channel");
        s.connect_channel_output(c, output).expect("connect");
        s.loop_mut(l)
            .expect("loop")
            .audio_channel_mut(0)
            .expect("channel")
            .load_data(&[1.0, 2.0, 3.0, 4.0]);
        s.loop_mut(l).expect("loop").set_length(4);
        s.apply_graph_changes().expect("graph");
        s.set_loop_mode(l, engine::LoopMode::Playing).expect("mode");
        s.process(4);

        let playback_names = vec![
            "cpal:test:playback_1".to_string(),
            "cpal:test:playback_2".to_string(),
        ];
        let connections = vec![(compat_port_id(output), playback_names[1].clone())];
        let mut interleaved = [0.0f32; 8];
        collect_virtual_audio_outputs(&s, &connections, &playback_names, 2, &mut interleaved, 4);

        assert_eq!(interleaved, [0.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_virtual_midi_input_fans_out_to_session_and_decoupled_ports() {
        let mut s = engine::Session::default();
        let input = s.add_port(engine::session::Port::ExternalMidi(
            engine::external_midi_port::ExternalMidiPort::new("min", engine::PortDirection::Input),
        ));
        let decoupled_id = engine::PortId(100_123);
        let decoupled_queue = Arc::new(Mutex::new(Vec::new()));
        let decoupled = vec![CpalDecoupledMidiPort {
            port_id: decoupled_id,
            direction: PortDirection::Input,
            queue: decoupled_queue.clone(),
        }];
        let input_name = "midir:test:output".to_string();
        let connections = vec![
            (compat_port_id(input), input_name.clone()),
            (decoupled_id, input_name.clone()),
        ];
        let events = vec![
            engine::midi_storage::MidiStorageElem::new(0, &[0x90, 60, 100]).unwrap(),
            engine::midi_storage::MidiStorageElem::new(0, &[0x80, 60, 0]).unwrap(),
        ];

        route_virtual_midi_inputs(&mut s, &connections, &input_name, &events, &decoupled);

        let port = s
            .port_mut(input)
            .and_then(|p| p.as_external_midi_mut())
            .unwrap();
        port.prepare(64);
        port.process(64);
        assert_eq!(port.visible_events().len(), 2);
        let queue = decoupled_queue.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].data, vec![0x90, 60, 100]);
        assert_eq!(queue[1].data, vec![0x80, 60, 0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_virtual_midi_output_drains_decoupled_queue_for_connected_output() {
        let output_name = "midir:test:input".to_string();
        let decoupled_id = engine::PortId(100_456);
        let queue = Arc::new(Mutex::new(vec![
            MidiEvent::new(0, vec![0x90, 64, 127]),
            MidiEvent::new(2, vec![0x80, 64, 0]),
        ]));
        let decoupled = vec![CpalDecoupledMidiPort {
            port_id: decoupled_id,
            direction: PortDirection::Output,
            queue: queue.clone(),
        }];
        let connections = vec![(decoupled_id, output_name.clone())];

        let events = drain_decoupled_midi_output_events(&connections, &output_name, &decoupled);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data(), &[0x90, 64, 127]);
        assert_eq!(events[1].data(), &[0x80, 64, 0]);
        assert!(queue.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_test_backend_publishes_mock_virtual_audio_ports() {
        let driver = AudioDriver::new(AudioDriverType::CpalTest, None).expect("driver");
        let settings = AudioDriverSettings::Cpal(CpalMidiAudioDriverSettings {
            client_name: "shoop-cpal-test".to_string(),
            host: "default".to_string(),
            output_device: "default".to_string(),
            input_device: "none".to_string(),
            sample_rate: 0,
            buffer_size: 0,
            input_channels: "all".to_string(),
            output_channels: "all".to_string(),
            capture_ring_frames: 256,
            midi_inputs: vec!["none".to_string()],
            midi_outputs: vec!["none".to_string()],
        });
        driver.start(&settings).expect("settings accepted");
        let sess = BackendSession::new().expect("session");
        sess.set_audio_driver(&driver)
            .expect("mock driver activation");

        let playback_ports = driver.find_external_ports(
            None,
            PortDirection::Input as u32,
            PortDataType::Audio as u32,
        );
        assert!(
            playback_ports
                .iter()
                .any(|p| p.name.starts_with("cpal:") && p.name.contains(":playback_")),
            "no virtual CPAL playback ports: {playback_ports:?}"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cpal_backend_exposes_virtual_audio_ports_through_app_api_when_device_available() {
        if std::env::var_os("SHOOP_RUN_REAL_AUDIO_SMOKE").is_none() {
            eprintln!("skipping optional real CPAL smoke; set SHOOP_RUN_REAL_AUDIO_SMOKE=1");
            return;
        }
        let driver = AudioDriver::new(AudioDriverType::Cpal, None).expect("driver");
        let settings = AudioDriverSettings::Cpal(CpalMidiAudioDriverSettings {
            client_name: "shoop-cpal-test".to_string(),
            host: "default".to_string(),
            output_device: "default".to_string(),
            input_device: "none".to_string(),
            sample_rate: 0,
            buffer_size: 0,
            input_channels: "all".to_string(),
            output_channels: "all".to_string(),
            capture_ring_frames: 256,
            midi_inputs: vec!["none".to_string()],
            midi_outputs: vec!["none".to_string()],
        });
        driver.start(&settings).expect("settings accepted");
        let sess = BackendSession::new().expect("session");
        if let Err(e) = sess.set_audio_driver(&driver) {
            // Once this optional real smoke is requested, failure is explicit unless the
            // caller also opts into environment-aware backend skips.
            assert!(
                std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some(),
                "a CPAL output device is required by this test but unavailable: {e}.\n\
                 Set SHOOP_ALLOW_MISSING_BACKENDS=1 to skip backend-dependent tests."
            );
            eprintln!("skipping: no usable CPAL output device ({e})");
            return;
        }

        let playback_ports = driver.find_external_ports(
            None,
            PortDirection::Input as u32,
            PortDataType::Audio as u32,
        );
        assert!(
            playback_ports
                .iter()
                .any(|p| p.name.starts_with("cpal:") && p.name.contains(":playback_")),
            "no virtual CPAL playback ports: {playback_ports:?}"
        );

        let app_port =
            AudioPort::new_driver_port(&sess, &driver, "app_audio_out", &PortDirection::Output, 0)
                .expect("app port");
        sess.wait_for_command(app_port.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT)
            .expect("app port creation");
        let target = playback_ports[0].name.clone();
        let wait_for_connection = |expected: bool| {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                let state = app_port.get_connections_state();
                if state.get(&target) == Some(&expected) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "connection cache did not refresh"
                );
                thread::sleep(Duration::from_millis(10));
            }
        };
        wait_for_connection(false);
        app_port.connect_external_port(&target);
        wait_for_connection(true);
        app_port.disconnect_external_port(&target);
        wait_for_connection(false);

        let state = driver.get_state();
        assert_eq!(state.active, 1);
        assert!(state.sample_rate > 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fx_port_getters_return_stable_pending_handles_without_duplicate_topology() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "stable-fx", 0)
            .expect("chain");
        let first = chain.get_audio_input_port(0).expect("first handle");
        let again = chain.get_audio_input_port(0).expect("same handle");
        assert!(Arc::ptr_eq(&first.control, &again.control));
        assert_eq!(first.lifecycle(), ObjectLifecycle::Pending);
        first.set_gain(0.5).expect("pending gain");

        engine.pump();
        assert_eq!(first.lifecycle(), ObjectLifecycle::Ready);
        assert_eq!(first.get_state().expect("state").gain, 0.5);
        assert_eq!(engine.session().n_ports(), 5);
        let commands = engine.stats().commands_applied.load(Ordering::Relaxed);
        for _ in 0..100 {
            let handle = chain.get_audio_input_port(0).expect("stable getter");
            assert!(Arc::ptr_eq(&first.control, &handle.control));
        }
        assert_eq!(engine.session().n_ports(), 5);
        assert_eq!(
            engine.stats().commands_applied.load(Ordering::Relaxed),
            commands
        );
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fx_output_capture_uses_bounded_chunks() {
        const CAPTURE_SAMPLES: u32 = 480_000;

        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "capturing-fx", CAPTURE_SAMPLES)
            .expect("chain");
        engine.pump();
        let input = chain
            .get_audio_input_port(0)
            .unwrap()
            .control
            .ready_id()
            .unwrap()
            .index();
        let output = chain
            .get_audio_output_port(0)
            .unwrap()
            .control
            .ready_id()
            .unwrap()
            .index();
        let input = engine.session().port(input).unwrap().audio().unwrap();
        let output = engine.session().port(output).unwrap().audio().unwrap();
        let chunk_size = output.ringbuffer_contents().buffer_size;

        assert_eq!(input.ringbuffer_capacity(), 0);
        assert!(chunk_size >= (CAPTURE_SAMPLES as usize).div_ceil(32));
        assert!(output.ringbuffer_capacity() >= CAPTURE_SAMPLES as usize);
        assert!(output.ringbuffer_capacity() < CAPTURE_SAMPLES as usize + chunk_size);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn test_fx_chain_runs_as_a_scheduled_processor_node() {
        let sess = BackendSession::new().expect("session");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "scheduled-fx", 0)
            .expect("chain");
        chain.set_active(true);
        let mut engine = sess.shared.take_engine().expect("parked engine");
        engine.pump();
        let input = chain
            .get_audio_input_port(0)
            .unwrap()
            .control
            .ready_id()
            .unwrap()
            .index();
        let output = chain
            .get_audio_output_port(0)
            .unwrap()
            .control
            .ready_id()
            .unwrap()
            .index();
        let source = engine.session_mut().add_port(engine::session::Port::Dummy(
            engine::DummyAudioPort::new(
                engine::PortId(800),
                "source",
                engine::PortDirection::Input,
                4,
            ),
        ));
        let sink = engine.session_mut().add_port(engine::session::Port::Dummy(
            engine::DummyAudioPort::new(
                engine::PortId(801),
                "sink",
                engine::PortDirection::Output,
                4,
            ),
        ));
        engine
            .session_mut()
            .connect_ports_internal(source, input)
            .unwrap();
        engine
            .session_mut()
            .connect_ports_internal(output, sink)
            .unwrap();
        engine.session_mut().apply_graph_changes().unwrap();
        engine
            .session_mut()
            .port_mut(source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[2.0, 4.0, 6.0, 8.0]);
        engine
            .session_mut()
            .port_mut(sink)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        engine.session_mut().process(4);
        let output = engine
            .session_mut()
            .port_mut(sink)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0]);
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn internal_fx_midi_capture_observes_routed_host_input() {
        let sess = BackendSession::new().expect("session");
        let mut engine = sess.shared.take_engine().expect("parked engine");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "captured-fx", 0)
            .expect("chain");
        let midi_input = chain.get_midi_input_port(0).expect("MIDI input");
        engine.pump();
        let target = midi_input
            .control
            .ready_id()
            .expect("ready FX MIDI port")
            .index();
        let source = engine
            .session_mut()
            .add_port(engine::session::Port::ExternalMidi(
                engine::external_midi_port::ExternalMidiPort::new(
                    "source",
                    engine::PortDirection::Input,
                ),
            ));
        engine
            .session_mut()
            .connect_ports_internal(source, target)
            .unwrap();
        engine.session_mut().apply_graph_changes().unwrap();
        engine
            .session_mut()
            .port_mut(source)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .push_incoming(0, &[0x90, 72, 100]);
        engine
            .session_mut()
            .port_mut(target)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .request_output();
        engine.session_mut().process(1);

        assert_eq!(
            midi_input.dummy_dequeue_data(),
            vec![MidiEvent::new(0, vec![0x90, 72, 100])]
        );

        engine
            .session_mut()
            .port_mut(source)
            .unwrap()
            .midi_mut()
            .unwrap()
            .set_passthrough_muted(true);
        engine
            .session_mut()
            .port_mut(source)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .push_incoming(0, &[0x90, 73, 100]);
        engine
            .session_mut()
            .port_mut(target)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .request_output();
        engine.session_mut().process(1);
        assert_eq!(
            midi_input.dummy_dequeue_data(),
            vec![MidiEvent::new(0, vec![0x80, 72, 0])]
        );

        engine
            .session_mut()
            .port_mut(source)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .push_incoming(0, &[0x90, 74, 100]);
        engine
            .session_mut()
            .port_mut(target)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .request_output();
        engine.session_mut().process(1);
        assert!(midi_input.dummy_dequeue_data().is_empty());
        sess.shared.return_engine(engine);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn current_fx_chain_handle_controls_visibility_activity_and_ports() {
        let sess = BackendSession::new().expect("session");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "test_fx", 0)
            .expect("fx chain");

        assert!(chain.available());
        assert_eq!(
            chain.get_state().expect("state"),
            FXChainState {
                ready: 1,
                active: 0,
                visible: 0,
            }
        );

        chain.set_visible(true);
        chain.set_active(true);
        assert_eq!(
            chain.get_state().expect("state"),
            FXChainState {
                ready: 1,
                active: 1,
                visible: 1,
            }
        );
        assert!(chain.get_state_str().expect("state string").is_empty());
        chain.restore_state("");

        let audio_in = chain.get_audio_input_port(0).expect("audio input port");
        let audio_out = chain.get_audio_output_port(0).expect("audio output port");
        let midi_in = chain.get_midi_input_port(0).expect("midi input port");
        assert!(chain.get_midi_output_port(0).is_none());
        assert_eq!(audio_in.direction(), PortDirection::Output);
        assert_eq!(audio_out.direction(), PortDirection::Input);
        assert_eq!(midi_in.direction(), PortDirection::Output);
        // Creating the chain's ports leaves a reschedule pending rather than applying it
        // inline, so settle it first -- as `wait_process` does for real callers.
        sess.shared.flush_graph_changes();
        assert!(graph_up_to_date(&sess));
    }

    #[cfg(feature = "carla")]
    #[shoop_wasm_test_support::shoop_test]
    fn carla_fx_chain_handle_instantiates_when_plugin_is_available() {
        let _exclusive = engine::carla_native::lock_carla_test();
        let sess = BackendSession::new().expect("session");
        let chain = sess
            .create_fx_chain(FXChainType::CarlaRack, "carla", 0)
            .expect("chain handle");
        if !chain.available() {
            eprintln!(
                "skipping app-backend Carla availability assertion: {}",
                chain.get_state_str().unwrap_or_default()
            );
            return;
        }
        assert_eq!(
            chain.get_state().expect("state"),
            FXChainState {
                ready: 1,
                active: 0,
                visible: 0,
            }
        );
        chain.set_active(true);
        assert_eq!(chain.get_state().expect("state").active, 1);
        let state = chain.get_state_str().expect("state string");
        assert!(
            state.starts_with("shoop-carla-native-state:2:rack:"),
            "Carla state should use the native envelope: {state}"
        );
        chain.restore_state(&state);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn audio_port_peak_state_is_per_poll_cycle() {
        const BUFFER: u32 = 4;

        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        driver
            .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name: "peak-poll-test".to_string(),
                sample_rate: 48_000,
                buffer_size: BUFFER,
            }))
            .expect("start driver");
        let sess = BackendSession::new().expect("session");
        sess.set_audio_driver(&driver).expect("attach driver");
        driver.dummy_enter_controlled_mode();

        let port = AudioPort::new_driver_port(&sess, &driver, "input", &PortDirection::Input, 0)
            .expect("port");
        sess.wait_for_command(port.creation_sequence(), engine::DEFAULT_WAIT_TIMEOUT)
            .expect("port creation");
        sess.shared.flush_graph_changes();
        driver.wait_process();
        let initial = match port.poll_state() {
            Some(state) => state,
            None => port.get_state().expect("initial state"),
        };
        assert_eq!(initial.input_peak, 0.0);
        assert_eq!(initial.output_peak, 0.0);

        let sequence = port
            .dummy_queue_data(&[0.0, -0.8, 0.2, 0.1])
            .expect("queue first input");
        sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("first input command");
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let first = port.poll_state().expect("first state");
        assert_eq!(first.input_peak, 0.8);
        assert_eq!(first.output_peak, 0.8);

        let sequence = port
            .dummy_queue_data(&[0.0, -0.3, 0.1, 0.2])
            .expect("queue second input");
        sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("second input command");
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let second = port.poll_state().expect("second state");
        assert_eq!(second.input_peak, 0.3);
        assert_eq!(second.output_peak, 0.3);

        let sequence = port
            .dummy_queue_data(&[0.0, -0.1, 0.05, 0.0])
            .expect("queue third input");
        sess.wait_for_command(sequence, engine::DEFAULT_WAIT_TIMEOUT)
            .expect("third input command");
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let third = port.poll_state().expect("third state");
        assert_eq!(third.input_peak, 0.1);
        assert_eq!(third.output_peak, 0.1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn current_audio_driver_handle_reports_dummy_lifecycle_state() {
        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        driver
            .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name: "api-test".to_string(),
                sample_rate: 48_000,
                buffer_size: 128,
            }))
            .expect("start driver");
        let sess = BackendSession::new().expect("session");
        sess.set_audio_driver(&driver).expect("attach driver");
        let state = driver.get_state();
        assert_eq!(state.active, 1);
        assert_eq!(state.sample_rate, 48_000);
        assert_eq!(state.buffer_size, 128);
        assert_eq!(state.maybe_instance_name, "api-test");
    }

    /// Controlled mode advances the session by exactly what was asked for, in
    /// buffer-sized cycles.
    ///
    /// This replaces `driver.rs`'s `a_request_is_split_into_buffer_sized_cycles` and
    /// `control_work_and_cycles_meet`, which asserted the same chunking against
    /// `DummyEngineDriver` -- a driver the application never ran. The request here is
    /// deliberately not a multiple of the buffer, so the final short cycle is exercised:
    /// 160 frames at a buffer of 64 is 64 + 64 + 32, and a driver that dropped the
    /// remainder or rounded up to a whole buffer would land on a different position.
    #[shoop_wasm_test_support::shoop_test]
    fn a_controlled_request_advances_the_session_by_exactly_that_many_frames() {
        const BUFFER: u32 = 64;
        const REQUEST: u32 = 160;

        let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
        driver
            .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name: "chunking-test".to_string(),
                sample_rate: 48_000,
                buffer_size: BUFFER,
            }))
            .expect("start driver");
        let sess = BackendSession::new().expect("session");
        sess.set_audio_driver(&driver).expect("attach driver");

        // Controlled before anything is requested, so no cycle can slip in ahead of the
        // setup and the position below is attributable to the request alone.
        driver.dummy_enter_controlled_mode();
        assert!(driver.dummy_is_controlled());
        assert_eq!(driver.dummy_n_requested_frames(), 0);

        let loop_ = sess.create_loop().expect("loop");
        loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel");
        // Longer than the request, so the loop cannot wrap and hide a miscount.
        loop_.set_length(REQUEST * 2).expect("length");
        loop_
            .transition(LoopMode::Playing, -1, -1)
            .expect("playing");
        driver.wait_process();
        assert_eq!(loop_.get_state().expect("state").position, 0);

        driver.dummy_request_controlled_frames(REQUEST);
        assert_eq!(driver.dummy_n_requested_frames(), REQUEST);
        driver.dummy_run_requested_frames();

        assert_eq!(driver.dummy_n_requested_frames(), 0);
        assert_eq!(loop_.get_state().expect("state").position, REQUEST);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dummy_iteration_uses_only_explicit_realtime_lock_permissions() {
        struct DisableGuard;
        impl Drop for DisableGuard {
            fn drop(&mut self) {
                crate::realtime_lock_guard::set_enabled(false);
            }
        }

        let mut dummy = engine::DummyDriver::default();
        dummy.start(engine::DriverSettings {
            sample_rate: 48_000,
            buffer_size: 64,
            client_name: "lock-guard-test".to_string(),
        });
        let inner = Arc::new(Mutex::new(DriverInner {
            driver_type: AudioDriverType::Dummy,
            dummy,
            last_processed: 0,
            process_generation: 0,
            finish: Arc::new(AtomicBool::new(false)),
            dummy_thread: None,
            session: None,
            jack: None,
            cpal: None,
            cpal_settings: None,
            cpal_decoupled_midi_ports: Arc::new(Mutex::new(Vec::new())),
            maybe_process_callback: None,
        }));
        let mut session = engine::Session::default();
        session.apply_graph_changes().unwrap();
        let (engine, _handle) = engine::split(session, 4);
        let mut engine = Some(engine);

        crate::realtime_lock_guard::set_enabled(true);
        let _disable = DisableGuard;
        process_dummy_driver_iteration(&inner, &mut engine);
        crate::realtime_lock_guard::set_enabled(false);

        let state = inner.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(state.last_processed, 64);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn get_state_does_not_advance_dummy_time() {
        // Built by hand, with no dummy thread, so nothing but `get_state` can advance it.
        let mut dummy = engine::DummyDriver::default();
        dummy.start(engine::DriverSettings {
            sample_rate: 48_000,
            buffer_size: 256,
            client_name: "test".to_string(),
        });
        let driver = AudioDriver {
            inner: Arc::new(Mutex::new(DriverInner {
                driver_type: AudioDriverType::Dummy,
                dummy,
                last_processed: 0,
                process_generation: 0,
                finish: Arc::new(AtomicBool::new(false)),
                dummy_thread: None,
                session: None,
                jack: None,
                cpal: None,
                cpal_settings: None,
                cpal_decoupled_midi_ports: Arc::new(Mutex::new(Vec::new())),
                maybe_process_callback: None,
            })),
        };

        let state = driver.get_state();
        assert_eq!(state.last_processed, 0);
        let inner = driver.inner.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(inner.last_processed, 0);
        assert_eq!(inner.process_generation, 0);
    }
}
