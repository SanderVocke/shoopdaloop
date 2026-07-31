//! Application-facing backend handles used by the QML/frontend layer.
//!
//! This module is the compatibility boundary between the CXX-Qt frontend objects and
//! the Rust engine.  It owns driver/session handles, port/channel/loop handles and
//! the small amount of JACK/CPAL/midir routing glue the GUI expects, while all actual
//! loop, graph, port, MIDI and FX processing stays in the core engine modules.

#![allow(dead_code)]

use crate as engine;
use crate::graph_scheduler::{GraphScheduler, DEFAULT_WINDOW};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
pub use engine::{
    cpal_host_names, cpal_input_device_names, cpal_input_device_names_for_host,
    cpal_output_device_names, cpal_output_device_names_for_host, driver_type_supported,
    midir_input_port_names, midir_output_port_names, AudioDriverType, ChannelMode, FXChainType,
    LoopMode, MidiEvent, MultichannelAudio, PortConnectabilityKind, PortDataType, PortDirection,
    ProfilingReport, ProfilingReportItem,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

pub type PortConnectability = engine::PortConnectability;

/// How many control operations may be outstanding between cycles.
///
/// Sized for a burst, not for the steady state: loading a session queues a mutation per port,
/// loop, channel and connection with no cycle in between, and the queue refuses rather than
/// growing when it is full. A parked engine drains after every send, so this bound only
/// really applies once a driver is running.
const COMMAND_QUEUE_CAPACITY: usize = 4096;

#[derive(Debug, Clone)]
pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

pub type BackendSessionState = engine::BackendSessionState;

enum JackRegisteredPort {
    AudioIn {
        session_idx: usize,
        jack: jack::Port<jack::AudioIn>,
    },
    AudioOut {
        session_idx: usize,
        jack: jack::Port<jack::AudioOut>,
    },
    MidiIn {
        session_idx: usize,
        jack: jack::Port<jack::MidiIn>,
    },
    MidiOut {
        session_idx: usize,
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
        let n_frames = ps.n_frames() as usize;
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
        let mut ports = ports.lock().unwrap_or_else(|e| e.into_inner());

        for p in ports.iter() {
            match p {
                JackRegisteredPort::AudioIn { session_idx, jack } => {
                    if let Some(port) = session
                        .port_mut(*session_idx)
                        .and_then(|p| p.as_external_mut())
                    {
                        port.stage_input(jack.as_slice(ps));
                    }
                }
                JackRegisteredPort::MidiIn { session_idx, jack } => {
                    if let Some(port) = session
                        .port_mut(*session_idx)
                        .and_then(|p| p.as_external_midi_mut())
                    {
                        for e in jack.iter(ps) {
                            let _ = port.push_incoming(e.time, e.bytes);
                        }
                    }
                }
                JackRegisteredPort::DecoupledMidiIn { queue, jack } => {
                    let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
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
                JackRegisteredPort::AudioOut { session_idx, jack } => {
                    let out = jack.as_mut_slice(ps);
                    if let Some(port) = session.port(*session_idx).and_then(|p| p.as_external()) {
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
                JackRegisteredPort::MidiOut { session_idx, jack } => {
                    let mut writer = jack.writer(ps);
                    if let Some(port) = session
                        .port(*session_idx)
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
                    let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
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
    input_ring: Option<Arc<Mutex<VecDeque<f32>>>>,
    input_channels: usize,
    output_channels: usize,
    playback_names: Vec<String>,
    capture_names: Vec<String>,
    midi_inputs: Arc<Mutex<Vec<CpalMidiInputEndpoint>>>,
    midi_outputs: Arc<Mutex<Vec<CpalMidiOutputEndpoint>>>,
    decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
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
            let mut queue = port.queue.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut queue = port.queue.lock().unwrap_or_else(|e| e.into_inner());
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
                input_stream = Some(input_device.build_input_stream(
                    &input_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut ring = cb_ring.lock().unwrap_or_else(|e| e.into_inner());
                        for &s in data {
                            if ring.len() >= cap {
                                ring.pop_front();
                                cb_xruns_in.fetch_add(1, Ordering::Relaxed);
                            }
                            ring.push_back(s);
                        }
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
        let midi_inputs_ret = midi_inputs_cb.clone();
        let midi_outputs_ret = midi_outputs_cb.clone();
        let external_cb = external.clone();
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
                let n_frames = data.len().checked_div(output_channels.max(1)).unwrap_or(0);
                last_processed_cb.store(n_frames as u32, Ordering::Relaxed);
                for s in data.iter_mut() {
                    *s = 0.0;
                }
                if let Some(callback) = maybe_process_callback {
                    unsafe {
                        callback();
                    }
                }
                let connections = external_cb
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .connections();
                let wanted = n_frames * input_channels;
                if capture_scratch.len() < wanted {
                    capture_scratch.resize(wanted, 0.0);
                }
                if let Some(ring) = input_ring_cb.as_ref() {
                    let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                    for s in &mut capture_scratch[..wanted] {
                        *s = ring.pop_front().unwrap_or(0.0);
                    }
                }

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
                    let decoupled = decoupled_cb
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    let mut inputs = midi_inputs_cb.lock().unwrap_or_else(|e| e.into_inner());
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
                    let decoupled = decoupled_cb
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    let mut outputs = midi_outputs_cb.lock().unwrap_or_else(|e| e.into_inner());
                    for output in outputs.iter_mut() {
                        for (port_id, ext_name) in &connections {
                            if ext_name != &output.name {
                                continue;
                            }
                            if let Some(session_idx) = port_id.0.checked_sub(1).map(|v| v as usize)
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
            input_ring,
            input_channels,
            output_channels,
            playback_names,
            capture_names,
            midi_inputs: midi_inputs_ret,
            midi_outputs: midi_outputs_ret,
            decoupled_midi_ports,
            last_processed,
            xruns,
        })
    }

    /// Start a CPAL backend against the software mock host rather than a real
    /// OS audio device, so the CPAL virtual port routing can be exercised on
    /// headless CI where ALSA / CoreAudio / WASAPI has no usable device.
    fn start_with_mock(
        _settings: &CpalMidiAudioDriverSettings,
        _external: Arc<Mutex<engine::DummyExternalConnections>>,
        decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
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

        let input_device = host.default_input_device().expect("mock input device");
        let input_config = input_device.default_input_config()?;
        let input_channels = input_config.channels() as usize;
        let input_device_name = input_device
            .name()
            .unwrap_or_else(|_| "mock-input".to_string());
        let capture_names: Vec<String> = (0..input_channels)
            .map(|c| format!("cpal:{input_device_name}:capture_{}", c + 1))
            .collect();

        let last_processed = Arc::new(AtomicU32::new(0));
        let xruns = Arc::new(AtomicU32::new(0));

        Ok(Self {
            stale_graph_cycles: Arc::new(AtomicU32::new(0)),
            _output: None,
            _input: None,
            input_ring: None,
            input_channels,
            sample_rate,
            configured_buffer_size: 0,
            output_channels,
            playback_names,
            capture_names,
            midi_inputs: Arc::new(Mutex::new(vec![])),
            midi_outputs: Arc::new(Mutex::new(vec![])),
            decoupled_midi_ports,
            last_processed,
            xruns,
        })
    }
}

#[derive(Clone)]
struct CompositeConfig {
    descriptor: engine::CompositePlanDescriptor,
    sync_source: engine::LoopIdentity,
}

#[derive(Clone, Default)]
struct CompositeRegistry {
    configs: BTreeMap<engine::LoopIdentity, CompositeConfig>,
    metadata: BTreeMap<engine::LoopIdentity, engine::LoopTargetMetadata>,
}

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
    engine::CompositeBoundaryTimeline::new(nodes, engine::CompositeTimelineLimits::default())
        .map_err(|error| anyhow!("composite timeline validation failed: {error}"))
}

struct SharedSession {
    /// The control side of the engine. Only ever touched by non-audio threads.
    ///
    /// The mutex here guards the *handle*, not the session: several GUI threads may queue
    /// work, but the session itself is reached solely through the queues inside, so no
    /// audio thread ever waits on this.
    handle: Mutex<engine::EngineHandle>,
    /// The engine, for as long as no driver has taken it.
    ///
    /// Between construction and a driver activating there is no audio thread at all, and
    /// session building still has to work. While the engine sits here the control thread
    /// drives it directly, which is sound precisely because nothing else can reach it.
    parked: Mutex<Option<engine::Engine>>,
    /// The cycle count when the most recent mutation was queued.
    ///
    /// What makes a published snapshot safe to read after writing. A command queued during
    /// cycle C is applied at the start of C+1 and published by that same cycle, so a snapshot
    /// is known to contain every queued write once its `cycle` is past this -- and until then
    /// a reader must ask the engine directly instead.
    ///
    /// Without this, a caller that sets something and immediately reads it back gets the value
    /// from before the set. That is not a hypothetical: it made `verify_loop_cleared` in the
    /// QML suite see the loop it had just cleared still playing at its old length.
    queued_at_cycle: AtomicU32,
    next_composite_slot: AtomicU32,
    next_composite_version: AtomicU32,
    composite_registry: Mutex<CompositeRegistry>,
    primitive_sync_sources: Mutex<Vec<Option<usize>>>,
    external: Mutex<Option<Arc<Mutex<engine::DummyExternalConnections>>>>,
    jack: Mutex<Option<Arc<Mutex<JackBackend>>>>,
    cpal: Mutex<Option<Arc<Mutex<CpalBackend>>>>,
    /// Rebuilds the schedule after topology changes.
    ///
    /// A `OnceLock` rather than a `Mutex`: it is set once immediately after construction
    /// -- it needs a `Weak` back to the `SharedSession` it applies to, so it cannot be
    /// built in the initialiser -- and read on every mutation thereafter.
    scheduler: OnceLock<GraphScheduler>,
}

impl SharedSession {
    /// Queues a mutation, and notes that the graph may need rebuilding.
    ///
    /// Arms the scheduler unconditionally rather than asking whether this particular change
    /// was structural. That is what `ControlGuard` used to buy by rescheduling on drop: no
    /// mutation site has to remember, which is the property that was missing when wiring up
    /// a track left a schedule that never got rebuilt. An armed-but-clean window is cheap --
    /// the scheduler asks the engine whether the graph is actually stale and does nothing if
    /// it is not.
    fn send(&self, f: impl FnMut(&mut engine::Session) + Send + 'static) {
        self.send_inner(f);
        if let Some(s) = self.scheduler.get() {
            s.arm();
        }
    }

    /// Queues work without arming the scheduler.
    ///
    /// For the scheduler's own install: arming from it would make every rebuild schedule
    /// another window, which then finds the graph current and does nothing. Safe because a
    /// change landing between the describe and the install armed the scheduler itself when it
    /// was queued, so nothing is lost by this path staying quiet.
    fn send_inner(&self, f: impl FnMut(&mut engine::Session) + Send + 'static) {
        {
            let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
            // Noted before queueing, so the recorded cycle can only be too early -- which
            // costs a reader one extra blocking read, where too late would hand it a stale
            // answer it had no way to detect.
            self.queued_at_cycle.store(
                handle.stats().cycles.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            let _ = handle.send(Box::new(f));
        }
        // Nothing is cycling a parked engine, so this thread runs what it just queued.
        if let Some(e) = self
            .parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            e.pump();
        }
    }

    /// Asks the engine something and waits for the answer.
    ///
    /// Ordered behind everything already queued, whichever path it takes, so a read always
    /// sees the writes that preceded it.
    ///
    /// Arms the scheduler like [`Self::send`] does, and for the same reason: this is not only
    /// used for reads. `create_loop`, `add_port` and `add_audio_channel` all go through here
    /// because the caller needs the index back, and every one of them changes the topology.
    /// Arming only on `send` is precisely the "some mutation sites remember and some do not"
    /// bug that `ControlGuard` was introduced to make impossible.
    fn query<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<T> {
        let answer = self.query_inner(f);
        if let Some(s) = self.scheduler.get() {
            s.arm();
        }
        answer
    }

    /// Asks the engine something without arming the scheduler.
    ///
    /// For the scheduler's own describe: arming from it would schedule another window after
    /// every rebuild, which would then find the graph current and do nothing.
    fn query_inner<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut engine::Session) -> T + Send + 'static,
    ) -> Result<T> {
        // Parked: no driver, so no other thread can be in the session. Apply anything queued
        // first to keep the ordering, then answer directly rather than waiting for a cycle
        // that nobody is going to run.
        if let Some(e) = self
            .parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            e.pump();
            return Ok(f(e.session_mut()));
        }
        // A driver owns the engine; the queue is the only way in.
        //
        // Queued under the lock, waited for *without* it. Holding the handle across the wait
        // is what makes every other control operation queue behind a full round trip to the
        // audio thread: with the scheduler describing the topology on its own thread, that
        // starved the GUI thread continuously and presented as the application hanging. It
        // is not a deadlock and nothing times out, which is what makes it hard to find.
        let rx = {
            let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
            handle
                .send_for_result(f)
                .map_err(|e| anyhow!("could not queue the request: {e}"))?
        };
        engine::wait_for_result(rx, engine::DEFAULT_WAIT_TIMEOUT)
            .map_err(|e| anyhow!("engine did not answer: {e}"))
    }

    /// Reads the newest published state, if a cycle has published one.
    ///
    /// The call every 40 Hz poll goes through. No lock the audio thread wants, no round
    /// trip: a `get_state` per object per frame as a blocking query would cost an audio
    /// cycle each, which is more than the frame budget as soon as a session has a few
    /// tracks.
    fn poll<T>(&self, f: impl FnOnce(&engine::StateSnapshot) -> T) -> Option<T> {
        let trustworthy_after = self.queued_at_cycle.load(Ordering::Relaxed);
        let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        match handle.poll() {
            // Past every queued write, so it reflects them all.
            Some(snap) if snap.cycle > trustworthy_after => Some(f(snap)),
            // Either nothing has been published yet, or what has predates a write this caller
            // may be about to read back. Callers treat `None` as "ask the engine instead".
            _ => None,
        }
    }

    /// Whether a schedule rebuild might be needed, without asking the audio thread.
    ///
    /// The scheduler runs this on every armed window, and every control operation arms one, so
    /// it has to be cheap: asking the engine directly costs a round trip, and doing that
    /// ~90 times a second kept the command queue permanently busy with questions whose answer
    /// was almost always "nothing to do" -- which starved every other control operation.
    ///
    /// A published `true` is trusted at once. A published `false` is only trusted when the
    /// queue is empty as well: a mutation that is queued but not yet applied has not dirtied
    /// the graph *yet*, and taking `false` at face value there would drop the rebuild on the
    /// floor with nothing left to arm another window. So a non-empty queue re-arms and looks
    /// again, which is bounded by the queue draining.
    fn graph_may_need_rebuild(&self) -> bool {
        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        if handle.stats().graph_stale.load(Ordering::Relaxed) {
            return true;
        }
        if handle.n_pending() > 0 {
            drop(handle);
            if let Some(s) = self.scheduler.get() {
                s.arm();
            }
            return false;
        }
        false
    }

    /// Hands the engine to a driver that is about to start cycling it.
    fn take_engine(&self) -> Option<engine::Engine> {
        self.parked.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// Takes the engine back from a driver that has stopped.
    ///
    /// Two reasons this is not optional. A driver that stops without returning it leaves the
    /// session unreachable, so every control call afterwards waits out its timeout. And the
    /// session would then be *destroyed on the driver's thread* -- which for a session holding
    /// Carla LV2 hosts means tearing down plugin instances on a thread that did not create
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
    fn cpal(&self) -> Option<Arc<Mutex<CpalBackend>>> {
        self.cpal.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
fn jack_connections_state(
    jack: &Arc<Mutex<JackBackend>>,
    own_short: &str,
    direction: PortDirection,
    data_type: PortDataType,
) -> HashMap<String, bool> {
    let j = jack.lock().unwrap_or_else(|e| e.into_inner());
    let client = j.client();
    let own = jack_full_name(client, own_short);
    let connected = client
        .port_by_name(&own)
        .map(|p| p.get_connections())
        .unwrap_or_default();
    jack_external_ports(&j, opposite_direction(direction), data_type)
        .into_iter()
        .map(|name| {
            let c = connected.iter().any(|n| n == &name);
            (name, c)
        })
        .collect()
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
    pub fn new() -> Result<Self> {
        Self::create()
    }
    pub fn create() -> Result<Self> {
        let mut s = engine::Session::default();
        s.apply_graph_changes().ok();
        // Capacity bounds how many control operations may be outstanding between cycles. A
        // session load issues them in bursts, so this is sized for a burst rather than for
        // the steady state.
        let (engine, handle) = engine::split(s, COMMAND_QUEUE_CAPACITY);
        let shared = Arc::new(SharedSession {
            handle: Mutex::new(handle),
            parked: Mutex::new(Some(engine)),
            queued_at_cycle: AtomicU32::new(0),
            next_composite_slot: AtomicU32::new(0x8000_0000),
            next_composite_version: AtomicU32::new(1),
            composite_registry: Mutex::new(CompositeRegistry::default()),
            primitive_sync_sources: Mutex::new(Vec::new()),
            external: Mutex::new(None),
            jack: Mutex::new(None),
            cpal: Mutex::new(None),
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
                let Ok(Some(topology)) = shared.query_inner(|s: &mut engine::Session| {
                    (!s.graph_up_to_date()).then(|| s.describe_topology())
                }) else {
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
                match shared
                    .query_inner(move |s: &mut engine::Session| s.install_schedule(prepared))
                {
                    Ok(displaced) => drop(displaced),
                    Err(_) => return,
                }
            }),
        );
        let _ = shared.scheduler.set(scheduler);

        Ok(Self { shared })
    }
    pub fn set_audio_driver(&self, driver: &AudioDriver) -> Result<()> {
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
    pub fn get_state(&self) -> BackendSessionState {
        BackendSessionState {
            audio_driver: std::ptr::null_mut(),
            n_audio_buffers_created: 0,
            n_audio_buffers_available: 0,
        }
    }
    /// Adds a loop and returns a handle to it.
    ///
    /// Blocking, unlike the setters: the caller needs the index before it can do anything
    /// with the loop, and guessing it would race any other creator.
    pub fn create_loop(&self) -> Result<Loop> {
        let idx = self
            .shared
            .query(|s: &mut engine::Session| s.create_loop())?;
        let mut sync_sources = self
            .shared
            .primitive_sync_sources
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if sync_sources.len() <= idx {
            sync_sources.resize(idx + 1, None);
        }
        Ok(Loop {
            shared: self.shared.clone(),
            idx,
        })
    }
    pub fn create_composite_loop(&self) -> Result<CompositeLoop> {
        let slot = self
            .shared
            .next_composite_slot
            .fetch_add(1, Ordering::Relaxed);
        if slot == u32::MAX {
            return Err(anyhow!("composite identity capacity exhausted"));
        }
        Ok(CompositeLoop {
            shared: self.shared.clone(),
            identity: engine::LoopIdentity {
                slot,
                generation: 1,
                kind: engine::LoopTargetKind::Composite,
            },
        })
    }
    pub fn primitive_sync_sources(&self) -> Vec<Option<usize>> {
        self.shared
            .primitive_sync_sources
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
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
        match self
            .shared
            .query_inner(move |session| session.install_prepared_composite_timeline(timeline))?
        {
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
        if descriptor.source != composite.identity || !Arc::ptr_eq(&self.shared, &composite.shared)
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
            composite.identity,
            CompositeConfig {
                descriptor,
                sync_source,
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
        if !registry.configs.contains_key(&composite.identity) {
            return Ok(0);
        }

        let mut removed = BTreeSet::from([composite.identity]);
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
            let handle = CompositeLoop {
                shared: self.shared.clone(),
                identity,
            };
            if handle.get_state()?.mode != LoopMode::Stopped {
                handle.transition_immediate(LoopMode::Stopped, 0)?;
            }
        }
        let deadline = Instant::now() + engine::DEFAULT_WAIT_TIMEOUT;
        for &identity in &removed {
            let handle = CompositeLoop {
                shared: self.shared.clone(),
                identity,
            };
            loop {
                match handle.get_state() {
                    Ok(state) if state.mode == LoopMode::Stopped => break,
                    Ok(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                    Ok(_) => return Err(anyhow!("composite did not stop before removal")),
                    Err(error) => return Err(error),
                }
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
        Ok(version)
    }

    pub fn create_fx_chain(&self, chain_type: FXChainType, title: &str) -> Result<FXChain> {
        let backend = match chain_type {
            FXChainType::Test2x2x1 => FXChainBackendKind::Test2x2x1,
            FXChainType::CarlaRack | FXChainType::CarlaPatchbay | FXChainType::CarlaPatchbay16x => {
                #[cfg(feature = "lv2")]
                {
                    let (sample_rate, buffer_size) = self
                        .shared
                        .query(|s: &mut engine::Session| {
                            (s.sample_rate().max(1), s.buffer_size().max(1))
                        })
                        .unwrap_or((48_000, 256));
                    match engine::lv2_carla::CarlaLv2Host::instantiate(
                        chain_type,
                        sample_rate,
                        buffer_size,
                    ) {
                        Ok(host) => FXChainBackendKind::Carla(Arc::new(Mutex::new(host))),
                        Err(e) => FXChainBackendKind::Unavailable {
                            reason: e.to_string(),
                        },
                    }
                }
                #[cfg(not(feature = "lv2"))]
                {
                    FXChainBackendKind::Unavailable {
                        reason: "shoop_engine was built without LV2 support".to_string(),
                    }
                }
            }
        };
        #[cfg(feature = "lv2")]
        if let FXChainBackendKind::Carla(host) = &backend {
            let (title, host) = (title.to_string(), host.clone());
            let mut pending = Some((title, host));
            self.shared.send(move |s: &mut engine::Session| {
                if let Some((title, host)) = pending.take() {
                    s.set_carla_fx_host(title, host);
                }
            });
        }
        Ok(FXChain {
            shared: self.shared.clone(),
            title: title.to_string(),
            chain_type,
            backend,
            state: Arc::new(Mutex::new(FXChainState::default())),
        })
    }
    pub fn get_profiling_report(&self) -> ProfilingReport {
        ProfilingReport::default()
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
    let (session, n, sample_rate, buffer_size, callback) = {
        let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
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
        )
    };

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
            inner
                .lock()
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
        inner
            .lock()
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
    inner
        .lock()
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
        i.dummy.settings_mut().buffer_size = backend.configured_buffer_size;
        i.cpal = Some(Arc::new(Mutex::new(backend)));
        Ok(())
    }
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
        if i.driver_type == AudioDriverType::Cpal {
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
            i.dummy_thread = Some(thread::spawn(move || {
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
                    // through them. A blocking read from the GUI thread would otherwise wait
                    // out the whole cycle interval, and the QML suite makes thousands of them:
                    // sleeping the interval in one go made the suite several times slower for
                    // no reason other than latency. Only pumps when something is actually
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
                // the session on this thread, and a session holding Carla LV2 hosts does not
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
            }));
        }
        Ok(())
    }
    fn register_audio_port(
        &self,
        name: &str,
        direction: PortDirection,
        session_idx: usize,
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
                            session_idx,
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
                            session_idx,
                            jack: p,
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn register_midi_port(
        &self,
        name: &str,
        direction: PortDirection,
        session_idx: usize,
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
                            session_idx,
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
                            session_idx,
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
        // Synchronously drain all pending controlled frames.
        // Unlike the QML wait_controlled_mode which relies on the async
        // update pipeline (UpdatedOnGuiThread signal), this directly polls
        // the driver state, which is reliable across test-file reloads.
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
    fn from_snapshot(snapshot: &engine::CompositeSnapshot) -> Self {
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
    identity: engine::LoopIdentity,
}

impl CompositeLoop {
    pub fn identity(&self) -> engine::LoopIdentity {
        self.identity
    }

    pub fn transition(&self, mode: LoopMode, delay: u32) -> Result<u64> {
        let (source, mode) = (self.identity, mode.into());
        self.shared
            .query_inner(move |session| session.accept_composite_transition(source, mode, delay))?
            .map_err(|error| anyhow!("composite transition rejected: {error}"))
    }

    pub fn transition_immediate(&self, mode: LoopMode, iteration: i64) -> Result<u64> {
        let (source, mode) = (self.identity, mode.into());
        self.shared
            .query_inner(move |session| {
                session.accept_composite_immediate_transition(source, mode, iteration)
            })?
            .map_err(|error| anyhow!("composite immediate transition rejected: {error}"))
    }

    pub fn set_play_after_record(&self, enabled: bool) -> Result<u64> {
        let source = self.identity;
        self.shared
            .query_inner(move |session| {
                session.accept_composite_play_after_record(source, enabled)
            })?
            .map_err(|error| anyhow!("composite record option rejected: {error}"))
    }

    pub fn poll_state(&self) -> Option<CompositeLoopState> {
        let identity = self.identity;
        self.shared.poll(|snapshot| {
            snapshot
                .composites
                .iter()
                .find(|composite| composite.identity == identity)
                .map(CompositeLoopState::from_snapshot)
        })?
    }

    pub fn get_state(&self) -> Result<CompositeLoopState> {
        let identity = self.identity;
        self.shared
            .query_inner(move |session| {
                let timeline = session.composite_timeline();
                (0..timeline.n_composites()).find_map(|index| {
                    let node = timeline.node_state(index)?;
                    (node.plan.source() == identity).then(|| {
                        let pending = node.runtime.pending();
                        let anticipated = pending
                            .map(|pending| (pending.mode, pending.boundaries_to_skip))
                            .or_else(|| timeline.anticipated_transition(identity));
                        CompositeLoopState {
                            identity,
                            active_plan_version: node.active_version,
                            pending_plan_version: node.pending_version,
                            mode: node.runtime.mode().into(),
                            maybe_next_mode: anticipated.map(|(mode, _)| mode.into()),
                            maybe_next_mode_delay: anticipated.map(|(_, delay)| delay),
                            iteration: node.runtime.iteration(),
                            cycle_count: node.runtime.cycle_count(),
                            length: node.runtime.length_samples(node.plan).unwrap_or(0),
                            position: node.runtime.position_samples(node.plan).unwrap_or(0),
                            play_after_record: node.runtime.play_after_record(),
                            active_children: node.runtime.active_children().collect(),
                            runtime_counters: node.runtime.counters(),
                            runtime_fault: node.runtime.fault(),
                        }
                    })
                })
            })?
            .ok_or_else(|| anyhow!("composite loop is not installed"))
    }
}

#[derive(Clone)]
pub struct Loop {
    shared: Arc<SharedSession>,
    idx: usize,
}
pub type LoopState = engine::LoopState;
impl Loop {
    pub fn identity(&self) -> engine::LoopIdentity {
        engine::LoopIdentity {
            slot: self.idx as u32,
            generation: 1,
            kind: engine::LoopTargetKind::Basic,
        }
    }

    pub fn add_audio_channel(&self, mode: ChannelMode) -> Result<AudioChannel> {
        let (idx, mode) = (self.idx, mode);
        let (session_idx, chan_idx) = self.shared.query(move |s: &mut engine::Session| {
            let session_idx = s.add_audio_channel(idx, 64, mode.into())?;
            let chan_idx = s
                .loop_(idx)
                .map_or(0, |l| l.n_audio_channels().saturating_sub(1));
            Ok::<_, engine::SessionError>((session_idx, chan_idx))
        })??;
        Ok(AudioChannel {
            shared: self.shared.clone(),
            loop_idx: self.idx,
            chan_idx,
            session_idx,
        })
    }
    pub fn add_midi_channel(&self, mode: ChannelMode) -> Result<MidiChannel> {
        let (idx, mode) = (self.idx, mode);
        let (session_idx, chan_idx) = self.shared.query(move |s: &mut engine::Session| {
            let session_idx = s.add_midi_channel(idx, 1024, mode.into())?;
            let chan_idx = s
                .loop_(idx)
                .map_or(0, |l| l.n_midi_channels().saturating_sub(1));
            Ok::<_, engine::SessionError>((session_idx, chan_idx))
        })??;
        Ok(MidiChannel {
            shared: self.shared.clone(),
            loop_idx: self.idx,
            chan_idx,
            session_idx,
        })
    }
    pub fn transition(
        &self,
        to_mode: LoopMode,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) -> Result<()> {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if maybe_cycles_delay >= 0 || maybe_to_sync_at_cycle >= 0 {
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
        });
        Ok(())
    }

    /// This loop's state as of the last published cycle, without blocking.
    ///
    /// The call a UI polling at frame rate wants. `get_state` would be correct too but costs
    /// a round trip per object per frame, which is more than a frame is worth once a session
    /// has a few tracks. Returns `None` until a cycle has published, and may be one cycle
    /// behind a setter that has only just been queued -- use `get_state` where that matters.
    pub fn poll_state(&self) -> Option<LoopState> {
        let idx = self.idx;
        self.shared.poll(|snap| {
            snap.loops.get(idx).map(|l| LoopState {
                mode: l.mode.into(),
                length: l.length,
                position: l.position,
                maybe_next_mode: l.next_mode.map(|m| m.into()),
                maybe_next_mode_delay: l.next_mode_delay,
            })
        })?
    }

    /// This loop's state, asked for directly.
    ///
    /// Ordered behind anything already queued, so a setter followed by this reads back what
    /// was set. That is what makes it the right call outside a frame-rate poll.
    pub fn get_state(&self) -> Result<LoopState> {
        let idx = self.idx;
        self.shared
            .query(move |s: &mut engine::Session| {
                s.loop_(idx).map(|l| {
                    let next = l.first_planned_transition().or_else(|| {
                        s.loop_identity(idx).and_then(|identity| {
                            s.composite_timeline().anticipated_transition(identity)
                        })
                    });
                    LoopState {
                        mode: l.mode().into(),
                        length: l.length(),
                        position: l.position(),
                        maybe_next_mode: next.map(|(m, _)| m.into()),
                        maybe_next_mode_delay: next.map(|(_, d)| d),
                    }
                })
            })?
            .ok_or_else(|| anyhow!("no loop"))
    }
    pub fn set_length(&self, length: u32) -> Result<()> {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.set_length(length);
            }
        });
        Ok(())
    }
    pub fn set_position(&self, position: u32) -> Result<()> {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.set_position(position);
            }
        });
        Ok(())
    }
    pub fn clear(&self, length: u32) -> Result<()> {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.clear(length);
                l.clear_planned_transitions();
                l.set_mode(engine::LoopMode::Stopped);
                l.set_position(0);
            }
        });
        Ok(())
    }
    pub fn set_sync_source(&self, src: Option<&Loop>) -> Result<()> {
        let idx = self.idx;
        let src = src.map(|loop_| loop_.idx).filter(|source| *source != idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.set_loop_sync_source(idx, src);
        });
        let mut sync_sources = self
            .shared
            .primitive_sync_sources
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if sync_sources.len() <= idx {
            sync_sources.resize(idx + 1, None);
        }
        sync_sources[idx] = src;
        Ok(())
    }
    pub fn adopt_ringbuffer_contents(
        &self,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<()> {
        let idx = self.idx;
        // Blocking, because the caller is told whether the adoption succeeded and a
        // fire-and-forget would have to swallow the error.
        self.shared.query(move |s: &mut engine::Session| {
            s.adopt_audio_ringbuffers_for_loop(
                idx,
                reverse_start_cycle,
                cycles_length,
                go_to_cycle,
                go_to_mode.into(),
            )
        })??;
        Ok(())
    }
}
pub fn transition_multiple_loops(
    loops: &[&Loop],
    to_state: LoopMode,
    maybe_cycles_delay: i32,
    maybe_to_sync_at_cycle: i32,
) -> Result<()> {
    for l in loops {
        l.transition(to_state, maybe_cycles_delay, maybe_to_sync_at_cycle)?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct AudioChannel {
    shared: Arc<SharedSession>,
    loop_idx: usize,
    chan_idx: usize,
    session_idx: usize,
}
pub type AudioChannelState = engine::AudioChannelState;
impl AudioChannel {
    /// Queues a mutation of this channel.
    ///
    /// One helper for every setter, as before -- what changed is that the closure now runs on
    /// the audio thread at a cycle boundary instead of under a lock. `FnMut` rather than
    /// `FnOnce` because the command is called through its box and the box has to survive to be
    /// sent back for freeing; it is still called exactly once.
    fn with_mut(&self, mut f: impl FnMut(&mut engine::AudioChannel) + Send + 'static) {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                f(c)
            }
        });
    }
    pub fn connect_input(&self, port: &AudioPort) {
        let (ci, pi) = (self.session_idx, port.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_channel_input(ci, pi);
        });
    }
    pub fn connect_output(&self, port: &AudioPort) {
        let (ci, pi) = (self.session_idx, port.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_channel_output(ci, pi);
        });
    }
    pub fn disconnect(&self, _port: &AudioPort) {
        let _ = self.session_idx;
    }
    pub fn load_data(&self, data: &[f32]) {
        // Copied here, on this thread, and moved into the command: the audio thread copies
        // out of the vector but never allocates one.
        let owned = data.to_vec();
        self.with_mut(move |c| c.load_data(&owned));
    }
    /// Reads the channel's samples back. Blocking; not for a frame-rate poll.
    pub fn get_data(&self) -> Vec<f32> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared
            .query(move |s: &mut engine::Session| {
                s.loop_(li)
                    .and_then(|l| l.audio_channel(ci))
                    .map(|c| c.data())
            })
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// This channel's state as of the last published cycle, without blocking.
    pub fn poll_state(&self) -> Option<AudioChannelState> {
        let session_idx = self.session_idx;
        let state = self
            .shared
            .poll(|snap| snap.audio_channels.get(session_idx).copied())?;
        if state.is_some() {
            let (loop_idx, chan_idx) = (self.loop_idx, self.chan_idx);
            self.shared.send_inner(move |s: &mut engine::Session| {
                if let Some(c) = s
                    .loop_mut(loop_idx)
                    .and_then(|l| l.audio_channel_mut(chan_idx))
                {
                    c.reset_output_peak();
                }
            });
        }
        state
    }

    pub fn get_state(&self) -> Result<AudioChannelState> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared
            .query(move |s: &mut engine::Session| {
                s.loop_mut(li)
                    .and_then(|l| l.audio_channel_mut(ci))
                    .map(|c| {
                        let output_peak = c.output_peak();
                        c.reset_output_peak();
                        AudioChannelState {
                            mode: c.mode(),
                            gain: c.gain(),
                            output_peak,
                            length: c.length() as u32,
                            start_offset: c.start_offset(),
                            played_back_sample: c.played_back_sample(),
                            n_preplay_samples: c.pre_play_samples(),
                            data_dirty: c.data_seq_nr() != 0,
                        }
                    })
            })?
            .ok_or_else(|| anyhow!("no channel"))
    }
    pub fn set_gain(&self, gain: f32) {
        self.with_mut(move |c| c.set_gain(gain));
    }
    pub fn set_mode(&self, mode: ChannelMode) {
        self.with_mut(move |c| c.set_mode(mode.into()));
    }
    pub fn set_start_offset(&self, offset: i32) {
        self.with_mut(move |c| c.set_start_offset(offset));
    }
    pub fn set_n_preplay_samples(&self, n: u32) {
        self.with_mut(move |c| c.set_pre_play_samples(n));
    }
    pub fn clear_data_dirty(&self) {}
    pub fn clear(&self, length: u32) {
        self.with_mut(move |c| c.clear(length as usize));
    }
}

