//! Application-facing backend handles used by the QML/frontend layer.
//!
//! This module is the compatibility boundary between the CXX-Qt frontend objects and
//! the Rust engine.  It owns driver/session handles, port/channel/loop handles and
//! the small amount of JACK/CPAL/midir routing glue the GUI expects, while all actual
//! loop, graph, port, MIDI and FX processing stays in the core engine modules.

#![allow(dead_code)]

use crate as engine;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
pub use engine::{
    cpal_host_names, cpal_input_device_names, cpal_input_device_names_for_host,
    cpal_output_device_names, cpal_output_device_names_for_host, driver_type_supported,
    midir_input_port_names, midir_output_port_names, AudioDriverType, ChannelMode, FXChainType,
    LoopMode, MidiEvent, MultichannelAudio, PortConnectabilityKind, PortDataType, PortDirection,
    ProfilingReport, ProfilingReportItem,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant};

pub type PortConnectability = engine::PortConnectability;

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
    shared: Weak<SharedSession>,
    ports: Arc<Mutex<Vec<JackRegisteredPort>>>,
    last_processed: Arc<AtomicU32>,
    sample_rate: u32,
    maybe_process_callback: Option<ProcessCallback>,
}
impl jack::ProcessHandler for JackProcess {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;
        let Some(shared) = self.shared.upgrade() else {
            return jack::Control::Continue;
        };
        if let Some(callback) = self.maybe_process_callback {
            unsafe {
                callback();
            }
        }
        let mut session = shared.lock();
        session.set_sample_rate(self.sample_rate);
        session.set_buffer_size(n_frames as u32);
        let mut ports = self.ports.lock().unwrap_or_else(|e| e.into_inner());

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

        let _ = crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| session.process(n_frames));

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
        self.last_processed
            .store(n_frames as u32, Ordering::Relaxed);
        jack::Control::Continue
    }
}