#[derive(Clone)]
pub struct MidiChannel {
    shared: Arc<SharedSession>,
    loop_idx: usize,
    chan_idx: usize,
    session_idx: usize,
}
pub type MidiChannelState = engine::MidiChannelState;
impl MidiChannel {
    fn with_mut(&self, mut f: impl FnMut(&mut engine::MidiChannel) + Send + 'static) {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.midi_channel_mut(ci)) {
                f(c)
            }
        });
    }
    /// Reads the channel's events back. Blocking; not for a frame-rate poll.
    pub fn get_all_midi_data(&self) -> Vec<MidiEvent> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared
            .query(move |s: &mut engine::Session| {
                s.loop_(li).and_then(|l| l.midi_channel(ci)).map(|c| {
                    let mut out: Vec<MidiEvent> = c
                        .recording_start_state_messages()
                        .into_iter()
                        .map(|data| MidiEvent { time: -1, data })
                        .collect();
                    out.extend(c.contents().iter().map(|e| MidiEvent {
                        time: e.time as i32,
                        data: e.data().to_vec(),
                    }));
                    out
                })
            })
            .ok()
            .flatten()
            .unwrap_or_default()
    }
    pub fn load_all_midi_data(&self, msgs: &[MidiEvent]) {
        let state: Vec<Vec<u8>> = msgs
            .iter()
            .filter(|m| m.time < 0)
            .map(|m| m.data.clone())
            .collect();
        let elems: Vec<_> = msgs
            .iter()
            .filter(|m| m.time >= 0)
            .filter_map(|m| engine::midi_storage::MidiStorageElem::new(m.time as u32, &m.data))
            .collect();
        let len = elems.iter().map(|e| e.time).max().unwrap_or(0);
        self.with_mut(move |c| {
            c.set_contents(&elems, len, (!state.is_empty()).then_some(state.as_slice()))
        });
    }
    pub fn connect_input(&self, port: &MidiPort) {
        let (ci, pi) = (self.session_idx, port.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_channel_input(ci, pi);
        });
    }
    pub fn connect_output(&self, port: &MidiPort) {
        let (ci, pi) = (self.session_idx, port.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_channel_output(ci, pi);
        });
    }
    pub fn disconnect(&self, _port: &MidiPort) {
        let _ = self.session_idx;
    }

    /// This channel's state as of the last published cycle, without blocking.
    pub fn poll_state(&self) -> Option<MidiChannelState> {
        let ci = self.session_idx;
        self.shared
            .poll(|snap| snap.midi_channels.get(ci).copied())?
    }

    pub fn get_state(&self) -> Result<MidiChannelState> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        self.shared
            .query(move |s: &mut engine::Session| {
                s.loop_(li)
                    .and_then(|l| l.midi_channel(ci))
                    .map(|c| MidiChannelState {
                        mode: c.mode(),
                        n_events_triggered: c.n_events_triggered(),
                        n_notes_active: c.n_notes_active(),
                        length: c.length(),
                        start_offset: c.start_offset(),
                        played_back_sample: c.played_back_sample(),
                        n_preplay_samples: c.pre_play_samples(),
                        data_dirty: c.data_seq_nr() != 0,
                    })
            })?
            .ok_or_else(|| anyhow!("no channel"))
    }
    pub fn set_mode(&self, mode: ChannelMode) {
        self.with_mut(move |c| c.set_mode(mode.into()));
    }
    pub fn set_start_offset(&self, offset: i32) {
        self.with_mut(move |c| c.set_start_offset(offset));
    }
    pub fn set_n_preplay_samples(&self, n: u32) {
        self.with_mut(move |c| c.set_pre_play_samples(n));
    }
    pub fn clear_data_dirty(&self) {}
    pub fn clear(&self) {
        self.with_mut(move |c| c.clear());
    }
    pub fn reset_state_tracking(&self) {}
}

#[derive(Clone)]
pub struct AudioPort {
    shared: Arc<SharedSession>,
    idx: usize,
    direction: PortDirection,
    /// Kept here because the audio thread cannot publish it: a name is a `String`, so the
    /// snapshot carries only numbers and this side supplies the name it created the port with.
    name: String,
}
pub type AudioPortState = engine::AudioPortState;
impl AudioPort {
    pub fn new_driver_port(
        sess: &BackendSession,
        driver: &AudioDriver,
        name: &str,
        direction: &PortDirection,
        ring: u32,
    ) -> Result<Self> {
        let (owned, dir) = (name.to_string(), *direction);
        let idx = sess.shared.query(move |s: &mut engine::Session| {
            s.add_port(engine::session::Port::External(
                engine::external_audio_port::ExternalAudioPort::new(
                    owned,
                    dir.into(),
                    ring as usize,
                ),
            ))
        })?;
        driver.register_audio_port(name, *direction, idx)?;
        Ok(Self {
            shared: sess.shared.clone(),
            idx,
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
    /// Queues a mutation of this port's audio side.
    fn with_audio_mut(&self, mut f: impl FnMut(&mut engine::AudioPort) + Send + 'static) {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(a) = s.port_mut(idx).and_then(|p| p.audio_mut()) {
                f(a)
            }
        });
    }