struct JackBackend {
    client: Option<jack::Client>,
    active_client: Option<jack::AsyncClient<JackNotifications, JackProcess>>,
    ports: Arc<Mutex<Vec<JackRegisteredPort>>>,
    last_processed: Arc<AtomicU32>,
    xruns: Arc<AtomicU32>,
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
        let notifications = JackNotifications {
            xruns: self.xruns.clone(),
        };
        let process = JackProcess {
            shared: Arc::downgrade(shared),
            ports: self.ports.clone(),
            last_processed: self.last_processed.clone(),
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
        shared: Weak<SharedSession>,
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
        let mut capture_scratch = Vec::<f32>::new();

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
                let Some(shared) = shared.upgrade() else {
                    return;
                };
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

                let mut session = shared.lock();
                session.set_sample_rate(sample_rate);
                session.set_buffer_size(n_frames as u32);

                stage_virtual_audio_inputs(
                    &mut session,
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
                                &mut session,
                                &connections,
                                &input.name,
                                &events,
                                &decoupled,
                            );
                        }
                    }
                }

                let _ = crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| {
                    session.process(n_frames)
                });

                collect_virtual_audio_outputs(
                    &session,
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
        _shared: Weak<SharedSession>,
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

struct SharedSession {
    session: Mutex<engine::Session>,
    external: Mutex<Option<Arc<Mutex<engine::DummyExternalConnections>>>>,
    jack: Mutex<Option<Arc<Mutex<JackBackend>>>>,
    cpal: Mutex<Option<Arc<Mutex<CpalBackend>>>>,
}
impl SharedSession {
    fn lock(&self) -> MutexGuard<'_, engine::Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
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
        Ok(Self {
            shared: Arc::new(SharedSession {
                session: Mutex::new(s),
                external: Mutex::new(None),
                jack: Mutex::new(None),
                cpal: Mutex::new(None),
            }),
        })
    }
    pub fn set_audio_driver(&self, driver: &AudioDriver) -> Result<()> {
        driver.attach_session(&self.shared);
        *self
            .shared
            .external
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(driver.external());
        *self.shared.jack.lock().unwrap_or_else(|e| e.into_inner()) = driver.jack();
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
    pub fn create_loop(&self) -> Result<Loop> {
        let mut s = self.shared.lock();
        let idx = s.create_loop();
        s.apply_graph_changes().ok();
        Ok(Loop {
            shared: self.shared.clone(),
            idx,
        })
    }
    pub fn create_fx_chain(&self, chain_type: FXChainType, title: &str) -> Result<FXChain> {
        let backend = match chain_type {
            FXChainType::Test2x2x1 => FXChainBackendKind::Test2x2x1,
            FXChainType::CarlaRack | FXChainType::CarlaPatchbay | FXChainType::CarlaPatchbay16x => {
                #[cfg(feature = "lv2")]
                {
                    let s = self.shared.lock();
                    let sample_rate = s.sample_rate().max(1);
                    let buffer_size = s.buffer_size().max(1);
                    drop(s);
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
            self.shared
                .lock()
                .set_carla_fx_host(title.to_string(), host.clone());
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
    settings: engine::DriverSettings,
    active: bool,
    controlled: bool,
    requested: u32,
    last_processed: u32,
    process_generation: u64,
    finish: Arc<AtomicBool>,
    dummy_thread: Option<thread::JoinHandle<()>>,
    session: Option<Weak<SharedSession>>,
    external: Arc<Mutex<engine::DummyExternalConnections>>,
    jack: Option<Arc<Mutex<JackBackend>>>,
    cpal: Option<Arc<Mutex<CpalBackend>>>,
    cpal_settings: Option<CpalMidiAudioDriverSettings>,
    cpal_decoupled_midi_ports: Arc<Mutex<Vec<CpalDecoupledMidiPort>>>,
    maybe_process_callback: Option<ProcessCallback>,
}
pub struct AudioDriver {
    inner: Arc<Mutex<DriverInner>>,
}

fn process_dummy_driver_iteration(inner: &Arc<Mutex<DriverInner>>) {
    let (session, n, sample_rate, buffer_size, callback) = {
        let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
        if !i.active || !driver_uses_dummy_processing(i.driver_type) {
            i.last_processed = 0;
            i.process_generation = i.process_generation.wrapping_add(1);
            return;
        }
        let n = if i.controlled {
            i.requested.min(i.settings.buffer_size)
        } else {
            i.settings.buffer_size
        };
        if i.controlled {
            i.requested -= n;
        }
        i.process_generation = i.process_generation.wrapping_add(1);
        (
            i.session.as_ref().and_then(|w| w.upgrade()),
            n,
            i.settings.sample_rate,
            i.settings.buffer_size,
            i.maybe_process_callback,
        )
    };

    if let Some(callback) = callback {
        unsafe {
            callback();
        }
    }
    if n == 0 {
        inner.lock().unwrap_or_else(|e| e.into_inner()).last_processed = 0;
        return;
    }
    if let Some(shared) = session {
        let mut s = shared.lock();
        s.set_sample_rate(sample_rate);
        s.set_buffer_size(buffer_size);
        // Channel connections and other internal routing changes bump graph_request_id
        // from the GUI thread, but the dummy iteration is the only path that calls
        // session.process().  Unless we apply those pending changes here, process()
        // returns Err(GraphOutOfDate) and the cycle is silently dropped.  The
        // original C++ backend handled this by calling PROC_handle_command_queue()
        // at the top of every iteration (and by running graph recalculation on a
        // separate thread that the process loop merely notified).
        s.apply_graph_changes().ok();
        s.process(n as usize).ok();
        {
            let mut i = inner.lock().unwrap_or_else(|e| e.into_inner());
            i.last_processed = n;
        }
    }
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
                settings: engine::DriverSettings::default(),
                active: false,
                controlled: false,
                requested: 0,
                last_processed: 0,
                process_generation: 0,
                finish: Arc::new(AtomicBool::new(false)),
                dummy_thread: None,
                session: None,
                external: Arc::new(Mutex::new(engine::DummyExternalConnections::default())),
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
            .external
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
        let backend = if i.driver_type == AudioDriverType::CpalTest {
            CpalBackend::start_with_mock(
                Arc::downgrade(shared),
                &settings,
                i.external.clone(),
                i.cpal_decoupled_midi_ports.clone(),
                i.maybe_process_callback,
            )?
        } else {
            CpalBackend::start(
                Arc::downgrade(shared),
                &settings,
                i.external.clone(),
                i.cpal_decoupled_midi_ports.clone(),
                i.maybe_process_callback,
            )?
        };
        i.settings.sample_rate = backend.sample_rate;
        i.settings.buffer_size = backend.configured_buffer_size;
        i.cpal = Some(Arc::new(Mutex::new(backend)));
        Ok(())
    }
    pub fn start(&self, settings: &AudioDriverSettings) -> Result<()> {
        let mut i = self.inner.lock().unwrap();
        match settings {
            AudioDriverSettings::Dummy(s) => {
                i.settings = engine::DriverSettings {
                    sample_rate: s.sample_rate,
                    buffer_size: s.buffer_size,
                    client_name: s.client_name.clone(),
                }
            }
            AudioDriverSettings::Jack(s) => i.settings.client_name = s.client_name_hint.clone(),
            AudioDriverSettings::Cpal(s) => i.settings.client_name = s.client_name.clone(),
        };
        if i.driver_type == AudioDriverType::Jack {
            let (client, _status) = jack::Client::new(
                &i.settings.client_name,
                jack::ClientOptions::NO_START_SERVER,
            )
            .map_err(|e| anyhow!("Failed to open JACK client: {e}"))?;
            i.settings.client_name = client.name().to_string();
            i.settings.sample_rate = client.sample_rate();
            i.settings.buffer_size = client.buffer_size();
            i.jack = Some(Arc::new(Mutex::new(JackBackend {
                client: Some(client),
                active_client: None,
                ports: Arc::new(Mutex::new(Vec::new())),
                last_processed: Arc::new(AtomicU32::new(0)),
                xruns: Arc::new(AtomicU32::new(0)),
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
            i.settings.sample_rate = 0;
            i.settings.buffer_size = 0;
            i.settings.client_name = cpal_settings.client_name.clone();
            i.cpal_settings = Some(cpal_settings);
        } else {
            i.cpal = None;
            i.cpal_settings = None;
        }
        if i.driver_type == AudioDriverType::JackTest {
            let mut ext = i.external.lock().unwrap_or_else(|e| e.into_inner());
            ext.remove_all_mock_ports();
            for client in [
                "test_client_1",
                "test_client_2",
                i.settings.client_name.as_str(),
            ] {
                ext.add_mock_port(
                    format!("{client}:audio_in"),
                    engine::PortDirection::Input,
                    engine::PortDataType::Audio,
                );
                ext.add_mock_port(
                    format!("{client}:audio_out"),
                    engine::PortDirection::Output,
                    engine::PortDataType::Audio,
                );
                ext.add_mock_port(
                    format!("{client}:midi_in"),
                    engine::PortDirection::Input,
                    engine::PortDataType::Midi,
                );
                ext.add_mock_port(
                    format!("{client}:midi_out"),
                    engine::PortDirection::Output,
                    engine::PortDataType::Midi,
                );
            }
        }
        i.active = true;
        if driver_uses_dummy_processing(i.driver_type) && i.dummy_thread.is_none() {
            i.finish.store(false, Ordering::Relaxed);
            let inner = self.inner.clone();
            let finish = i.finish.clone();
            i.dummy_thread = Some(thread::spawn(move || {
                while !finish.load(Ordering::Relaxed) {
                    let (sample_rate, buffer_size) = {
                        let i = inner.lock().unwrap_or_else(|e| e.into_inner());
                        (i.settings.sample_rate.max(1), i.settings.buffer_size.max(1))
                    };
                    let micros = ((buffer_size as f64 / sample_rate as f64) * 1_000_000.0)
                        .ceil()
                        .max(1.0) as u64;
                    let started = Instant::now();
                    process_dummy_driver_iteration(&inner);
                    let elapsed = started.elapsed();
                    let interval = Duration::from_micros(micros);
                    if elapsed < interval {
                        thread::sleep(interval - elapsed);
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
        self.inner.lock().unwrap().settings.sample_rate
    }
    pub fn get_buffer_size(&self) -> u32 {
        self.inner.lock().unwrap().settings.buffer_size
    }
    pub fn active(&self) -> bool {
        self.inner.lock().unwrap().active
    }
    pub fn wait_process(&self) {
        let (is_dummy, target) = {
            let i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            (
                driver_uses_dummy_processing(i.driver_type),
                i.process_generation.saturating_add(2),
            )
        };
        if is_dummy {
            wait_for_dummy_generation(&self.inner, target, Duration::from_millis(100));
        }
    }
    pub fn get_state(&self) -> AudioDriverState {
        let i = self.inner.lock().unwrap();
        let (last_processed, xruns_since_last) = if let Some(j) = i.jack.as_ref() {
            let j = j.lock().unwrap_or_else(|e| e.into_inner());
            (
                j.last_processed.load(Ordering::Relaxed),
                j.xruns.swap(0, Ordering::Relaxed),
            )
        } else if let Some(c) = i.cpal.as_ref() {
            let c = c.lock().unwrap_or_else(|e| e.into_inner());
            (
                c.last_processed.load(Ordering::Relaxed),
                c.xruns.swap(0, Ordering::Relaxed),
            )
        } else {
            (i.last_processed, 0)
        };
        AudioDriverState {
            dsp_load_percent: 0.0,
            xruns_since_last,
            maybe_instance_name: i.settings.client_name.clone(),
            sample_rate: i.settings.sample_rate,
            buffer_size: i.settings.buffer_size,
            active: i.active as u32,
            last_processed,
        }
    }
    pub fn dummy_enter_controlled_mode(&self) {
        let mut i = self.inner.lock().unwrap();
        i.controlled = true;
        i.requested = 0;
        i.last_processed = 0;
    }
    pub fn dummy_enter_automatic_mode(&self) {
        let mut i = self.inner.lock().unwrap();
        i.controlled = false;
        i.requested = 0;
    }
    pub fn dummy_is_controlled(&self) -> bool {
        self.inner.lock().unwrap().controlled
    }
    pub fn dummy_wait_controlled_mode(&self) {
        // Synchronously drain all pending controlled frames.
        // Unlike the QML wait_controlled_mode which relies on the async
        // update pipeline (UpdatedOnGuiThread signal), this directly polls
        // the driver state, which is reliable across test-file reloads.
        self.wait_process();
        while {
            let i = self.inner.lock().unwrap();
            i.last_processed != 0 || i.requested != 0
        } {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.wait_process();
    }
    pub fn dummy_request_controlled_frames(&self, n: u32) {
        self.inner.lock().unwrap().requested += n;
    }
    pub fn dummy_n_requested_frames(&self) -> u32 {
        self.inner.lock().unwrap().requested
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
            i.active = false;
            i.dummy_thread.take()
        };
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub struct Loop {
    shared: Arc<SharedSession>,
    idx: usize,
}
pub type LoopState = engine::LoopState;
impl Loop {
    pub fn add_audio_channel(&self, mode: ChannelMode) -> Result<AudioChannel> {
        let mut s = self.shared.lock();
        let session_idx = s.add_audio_channel(self.idx, 64, mode.into())?;
        s.apply_graph_changes().ok();
        let chan_idx = s
            .loop_(self.idx)
            .map_or(0, |l| l.n_audio_channels().saturating_sub(1));
        Ok(AudioChannel {
            shared: self.shared.clone(),
            loop_idx: self.idx,
            chan_idx,
            session_idx,
        })
    }
    pub fn add_midi_channel(&self, mode: ChannelMode) -> Result<MidiChannel> {
        let mut s = self.shared.lock();
        let session_idx = s.add_midi_channel(self.idx, 1024, mode.into())?;
        s.apply_graph_changes().ok();
        let chan_idx = s
            .loop_(self.idx)
            .map_or(0, |l| l.n_midi_channels().saturating_sub(1));
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
        let mut s = self.shared.lock();
        if maybe_cycles_delay >= 0 || maybe_to_sync_at_cycle >= 0 {
            if let Some(l) = s.loop_mut(self.idx) {
                l.plan_transition(
                    to_mode.into(),
                    (maybe_cycles_delay >= 0).then_some(maybe_cycles_delay as u32),
                    (maybe_to_sync_at_cycle >= 0).then_some(maybe_to_sync_at_cycle as u32),
                );
            }
        } else {
            let _ = s.set_loop_mode(self.idx, to_mode.into());
        }
        Ok(())
    }
    pub fn get_state(&self) -> Result<LoopState> {
        let s = self.shared.lock();
        let l = s.loop_(self.idx).ok_or_else(|| anyhow!("no loop"))?;
        let next = l.first_planned_transition();
        Ok(LoopState {
            mode: l.mode().into(),
            length: l.length(),
            position: l.position(),
            maybe_next_mode: next.map(|(m, _)| m.into()),
            maybe_next_mode_delay: next.map(|(_, d)| d),
        })
    }
    pub fn set_length(&self, length: u32) -> Result<()> {
        if let Some(l) = self.shared.lock().loop_mut(self.idx) {
            l.set_length(length);
        }
        Ok(())
    }
    pub fn set_position(&self, position: u32) -> Result<()> {
        if let Some(l) = self.shared.lock().loop_mut(self.idx) {
            l.set_position(position);
        }
        Ok(())
    }
    pub fn clear(&self, length: u32) -> Result<()> {
        let mut s = self.shared.lock();
        if let Some(l) = s.loop_mut(self.idx) {
            l.clear(length);
            l.clear_planned_transitions();
            l.set_mode(engine::LoopMode::Stopped);
            l.set_position(0);
        }
        s.apply_graph_changes().ok();
        Ok(())
    }
    pub fn set_sync_source(&self, src: Option<&Loop>) -> Result<()> {
        let _ = self
            .shared
            .lock()
            .set_loop_sync_source(self.idx, src.map(|l| l.idx));
        Ok(())
    }
    pub fn adopt_ringbuffer_contents(
        &self,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<()> {
        let mut s = self.shared.lock();
        s.adopt_audio_ringbuffers_for_loop(
            self.idx,
            reverse_start_cycle,
            cycles_length,
            go_to_cycle,
            go_to_mode.into(),
        )?;
        s.apply_graph_changes().ok();
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
    fn with_mut(&self, f: impl FnOnce(&mut engine::AudioChannel)) {
        if let Some(c) = self
            .shared
            .lock()
            .loop_mut(self.loop_idx)
            .and_then(|l| l.audio_channel_mut(self.chan_idx))
        {
            f(c)
        }
    }
    pub fn connect_input(&self, port: &AudioPort) {
        let _ = self
            .shared
            .lock()
            .connect_channel_input(self.session_idx, port.idx);
    }
    pub fn connect_output(&self, port: &AudioPort) {
        let _ = self
            .shared
            .lock()
            .connect_channel_output(self.session_idx, port.idx);
    }
    pub fn disconnect(&self, _port: &AudioPort) {
        let _ = self.session_idx;
    }
    pub fn load_data(&self, data: &[f32]) {
        self.with_mut(|c| c.load_data(data));
    }
    pub fn get_data(&self) -> Vec<f32> {
        self.shared
            .lock()
            .loop_(self.loop_idx)
            .and_then(|l| l.audio_channel(self.chan_idx))
            .map(|c| c.data())
            .unwrap_or_default()
    }
    pub fn get_state(&self) -> Result<AudioChannelState> {
        let s = self.shared.lock();
        let c = s
            .loop_(self.loop_idx)
            .and_then(|l| l.audio_channel(self.chan_idx))
            .ok_or_else(|| anyhow!("no channel"))?;
        Ok(AudioChannelState {
            mode: c.mode().into(),
            gain: c.gain(),
            output_peak: c.output_peak(),
            length: c.length() as u32,
            start_offset: c.start_offset(),
            played_back_sample: c.played_back_sample(),
            n_preplay_samples: c.pre_play_samples(),
            data_dirty: c.data_seq_nr() != 0,
        })
    }
    pub fn set_gain(&self, gain: f32) {
        self.with_mut(|c| c.set_gain(gain));
    }
    pub fn set_mode(&self, mode: ChannelMode) {
        self.with_mut(|c| c.set_mode(mode.into()));
    }
    pub fn set_start_offset(&self, offset: i32) {
        self.with_mut(|c| c.set_start_offset(offset));
    }
    pub fn set_n_preplay_samples(&self, n: u32) {
        self.with_mut(|c| c.set_pre_play_samples(n));
    }
    pub fn clear_data_dirty(&self) {}
    pub fn clear(&self, length: u32) {
        self.with_mut(|c| c.clear(length as usize));
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
    fn with_mut(&self, f: impl FnOnce(&mut engine::MidiChannel)) {
        if let Some(c) = self
            .shared
            .lock()
            .loop_mut(self.loop_idx)
            .and_then(|l| l.midi_channel_mut(self.chan_idx))
        {
            f(c)
        }
    }
    pub fn get_all_midi_data(&self) -> Vec<MidiEvent> {
        self.shared
            .lock()
            .loop_(self.loop_idx)
            .and_then(|l| l.midi_channel(self.chan_idx))
            .map(|c| {
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
        self.with_mut(|c| {
            c.set_contents(&elems, len, (!state.is_empty()).then_some(state.as_slice()))
        });
    }
    pub fn connect_input(&self, port: &MidiPort) {
        let _ = self
            .shared
            .lock()
            .connect_channel_input(self.session_idx, port.idx);
    }
    pub fn connect_output(&self, port: &MidiPort) {
        let _ = self
            .shared
            .lock()
            .connect_channel_output(self.session_idx, port.idx);
    }
    pub fn disconnect(&self, _port: &MidiPort) {
        let _ = self.session_idx;
    }
    pub fn get_state(&self) -> Result<MidiChannelState> {
        let s = self.shared.lock();
        let c = s
            .loop_(self.loop_idx)
            .and_then(|l| l.midi_channel(self.chan_idx))
            .ok_or_else(|| anyhow!("no channel"))?;
        Ok(MidiChannelState {
            mode: c.mode().into(),
            n_events_triggered: c.n_events_triggered(),
            n_notes_active: c.n_notes_active(),
            length: c.length(),
            start_offset: c.start_offset(),
            played_back_sample: c.played_back_sample(),
            n_preplay_samples: c.pre_play_samples(),
            data_dirty: c.data_seq_nr() != 0,
        })
    }
    pub fn set_mode(&self, mode: ChannelMode) {
        self.with_mut(|c| c.set_mode(mode.into()));
    }
    pub fn set_start_offset(&self, offset: i32) {
        self.with_mut(|c| c.set_start_offset(offset));
    }
    pub fn set_n_preplay_samples(&self, n: u32) {
        self.with_mut(|c| c.set_pre_play_samples(n));
    }
    pub fn clear_data_dirty(&self) {}
    pub fn clear(&self) {
        self.with_mut(|c| c.clear());
    }
    pub fn reset_state_tracking(&self) {}
}

#[derive(Clone)]
pub struct AudioPort {
    shared: Arc<SharedSession>,
    idx: usize,
    direction: PortDirection,
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
        let mut s = sess.shared.lock();
        let idx = s.add_port(engine::session::Port::External(
            engine::external_audio_port::ExternalAudioPort::new(
                name,
                (*direction).into(),
                ring as usize,
            ),
        ));
        s.apply_graph_changes().ok();
        drop(s);
        driver.register_audio_port(name, *direction, idx)?;
        Ok(Self {
            shared: sess.shared.clone(),
            idx,
            direction: *direction,
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
    pub fn get_state(&self) -> Result<AudioPortState> {
        let s = self.shared.lock();
        let p = s.port(self.idx).ok_or_else(|| anyhow!("no port"))?;
        let a = p.audio().ok_or_else(|| anyhow!("not audio"))?;
        Ok(AudioPortState {
            input_peak: a.input_peak(),
            output_peak: a.output_peak(),
            gain: a.gain(),
            muted: a.muted(),
            passthrough_muted: a.passthrough_muted(),
            ringbuffer_n_samples: a.ringbuffer_n_samples() as u32,
            name: p.name().to_string(),
        })
    }
    pub fn set_gain(&self, gain: f32) {
        if let Some(a) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.audio_mut())
        {
            a.set_gain(gain)
        }
    }
    pub fn set_muted(&self, muted: bool) {
        if let Some(a) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.audio_mut())
        {
            a.set_muted(muted)
        }
    }
    pub fn set_passthrough_muted(&self, muted: bool) {
        if let Some(a) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.audio_mut())
        {
            a.set_passthrough_muted(muted)
        }
    }
    pub fn connect_internal(&self, other: &AudioPort) {
        let mut s = self.shared.lock();
        let _ = s.connect_ports_internal(self.idx, other.idx);
        s.apply_graph_changes().ok();
    }
    pub fn dummy_queue_data(&self, data: &[f32]) {
        if let Some(p) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_mut())
        {
            p.stage_input(data)
        }
    }
    pub fn dummy_dequeue_data(&self, n: u32) -> Vec<f32> {
        self.shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_mut())
            .map(|p| p.dequeue_output(n as usize))
            .unwrap_or_default()
    }
    pub fn dummy_request_data(&self, _n: u32) {
        if let Some(p) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_mut())
        {
            p.clear_output_queue();
        }
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        if let Some(j) = self.shared.jack() {
            let name = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            return jack_connections_state(&j, &name, self.direction, PortDataType::Audio);
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
            let own = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            jack_connect_port(&j, &own, self.direction, name);
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
            let own = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            jack_disconnect_port(&j, &own, self.direction, name);
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
        if let Some(a) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.audio_mut())
        {
            a.set_ringbuffer_n_samples(n as usize)
        }
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
        let mut s = sess.shared.lock();
        let idx = s.add_port(engine::session::Port::ExternalMidi(
            engine::external_midi_port::ExternalMidiPort::new(name, (*direction).into()),
        ));
        s.apply_graph_changes().ok();
        drop(s);
        driver.register_midi_port(name, *direction, idx)?;
        Ok(Self {
            shared: sess.shared.clone(),
            idx,
            direction: *direction,
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
    pub fn get_state(&self) -> Result<MidiPortState> {
        let s = self.shared.lock();
        let p = s.port(self.idx).ok_or_else(|| anyhow!("no port"))?;
        let m = p.midi().ok_or_else(|| anyhow!("not midi"))?;
        Ok(MidiPortState {
            n_input_events: m.n_input_events(),
            n_input_notes_active: m.n_notes_active(),
            n_output_events: m.n_output_events(),
            n_output_notes_active: 0,
            muted: m.muted(),
            passthrough_muted: m.passthrough_muted(),
            ringbuffer_n_samples: m.ringbuffer_n_samples(),
            name: p.name().to_string(),
        })
    }
    pub fn set_muted(&self, muted: bool) {
        if let Some(m) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.midi_mut())
        {
            m.set_muted(muted)
        }
    }
    pub fn set_passthrough_muted(&self, muted: bool) {
        if let Some(m) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.midi_mut())
        {
            m.set_passthrough_muted(muted)
        }
    }
    pub fn connect_internal(&self, other: &MidiPort) {
        let mut s = self.shared.lock();
        let _ = s.connect_ports_internal(self.idx, other.idx);
        s.apply_graph_changes().ok();
    }
    pub fn dummy_clear_queues(&self) {
        if let Some(p) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_midi_mut())
        {
            p.clear_queues();
        }
    }
    pub fn dummy_queue_msg(&self, msg: &MidiEvent) {
        self.dummy_queue_msgs(vec![msg.clone()])
    }
    pub fn dummy_queue_msgs(&self, msgs: Vec<MidiEvent>) {
        let mut s = self.shared.lock();
        if let Some(p) = s.port_mut(self.idx).and_then(|p| p.as_external_midi_mut()) {
            for m in msgs {
                let _ = p.push_incoming(m.time.max(0) as u32, &m.data);
            }
        }
    }
    pub fn dummy_dequeue_data(&self) -> Vec<MidiEvent> {
        self.shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_midi_mut())
            .map(|p| {
                p.dequeue_output()
                    .iter()
                    .map(|e| MidiEvent {
                        time: e.time as i32,
                        data: e.data().to_vec(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn dummy_request_data(&self, _n: u32) {
        if let Some(p) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.as_external_midi_mut())
        {
            p.request_output();
        }
    }
    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        if let Some(j) = self.shared.jack() {
            let name = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            return jack_connections_state(&j, &name, self.direction, PortDataType::Midi);
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
            let own = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            jack_connect_port(&j, &own, self.direction, name);
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
            let own = self
                .shared
                .lock()
                .port(self.idx)
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            jack_disconnect_port(&j, &own, self.direction, name);
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
        if let Some(m) = self
            .shared
            .lock()
            .port_mut(self.idx)
            .and_then(|p| p.midi_mut())
        {
            m.set_ringbuffer_n_samples(n)
        }
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
            FXChainBackendKind::Test2x2x1 => self
                .shared
                .lock()
                .set_test_fx_active(self.title.clone(), active),
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
    fn make_audio_port(&self, name: String, direction: PortDirection) -> AudioPort {
        let mut s = self.shared.lock();
        let n_frames = s.buffer_size().max(1) as usize;
        let idx = s.add_port(engine::session::Port::Internal(
            engine::InternalAudioPort::new(
                name,
                n_frames,
                engine::PortConnectability::INTERNAL,
                engine::PortConnectability::INTERNAL,
                0,
            ),
        ));
        s.apply_graph_changes().ok();
        AudioPort {
            shared: self.shared.clone(),
            idx,
            direction,
        }
    }
    fn make_midi_port(&self, name: String, direction: PortDirection) -> MidiPort {
        let mut s = self.shared.lock();
        let idx = s.add_port(engine::session::Port::ExternalMidi(
            engine::external_midi_port::ExternalMidiPort::new(name, direction.into()),
        ));
        s.apply_graph_changes().ok();
        MidiPort {
            shared: self.shared.clone(),
            idx,
            direction,
        }
    }
    pub fn get_audio_input_port(&self, idx: u32) -> Option<AudioPort> {
        ((idx as usize) < self.n_audio_ports()).then(|| {
            self.make_audio_port(
                format!("{}:audio_in_{}", self.title, idx),
                PortDirection::Output,
            )
        })
    }
    pub fn get_audio_output_port(&self, idx: u32) -> Option<AudioPort> {
        ((idx as usize) < self.n_audio_ports()).then(|| {
            self.make_audio_port(
                format!("{}:audio_out_{}", self.title, idx),
                PortDirection::Input,
            )
        })
    }
    pub fn get_midi_input_port(&self, idx: u32) -> Option<MidiPort> {
        ((idx as usize) < self.n_midi_input_ports()).then(|| {
            self.make_midi_port(
                format!("{}:midi_in_{}", self.title, idx),
                PortDirection::Output,
            )
        })
    }
    pub fn get_midi_output_port(&self, _idx: u32) -> Option<MidiPort> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        s.process(4).expect("process");

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
        s.process(4).expect("process");

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
            eprintln!("no usable CPAL output device; skipping: {e}");
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
        assert!(sess.shared.lock().graph_up_to_date());
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

    #[test]
    fn get_state_does_not_advance_dummy_time() {
        let driver = AudioDriver {
            inner: Arc::new(Mutex::new(DriverInner {
                driver_type: AudioDriverType::Dummy,
                settings: engine::DriverSettings {
                    sample_rate: 48_000,
                    buffer_size: 256,
                    client_name: "test".to_string(),
                },
                active: true,
                controlled: false,
                requested: 0,
                last_processed: 0,
                process_generation: 0,
                finish: Arc::new(AtomicBool::new(false)),
                dummy_thread: None,
                session: None,
                external: Arc::new(Mutex::new(engine::DummyExternalConnections::default())),
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