    /// This port's state as of the last published cycle, without blocking.
    pub fn poll_state(&self) -> Option<AudioPortState> {
        let idx = self.idx;
        let polled = self
            .shared
            .poll(|snap| snap.audio_ports.get(idx).copied())??;
        let state = polled.map(|p| p.named(self.name.clone()));
        if state.is_some() {
            self.shared.send_inner(move |s: &mut engine::Session| {
                if let Some(a) = s.port_mut(idx).and_then(|p| p.audio_mut()) {
                    a.reset_input_peak();
                    a.reset_output_peak();
                }
            });
        }
        state
    }

    pub fn get_state(&self) -> Result<AudioPortState> {
        let idx = self.idx;
        let name = self.name.clone();
        self.shared
            .query(move |s: &mut engine::Session| {
                let a = s.port_mut(idx)?.audio_mut()?;
                let snapshot = engine::AudioPortSnapshot {
                    input_peak: a.input_peak(),
                    output_peak: a.output_peak(),
                    gain: a.gain(),
                    muted: a.muted(),
                    passthrough_muted: a.passthrough_muted(),
                    ringbuffer_n_samples: a.ringbuffer_n_samples() as u32,
                };
                a.reset_input_peak();
                a.reset_output_peak();
                Some(snapshot)
            })?
            .map(|p| p.named(name))
            .ok_or_else(|| anyhow!("no audio port"))
    }
    pub fn set_gain(&self, gain: f32) {
        self.with_audio_mut(move |a| a.set_gain(gain));
    }
    pub fn set_muted(&self, muted: bool) {
        self.with_audio_mut(move |a| a.set_muted(muted));
    }
    pub fn set_passthrough_muted(&self, muted: bool) {
        self.with_audio_mut(move |a| a.set_passthrough_muted(muted));
    }
    pub fn connect_internal(&self, other: &AudioPort) {
        let (from, to) = (self.idx, other.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_ports_internal(from, to);
        });
    }
    pub fn dummy_queue_data(&self, data: &[f32]) {
        let (idx, owned) = (self.idx, data.to_vec());
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(p) = s.port_mut(idx).and_then(|p| p.as_external_mut()) {
                p.stage_input(&owned)
            }
        });
    }
    pub fn dummy_dequeue_data(&self, n: u32) -> Vec<f32> {
        let idx = self.idx;
        self.shared
            .query(move |s: &mut engine::Session| {
                s.port_mut(idx)
                    .and_then(|p| p.as_external_mut())
                    .map(|p| p.dequeue_output(n as usize))
            })
            .ok()
            .flatten()
            .unwrap_or_default()
    }
    pub fn dummy_request_data(&self, _n: u32) {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(p) = s.port_mut(idx).and_then(|p| p.as_external_mut()) {
                p.clear_output_queue();
            }
        });
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        if let Some(j) = self.shared.jack() {
            return jack_connections_state(&j, &self.name, self.direction, PortDataType::Audio);
        }
        let mut out = HashMap::new();
        if let Some(ext) = self.shared.external() {
            let ext = ext.lock().unwrap_or_else(|e| e.into_inner());
            let connected = ext.connection_status_of(compat_port_id(self.idx));
            if let Ok(ports) = ext.find_external_ports(
                None,
                opposite_direction(self.direction).into(),
                engine::PortDataType::Audio,
            ) {
                for p in ports {
                    out.insert(p.name.clone(), *connected.get(&p.name).unwrap_or(&false));
                }
            }
        }
        out
    }
    pub fn connect_external_port(&self, name: &str) {
        if let Some(j) = self.shared.jack() {
            jack_connect_port(&j, &self.name, self.direction, name);
            return;
        }
        if let Some(ext) = self.shared.external() {
            let _ = ext
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .connect(compat_port_id(self.idx), name);
        }
    }
    pub fn disconnect_external_port(&self, name: &str) {
        if let Some(j) = self.shared.jack() {
            jack_disconnect_port(&j, &self.name, self.direction, name);
            return;
        }
        if let Some(ext) = self.shared.external() {
            let _ = ext
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .disconnect(compat_port_id(self.idx), name);
        }
    }
    pub fn set_ringbuffer_n_samples(&self, n: u32) {
        self.with_audio_mut(move |a| a.set_ringbuffer_n_samples(n as usize));
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
}

#[derive(Clone)]
pub struct MidiPort {
    shared: Arc<SharedSession>,
    idx: usize,
    direction: PortDirection,
    /// As on `AudioPort`: the audio thread cannot publish a `String`, so this side keeps it.
    name: String,
}
pub type MidiPortState = engine::MidiPortState;
impl MidiPort {
    pub fn new_driver_port(
        sess: &BackendSession,
        driver: &AudioDriver,
        name: &str,
        direction: &PortDirection,
        _ring: u32,
    ) -> Result<Self> {
        let (owned, dir) = (name.to_string(), *direction);
        let idx = sess.shared.query(move |s: &mut engine::Session| {
            s.add_port(engine::session::Port::ExternalMidi(
                engine::external_midi_port::ExternalMidiPort::new(owned, dir.into()),
            ))
        })?;
        driver.register_midi_port(name, *direction, idx)?;
        Ok(Self {
            shared: sess.shared.clone(),
            idx,
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
    fn with_midi_mut(&self, mut f: impl FnMut(&mut engine::MidiPort) + Send + 'static) {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(m) = s.port_mut(idx).and_then(|p| p.midi_mut()) {
                f(m)
            }
        });
    }

    /// This port's state as of the last published cycle, without blocking.
    pub fn poll_state(&self) -> Option<MidiPortState> {
        let idx = self.idx;
        let polled = self
            .shared
            .poll(|snap| snap.midi_ports.get(idx).copied())??;
        polled.map(|p| p.named(self.name.clone()))
    }

    pub fn get_state(&self) -> Result<MidiPortState> {
        let idx = self.idx;
        let name = self.name.clone();
        self.shared
            .query(move |s: &mut engine::Session| {
                let p = s.port(idx)?;
                let m = p.midi()?;
                Some(engine::MidiPortSnapshot {
                    n_input_events: m.n_input_events(),
                    n_input_notes_active: m.n_notes_active(),
                    n_output_events: m.n_output_events(),
                    n_output_notes_active: 0,
                    muted: m.muted(),
                    passthrough_muted: m.passthrough_muted(),
                    ringbuffer_n_samples: m.ringbuffer_n_samples(),
                })
            })?
            .map(|p| p.named(name))
            .ok_or_else(|| anyhow!("no midi port"))
    }
    pub fn set_muted(&self, muted: bool) {
        self.with_midi_mut(move |m| m.set_muted(muted));
    }
    pub fn set_passthrough_muted(&self, muted: bool) {
        self.with_midi_mut(move |m| m.set_passthrough_muted(muted));
    }
    pub fn connect_internal(&self, other: &MidiPort) {
        let (from, to) = (self.idx, other.idx);
        self.shared.send(move |s: &mut engine::Session| {
            let _ = s.connect_ports_internal(from, to);
        });
    }
    pub fn dummy_clear_queues(&self) {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(p) = s.port_mut(idx).and_then(|p| p.as_external_midi_mut()) {
                p.clear_queues();
            }
        });
    }
    pub fn dummy_queue_msg(&self, msg: &MidiEvent) {
        self.dummy_queue_msgs(vec![msg.clone()])
    }
    pub fn dummy_queue_msgs(&self, msgs: Vec<MidiEvent>) {
        let idx = self.idx;
        let mut pending = Some(msgs);
        self.shared.send(move |s: &mut engine::Session| {
            let Some(msgs) = pending.take() else {
                return;
            };
            if let Some(p) = s.port_mut(idx).and_then(|p| p.as_external_midi_mut()) {
                for m in msgs {
                    let _ = p.push_incoming(m.time.max(0) as u32, &m.data);
                }
            }
        });
    }
    pub fn dummy_dequeue_data(&self) -> Vec<MidiEvent> {
        let idx = self.idx;
        self.shared
            .query(move |s: &mut engine::Session| {
                s.port_mut(idx)
                    .and_then(|p| p.as_external_midi_mut())
                    .map(|p| {
                        p.dequeue_output()
                            .iter()
                            .map(|e| MidiEvent {
                                time: e.time as i32,
                                data: e.data().to_vec(),
                            })
                            .collect::<Vec<_>>()
                    })
            })
            .ok()
            .flatten()
            .unwrap_or_default()
    }
    pub fn dummy_request_data(&self, _n: u32) {
        let idx = self.idx;
        self.shared.send(move |s: &mut engine::Session| {
            if let Some(p) = s.port_mut(idx).and_then(|p| p.as_external_midi_mut()) {
                p.request_output();
            }
        });
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        if let Some(j) = self.shared.jack() {
            return jack_connections_state(&j, &self.name, self.direction, PortDataType::Midi);
        }
        let mut out = HashMap::new();
        if let Some(ext) = self.shared.external() {
            let ext = ext.lock().unwrap_or_else(|e| e.into_inner());
            let connected = ext.connection_status_of(compat_port_id(self.idx));
            if let Ok(ports) = ext.find_external_ports(
                None,
                opposite_direction(self.direction).into(),
                engine::PortDataType::Midi,
            ) {
                for p in ports {
                    out.insert(p.name.clone(), *connected.get(&p.name).unwrap_or(&false));
                }
            }
        }
        out
    }
    pub fn connect_external_port(&self, name: &str) {
        if let Some(j) = self.shared.jack() {
            jack_connect_port(&j, &self.name, self.direction, name);
            return;
        }
        if let Some(ext) = self.shared.external() {
            let _ = ext
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .connect(compat_port_id(self.idx), name);
        }
    }
    pub fn disconnect_external_port(&self, name: &str) {
        if let Some(j) = self.shared.jack() {
            jack_disconnect_port(&j, &self.name, self.direction, name);
            return;
        }
        if let Some(ext) = self.shared.external() {
            let _ = ext
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .disconnect(compat_port_id(self.idx), name);
        }
    }
    pub fn set_ringbuffer_n_samples(&self, n: u32) {
        self.with_midi_mut(move |m| m.set_ringbuffer_n_samples(n));
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
        if let Some(j) = self.jack.as_ref() {
            return jack_connections_state(j, &self.name, self.direction, PortDataType::Midi);
        }
        self.external
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connection_status_of(self.port_id)
            .into_iter()
            .collect()
    }
    pub fn connect_external_port(&self, name: &str) {
        if let Some(j) = self.jack.as_ref() {
            jack_connect_port(j, &self.name, self.direction, name);
            return;
        }
        let _ = self
            .external
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connect(self.port_id, name);
    }
    pub fn disconnect_external_port(&self, name: &str) {
        if let Some(j) = self.jack.as_ref() {
            jack_disconnect_port(j, &self.name, self.direction, name);
            return;
        }
        let _ = self
            .external
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .disconnect(self.port_id, name);
    }
}

pub type FXChainState = engine::FXChainState;

enum FXChainBackendKind {
    Test2x2x1,
    #[cfg(feature = "lv2")]
    Carla(Arc<Mutex<engine::lv2_carla::CarlaLv2Host>>),
    Unavailable {
        reason: String,
    },
}

pub struct FXChain {
    shared: Arc<SharedSession>,
    title: String,
    chain_type: FXChainType,
    backend: FXChainBackendKind,
    state: Arc<Mutex<FXChainState>>,
}
impl FXChain {
    pub fn available(&self) -> bool {
        !matches!(self.backend, FXChainBackendKind::Unavailable { .. })
    }
    pub fn set_visible(&self, visible: bool) {
        self.state.lock().unwrap().visible = visible as u32;
        #[cfg(feature = "lv2")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            let ok = host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_visible(visible)
                .is_ok();
            self.state.lock().unwrap().visible = (visible && ok) as u32;
        }
    }
    pub fn set_active(&self, active: bool) {
        self.state.lock().unwrap().active = active as u32;
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => {
                let mut pending = Some(self.title.clone());
                self.shared.send(move |s: &mut engine::Session| {
                    if let Some(title) = pending.take() {
                        s.set_test_fx_active(title, active);
                    }
                });
            }
            #[cfg(feature = "lv2")]
            FXChainBackendKind::Carla(host) => host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_active(active),
            FXChainBackendKind::Unavailable { .. } => {}
        }
    }
    pub fn get_state(&self) -> Option<FXChainState> {
        let mut s = self.state.lock().unwrap().clone();
        s.ready = self.available() as u32;
        #[cfg(feature = "lv2")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            s.visible = host.lock().unwrap_or_else(|e| e.into_inner()).is_visible() as u32;
            self.state.lock().unwrap().visible = s.visible;
        }
        Some(s)
    }
    pub fn get_state_str(&self) -> Option<String> {
        match &self.backend {
            FXChainBackendKind::Unavailable { reason } => Some(format!(
                "{{\"chain_type\":\"{:?}\",\"unavailable\":{reason:?}}}",
                self.chain_type
            )),
            #[cfg(feature = "lv2")]
            FXChainBackendKind::Carla(host) => host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .save_state_string()
                .ok(),
            _ => Some(String::new()),
        }
    }
    pub fn restore_state(&self, state: &str) {
        #[cfg(feature = "lv2")]
        if let FXChainBackendKind::Carla(host) = &self.backend {
            let _ = host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .restore_state_string(state);
        }
    }
    fn n_audio_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => 2,
            #[cfg(feature = "lv2")]
            FXChainBackendKind::Carla(host) => host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .info
                .ports
                .audio_inputs
                .len(),
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }
    fn n_midi_input_ports(&self) -> usize {
        match &self.backend {
            FXChainBackendKind::Test2x2x1 => 1,
            #[cfg(feature = "lv2")]
            FXChainBackendKind::Carla(host) => host
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .info
                .ports
                .midi_inputs
                .len(),
            FXChainBackendKind::Unavailable { .. } => 0,
        }
    }
    /// Adds an internal port for this chain, or `None` if the engine could not answer.
    ///
    /// `None` rather than a fallback index: defaulting to 0 would hand back a handle
    /// pointing at whichever port happens to be first, and every subsequent call on it would
    /// quietly act on that one instead. A missing port is visible; a wrong one is not.
    fn make_audio_port(&self, name: String, direction: PortDirection) -> Option<AudioPort> {
        let owned = name.clone();
        let idx = self
            .shared
            .query(move |s: &mut engine::Session| {
                let n_frames = s.buffer_size().max(1) as usize;
                s.add_port(engine::session::Port::Internal(
                    engine::InternalAudioPort::new(
                        owned,
                        n_frames,
                        engine::PortConnectability::INTERNAL,
                        engine::PortConnectability::INTERNAL,
                        0,
                    ),
                ))
            })
            .ok()?;
        Some(AudioPort {
            shared: self.shared.clone(),
            idx,
            direction,
            name,
        })
    }
    /// As [`Self::make_audio_port`], including why this is an `Option`.
    fn make_midi_port(&self, name: String, direction: PortDirection) -> Option<MidiPort> {
        let owned = name.clone();
        let idx = self
            .shared
            .query(move |s: &mut engine::Session| {
                s.add_port(engine::session::Port::ExternalMidi(
                    engine::external_midi_port::ExternalMidiPort::new(owned, direction.into()),
                ))
            })
            .ok()?;
        Some(MidiPort {
            shared: self.shared.clone(),
            idx,
            direction,
            name,
        })
    }
    pub fn get_audio_input_port(&self, idx: u32) -> Option<AudioPort> {
        ((idx as usize) < self.n_audio_ports())
            .then(|| {
                self.make_audio_port(
                    format!("{}:audio_in_{}", self.title, idx),
                    PortDirection::Output,
                )
            })
            .flatten()
    }
    pub fn get_audio_output_port(&self, idx: u32) -> Option<AudioPort> {
        ((idx as usize) < self.n_audio_ports())
            .then(|| {
                self.make_audio_port(
                    format!("{}:audio_out_{}", self.title, idx),
                    PortDirection::Input,
                )
            })
            .flatten()
    }
    pub fn get_midi_input_port(&self, idx: u32) -> Option<MidiPort> {
        ((idx as usize) < self.n_midi_input_ports())
            .then(|| {
                self.make_midi_port(
                    format!("{}:midi_in_{}", self.title, idx),
                    PortDirection::Output,
                )
            })
            .flatten()
    }
    pub fn get_midi_output_port(&self, _idx: u32) -> Option<MidiPort> {
        None
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
            .query(|s: &mut engine::Session| s.graph_up_to_date())
            .expect("engine answered")
    }

    /// The invariant `ControlGuard` exists to enforce.
    ///
    /// Connecting a channel to a port used to leave the graph dirty with nothing scheduled
    /// to rebuild it, because only three of the mutation sites remembered to call
    /// `apply_graph_changes`. Now the guard cannot be dropped without at least arming the
    /// rebuild, so no mutation site has to remember.
    #[test]
    fn a_connection_leaves_the_graph_scheduled_for_rebuild() {
        let sess = BackendSession::new().expect("session");
        let loop_ = sess.create_loop().expect("loop");
        let channel = loop_
            .add_audio_channel(ChannelMode::Direct)
            .expect("channel");
        let port = {
            let idx = sess
                .shared
                .query(|s: &mut engine::Session| {
                    s.add_port(engine::session::Port::External(
                        engine::external_audio_port::ExternalAudioPort::new(
                            "out",
                            engine::PortDirection::Output,
                            0,
                        ),
                    ))
                })
                .expect("add port");
            AudioPort {
                shared: sess.shared.clone(),
                idx,
                direction: PortDirection::Output,
                name: "out".to_string(),
            }
        };
        sess.shared.flush_graph_changes();
        assert!(graph_up_to_date(&sess));

        // The call that used to leave the session permanently stale.
        channel.connect_output(&port);
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
    #[test]
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

    #[test]
    fn real_jack_is_not_advanced_by_state_polling() {
        assert!(!driver_uses_dummy_processing(AudioDriverType::Jack));
        assert!(driver_uses_dummy_processing(AudioDriverType::JackTest));
        assert!(driver_uses_dummy_processing(AudioDriverType::Dummy));
        assert!(!driver_uses_dummy_processing(AudioDriverType::Cpal));
    }

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
    fn cpal_backend_exposes_virtual_audio_ports_through_app_api_when_device_available() {
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
            // Fails rather than passes when there is no audio device, unless skipping was
            // opted into -- mirrors `tests/backend_availability`, which an inline test in
            // the library crate cannot reach.
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
        let target = playback_ports[0].name.clone();
        let before = app_port.get_connections_state();
        assert_eq!(before.get(&target), Some(&false));
        app_port.connect_external_port(&target);
        let connected = app_port.get_connections_state();
        assert_eq!(connected.get(&target), Some(&true));
        app_port.disconnect_external_port(&target);
        let after = app_port.get_connections_state();
        assert_eq!(after.get(&target), Some(&false));

        let state = driver.get_state();
        assert_eq!(state.active, 1);
        assert!(state.sample_rate > 0);
    }

    #[test]
    fn current_fx_chain_handle_controls_visibility_activity_and_ports() {
        let sess = BackendSession::new().expect("session");
        let chain = sess
            .create_fx_chain(FXChainType::Test2x2x1, "test_fx")
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

    #[cfg(feature = "lv2")]
    #[test]
    fn carla_fx_chain_handle_instantiates_when_plugin_is_available() {
        let sess = BackendSession::new().expect("session");
        let chain = sess
            .create_fx_chain(FXChainType::CarlaRack, "carla")
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
            state.starts_with('{'),
            "Carla state should be JSON: {state}"
        );
        chain.restore_state(&state);
    }

    #[test]
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
        driver.wait_process();
        let initial = match port.poll_state() {
            Some(state) => state,
            None => port.get_state().expect("initial state"),
        };
        assert_eq!(initial.input_peak, 0.0);
        assert_eq!(initial.output_peak, 0.0);

        port.dummy_queue_data(&[0.0, -0.8, 0.2, 0.1]);
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let first = port.get_state().expect("first state");
        assert_eq!(first.input_peak, 0.8);
        assert_eq!(first.output_peak, 0.8);

        port.dummy_queue_data(&[0.0, -0.3, 0.1, 0.2]);
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let second = port.poll_state().expect("second state");
        assert_eq!(second.input_peak, 0.3);
        assert_eq!(second.output_peak, 0.3);

        port.dummy_queue_data(&[0.0, -0.1, 0.05, 0.0]);
        driver.dummy_request_controlled_frames(BUFFER);
        driver.dummy_run_requested_frames();
        let third = port.poll_state().expect("third state");
        assert_eq!(third.input_peak, 0.1);
        assert_eq!(third.output_peak, 0.1);
    }

    #[test]
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
    #[test]
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

    #[test]
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
