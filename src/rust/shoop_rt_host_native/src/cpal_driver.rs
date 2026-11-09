//! The CPAL driver: the engine renders in the output stream's callback; a separate
//! input stream feeds it through a lock-free bridge.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Error, ErrorKind, StreamConfig, SupportedBufferSize};
use rtrb::{Consumer, Producer, RingBuffer};
use shoop_rt_engine::{EngineDescription, HostPorts, MAX_HOST_PORTS};
use shoop_rt_protocol::{DriverKind, DriverRunState};

use crate::cycle::Cycle;
use crate::midi::{input_bridge, output_bridge, MidiDevices, Pump};
use crate::status::{Shared, StreamFailure};
use crate::{assemble, port_name, reset_wait, Driver, Prepared, Running, StartError, Started};

/// Captured audio the duplex bridge holds, in largest callbacks.
const BRIDGE_CALLBACKS: usize = 4;
/// How long stopping waits for a stream to hand back the render owner.
const RETURN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum DeviceSelection {
    /// The host's default device.
    #[default]
    Default,
    /// The device with exactly this name.
    Named(String),
    /// No device. Only inputs may be absent.
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CpalConfig {
    /// The CPAL host by name, such as `alsa`, `jack` or `wasapi`; the platform
    /// default when unset.
    pub host: Option<String>,
    pub output_device: DeviceSelection,
    pub input_device: DeviceSelection,
    /// The output device's default when unset.
    pub sample_rate: Option<u32>,
    /// Frames per callback; the device chooses when unset. Callbacks above the
    /// engine's prepared maximum are rendered in parts.
    pub buffer_frames: Option<u32>,
    /// Use at most this many channels of a device.
    pub output_channels: Option<u16>,
    pub input_channels: Option<u16>,
    /// midir port names to open.
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
    /// The name the host registers its MIDI connections under.
    pub client_name: String,
}

impl Default for CpalConfig {
    fn default() -> Self {
        Self {
            host: None,
            output_device: DeviceSelection::Default,
            input_device: DeviceSelection::Default,
            sample_rate: None,
            buffer_frames: None,
            output_channels: None,
            input_channels: None,
            midi_inputs: Vec::new(),
            midi_outputs: Vec::new(),
            client_name: "ShoopDaLoop".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioDevice {
    pub name: String,
    /// The host's default device for its direction.
    pub default: bool,
    pub channels: u16,
    pub sample_rate: u32,
    /// Supported frames per callback, if the device reports them.
    pub buffer_frames: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Discovery {
    /// The host the devices belong to.
    pub host: String,
    /// Every CPAL host available on this platform.
    pub hosts: Vec<String>,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
}

pub fn cpal_hosts() -> Vec<String> {
    cpal::available_hosts()
        .into_iter()
        .map(|id| id.name().to_lowercase())
        .collect()
}

fn select_host(name: Option<&str>) -> Result<cpal::Host, StartError> {
    let Some(name) = name else {
        return Ok(cpal::default_host());
    };
    let id = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| StartError::NoHost(name.into()))?;
    cpal::host_from_id(id).map_err(|error| StartError::Device(error.to_string()))
}

pub fn resolve_cpal_devices(config: &CpalConfig) -> Result<CpalConfig, StartError> {
    let host = select_host(config.host.as_deref())?;
    let mut resolved = resolve_cpal_devices_on(&host, config)?;
    resolved.host = Some(host.id().name().to_lowercase());
    Ok(resolved)
}

pub fn resolve_cpal_devices_on<H: HostTrait>(
    host: &H,
    config: &CpalConfig,
) -> Result<CpalConfig, StartError> {
    let mut resolved = config.clone();
    resolved.output_device = select_device(host, true, &config.output_device)?
        .map(|device| DeviceSelection::Named(device_name(&device)))
        .ok_or(StartError::NoDevice {
            direction: "output",
            name: None,
        })?;
    resolved.input_device = select_device(host, false, &config.input_device)?
        .map_or(DeviceSelection::None, |device| {
            DeviceSelection::Named(device_name(&device))
        });
    Ok(resolved)
}

pub fn preflight_cpal(config: &CpalConfig, max_frames: usize) -> Result<(u32, u32), StartError> {
    let host = select_host(config.host.as_deref())?;
    preflight_cpal_on(&host, config, max_frames)
}

pub fn preflight_cpal_on<H: HostTrait>(
    host: &H,
    config: &CpalConfig,
    max_frames: usize,
) -> Result<(u32, u32), StartError> {
    let output = select_device(host, true, &config.output_device)?.ok_or(StartError::NoDevice {
        direction: "output",
        name: None,
    })?;
    let default = output.default_output_config().map_err(device_error)?;
    let rate = config.sample_rate.unwrap_or(default.sample_rate());
    let channels = config.output_channels.unwrap_or(default.channels());
    if channels == 0 || usize::from(channels) > MAX_HOST_PORTS {
        return Err(StartError::InvalidConfig(
            "output channel capacity exceeded",
        ));
    }
    if !output
        .supported_output_configs()
        .map_err(device_error)?
        .any(|range| {
            range.min_sample_rate() <= rate
                && range.max_sample_rate() >= rate
                && range.channels() == channels
        })
    {
        return Err(StartError::InvalidConfig("output format is unavailable"));
    }
    buffer_size(config.buffer_frames, default.buffer_size(), max_frames)?;
    if let Some(input) = select_device(host, false, &config.input_device)? {
        let default = input.default_input_config().map_err(device_error)?;
        let channels = config.input_channels.unwrap_or(default.channels());
        if channels == 0 || usize::from(channels) > MAX_HOST_PORTS {
            return Err(StartError::InvalidConfig("input channel capacity exceeded"));
        }
        if !input
            .supported_input_configs()
            .map_err(device_error)?
            .any(|range| {
                range.min_sample_rate() <= rate
                    && range.max_sample_rate() >= rate
                    && range.channels() == channels
            })
        {
            return Err(StartError::InvalidConfig("input format is unavailable"));
        }
    }
    Ok((rate, config.buffer_frames.unwrap_or(0)))
}

/// Devices and MIDI ports of a CPAL host, the platform default when unset.
pub fn discover(host: Option<&str>) -> Result<Discovery, StartError> {
    let selected = select_host(host)?;
    let (outputs, inputs) = audio_devices(&selected);
    let (midi_inputs, midi_outputs) = crate::midi_ports();
    Ok(Discovery {
        host: selected.id().name().to_lowercase(),
        hosts: cpal_hosts(),
        outputs,
        inputs,
        midi_inputs,
        midi_outputs,
    })
}

fn device_name<D: DeviceTrait>(device: &D) -> String {
    device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "unnamed".into())
}

fn buffer_range(size: &SupportedBufferSize) -> Option<(u32, u32)> {
    match *size {
        SupportedBufferSize::Range { min, max } => Some((min, max)),
        SupportedBufferSize::Unknown => None,
    }
}

/// Output and input devices of `host` with their default configurations.
pub fn audio_devices<H: HostTrait>(host: &H) -> (Vec<AudioDevice>, Vec<AudioDevice>) {
    let list = |output: bool| -> Vec<AudioDevice> {
        let default = if output {
            host.default_output_device()
        } else {
            host.default_input_device()
        }
        .and_then(|device| device.id().ok());
        let devices = if output {
            host.output_devices()
                .map(|devices| devices.collect::<Vec<_>>())
        } else {
            host.input_devices()
                .map(|devices| devices.collect::<Vec<_>>())
        };
        devices
            .unwrap_or_default()
            .into_iter()
            .filter_map(|device| {
                let config = if output {
                    device.default_output_config()
                } else {
                    device.default_input_config()
                }
                .ok()?;
                Some(AudioDevice {
                    name: device_name(&device),
                    default: default.is_some() && device.id().ok() == default,
                    channels: config.channels(),
                    sample_rate: config.sample_rate(),
                    buffer_frames: buffer_range(config.buffer_size()),
                })
            })
            .collect()
    };
    (list(true), list(false))
}

fn select_device<H: HostTrait>(
    host: &H,
    output: bool,
    selection: &DeviceSelection,
) -> Result<Option<H::Device>, StartError> {
    let direction = if output { "output" } else { "input" };
    match selection {
        DeviceSelection::None => Ok(None),
        DeviceSelection::Default => Ok(if output {
            host.default_output_device()
        } else {
            host.default_input_device()
        }),
        DeviceSelection::Named(name) => {
            let devices = if output {
                host.output_devices()
                    .map(|devices| devices.collect::<Vec<_>>())
            } else {
                host.input_devices()
                    .map(|devices| devices.collect::<Vec<_>>())
            }
            .map_err(|error| StartError::Device(error.to_string()))?;
            devices
                .into_iter()
                .find(|device| device_name(device) == *name)
                .map(Some)
                .ok_or_else(|| StartError::NoDevice {
                    direction,
                    name: Some(name.clone()),
                })
        }
    }
}

fn channels(
    available: u16,
    limit: Option<u16>,
    direction: &str,
    warnings: &mut Vec<String>,
) -> u16 {
    let wanted = limit.map_or(available, |limit| limit.clamp(1, available.max(1)));
    if usize::from(wanted) > MAX_HOST_PORTS {
        warnings.push(format!(
            "using {MAX_HOST_PORTS} of the {direction} device's {wanted} channels"
        ));
        return MAX_HOST_PORTS as u16;
    }
    wanted
}

/// The buffer to request, refused if the engine cannot render it in one callback or
/// the device does not support it.
fn buffer_size(
    requested: Option<u32>,
    supported: &SupportedBufferSize,
    max: usize,
) -> Result<BufferSize, StartError> {
    let max = u32::try_from(max).unwrap_or(u32::MAX);
    match (requested, buffer_range(supported)) {
        (Some(requested), _) if requested > max => {
            Err(StartError::BufferTooLarge { requested, max })
        }
        (Some(requested), Some((min, most))) if requested < min || requested > most => {
            Err(StartError::BufferUnsupported {
                requested,
                min,
                max: most,
            })
        }
        (Some(requested), _) => Ok(BufferSize::Fixed(requested)),
        (None, Some((min, _))) if min > max => Err(StartError::BufferTooLarge {
            requested: min,
            max,
        }),
        (None, _) => Ok(BufferSize::Default),
    }
}

fn failure(shared: &Shared, error: Error) {
    match error.kind() {
        ErrorKind::Xrun => shared.xrun(&shared.counters.stream_underruns),
        ErrorKind::DeviceNotAvailable => shared.fail(StreamFailure::DeviceNotAvailable),
        ErrorKind::StreamInvalidated => shared.fail(StreamFailure::StreamInvalidated),
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
        _ => shared.fail(StreamFailure::Backend),
    }
}

fn device_error(error: impl std::fmt::Display) -> StartError {
    StartError::Device(error.to_string())
}

/// The input stream's side of the duplex bridge.
struct Capture {
    callbacks: Arc<crate::callback_gate::CallbackGate>,
    producer: Producer<f32>,
    channels: usize,
    started: Arc<AtomicBool>,
    shared: Arc<Shared>,
}

impl Capture {
    fn push(&mut self, data: &[f32]) {
        let Some(_turn) = self.callbacks.enter() else {
            return;
        };
        self.started.store(true, Ordering::Relaxed);
        let frames = data.len() / self.channels;
        let room = self.producer.slots() / self.channels;
        let take = frames.min(room) * self.channels;
        if let Ok(chunk) = self.producer.write_chunk_uninit(take) {
            chunk.fill_from_iter(data[..take].iter().copied());
        }
        if frames * self.channels > take {
            self.shared.xrun(&self.shared.counters.input_overruns);
        }
    }
}

/// The output callback's side of the duplex bridge.
struct Playback {
    consumer: Consumer<f32>,
    channels: usize,
    started: Arc<AtomicBool>,
}

impl Playback {
    fn read(&mut self, inputs: &mut [Box<[f32]>], frames: usize, shared: &Shared) {
        let channels = self.channels;
        let take = (self.consumer.slots() / channels).min(frames);
        if let Ok(chunk) = self.consumer.read_chunk(take * channels) {
            let (first, second) = chunk.as_slices();
            for (index, sample) in first.iter().chain(second).enumerate() {
                inputs[index % channels][index / channels] = *sample;
            }
            chunk.commit_all();
        }
        for input in inputs.iter_mut() {
            input[take..frames].fill(0.);
        }
        if take < frames && self.started.load(Ordering::Relaxed) {
            shared.xrun(&shared.counters.input_underruns);
        }
    }
}

/// Owns the render cycle inside the output callback and hands it back when CPAL
/// drops the callback.
struct Output {
    callbacks: Arc<crate::callback_gate::CallbackGate>,
    cycle: Option<Cycle>,
    playback: Option<Playback>,
    channels: usize,
    frames: usize,
    shared: Arc<Shared>,
    back: SyncSender<Cycle>,
}

impl Output {
    fn process(&mut self, data: &mut [f32]) {
        let Some(_turn) = self.callbacks.enter() else {
            data.fill(0.);
            return;
        };
        let started = Instant::now();
        let Some(cycle) = self.cycle.as_mut() else {
            data.fill(0.);
            return;
        };
        let shared = &*self.shared;
        let channels = self.channels;
        let frames = data.len() / channels;
        shared.begin_profiling(cycle.next_frame(), frames);
        Shared::count(&shared.counters.callbacks, 1);
        if frames != self.frames {
            self.frames = frames;
            shared.set_buffer_frames(frames as u32);
        }
        let max = cycle.max_frames();
        if frames > max {
            Shared::count(&shared.counters.split_callbacks, 1);
        }
        let now = shared.nanos(started);
        let mut done = 0;
        while done < frames {
            let count = (frames - done).min(max);
            match self.playback.as_mut() {
                Some(playback) => playback.read(cycle.inputs_mut(), count, shared),
                None => {
                    for input in cycle.inputs_mut() {
                        input[..count].fill(0.);
                    }
                }
            }
            cycle.render(count, now, done == 0);
            let block = &mut data[done * channels..(done + count) * channels];
            for (offset, frame) in block.chunks_exact_mut(channels).enumerate() {
                for (sample, output) in frame.iter_mut().zip(cycle.outputs()) {
                    *sample = output[offset];
                }
            }
            done += count;
        }
        data[frames * channels..].fill(0.);
        if frames > 0 {
            let period = frames as f32 / shared.sample_rate() as f32;
            shared.set_load(started.elapsed().as_secs_f32() / period);
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Some(cycle) = self.cycle.take() {
            let _ = self.back.try_send(cycle);
        }
    }
}

/// The streams of a running CPAL driver.
pub(crate) struct Streams {
    output: Option<Box<dyn StreamTrait + Send>>,
    input: Option<Box<dyn StreamTrait + Send>>,
    callbacks: Arc<crate::callback_gate::CallbackGate>,
    back: Receiver<Cycle>,
}

impl Streams {
    fn quiesce(&self) {
        self.callbacks.close();
        if let Some(output) = &self.output {
            let _ = output.pause();
        }
        if let Some(input) = &self.input {
            let _ = input.pause();
        }
        self.callbacks.wait();
    }
    pub fn stop(mut self) -> Option<Cycle> {
        self.quiesce();
        self.output.take();
        self.input.take();
        self.back.recv_timeout(RETURN_TIMEOUT).ok()
    }
}
impl Drop for Streams {
    fn drop(&mut self) {
        self.quiesce();
    }
}

/// Open the configured CPAL host, devices and midir ports and start rendering.
pub(crate) fn start(
    config: &CpalConfig,
    engine: &EngineDescription,
) -> Result<Started, StartError> {
    let host = select_host(config.host.as_deref())?;
    let mut warnings = Vec::new();
    let midi = if crate::audio_only(engine) {
        MidiDevices::default()
    } else {
        crate::open_midi(
            &config.client_name,
            &config.midi_inputs,
            &config.midi_outputs,
            &mut warnings,
        )
    };
    let mut started = start_cpal(&host, config, engine, midi)?;
    if let Some(running) = started.host.running.as_mut() {
        if let crate::DriverConfig::Cpal(config) = &mut running.configuration {
            config.host = Some(host.id().name().to_lowercase());
        }
        warnings.append(&mut running.warnings);
        running.warnings = warnings;
    }
    Ok(started)
}

/// Start rendering on `host`'s devices with the given MIDI ports. `config.host` and
/// the MIDI port names in `config` are not used.
pub fn start_cpal<H>(
    host: &H,
    config: &CpalConfig,
    engine: &EngineDescription,
    midi: MidiDevices,
) -> Result<Started, StartError>
where
    H: HostTrait,
    <H::Device as DeviceTrait>::Stream: Send + 'static,
{
    let midi = if crate::audio_only(engine) {
        MidiDevices::default()
    } else {
        midi
    };
    let mut warnings = Vec::new();
    if config.output_device == DeviceSelection::None {
        return Err(StartError::InvalidConfig(
            "the CPAL driver needs an output device",
        ));
    }
    let output = select_device(host, true, &config.output_device)?.ok_or(StartError::NoDevice {
        direction: "output",
        name: None,
    })?;
    let output_default = output.default_output_config().map_err(device_error)?;
    let sample_rate = config.sample_rate.unwrap_or(output_default.sample_rate());
    let buffer = buffer_size(
        config.buffer_frames,
        output_default.buffer_size(),
        engine.max_frames,
    )?;
    let output_channels = channels(
        output_default.channels(),
        config.output_channels,
        "output",
        &mut warnings,
    );
    let input = match select_device(host, false, &config.input_device)? {
        Some(device) => {
            let default = device.default_input_config().map_err(device_error)?;
            let channels = channels(
                default.channels(),
                config.input_channels,
                "input",
                &mut warnings,
            );
            Some((device, channels))
        }
        None if config.input_device == DeviceSelection::Default => {
            warnings.push("no default input device; audio inputs are absent".into());
            None
        }
        None => None,
    };
    if midi.inputs.len() > MAX_HOST_PORTS || midi.outputs.len() > MAX_HOST_PORTS {
        return Err(StartError::InvalidConfig("too many MIDI ports"));
    }
    let output_name = device_name(&output);
    let input_name = input.as_ref().map(|(device, _)| device_name(device));
    let opened = CpalConfig {
        host: None,
        output_device: DeviceSelection::Named(output_name.clone()),
        input_device: input_name
            .clone()
            .map_or(DeviceSelection::None, DeviceSelection::Named),
        sample_rate: Some(sample_rate),
        output_channels: Some(output_channels),
        input_channels: input.as_ref().map(|(_, channels)| *channels),
        midi_inputs: midi.inputs.iter().map(|port| port.name.clone()).collect(),
        midi_outputs: midi.outputs.iter().map(|port| port.name.clone()).collect(),
        ..config.clone()
    };
    let ports = HostPorts {
        audio_inputs: input
            .as_ref()
            .map(|(_, channels)| {
                (1..=*channels)
                    .map(|channel| {
                        port_name(
                            input_name.as_deref().unwrap_or_default(),
                            &format!(":capture_{channel}"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        audio_outputs: (1..=output_channels)
            .map(|channel| port_name(&output_name, &format!(":playback_{channel}")))
            .collect(),
        midi_inputs: midi
            .inputs
            .iter()
            .map(|port| port_name(&port.name, ""))
            .collect(),
        midi_outputs: midi
            .outputs
            .iter()
            .map(|port| port_name(&port.name, ""))
            .collect(),
    };
    let requested = match buffer {
        BufferSize::Fixed(frames) => Some(frames),
        BufferSize::Default => None,
    };
    let Prepared {
        description,
        endpoint,
        outbox,
        render,
        shared,
        layout,
    } = Prepared::new(engine, ports, DriverKind::Cpal, sample_rate, requested)?;
    let max_frames = description.max_frames;

    let mut connections: Vec<Box<dyn Any + Send>> = Vec::new();
    let mut receivers = Vec::new();
    for port in midi.inputs {
        let (sender, receiver) = input_bridge(&shared);
        match port.source.connect(sender) {
            Ok(connection) => {
                connections.push(connection);
                receivers.push(Some(receiver));
            }
            Err(error) => {
                warnings.push(format!("MIDI input {} is silent: {error}", port.name));
                receivers.push(None);
            }
        }
    }
    let mut senders = Vec::new();
    let mut sinks = Vec::new();
    for port in midi.outputs {
        let (sender, receiver) = output_bridge();
        senders.push(Some(sender));
        sinks.push((receiver, port.sink));
    }
    let cycle = Cycle::new(render, shared.clone(), &description, receivers, senders)
        .map_err(|error| StartError::Prepare(shoop_rt_engine::PrepareError::Runtime(error)))?;
    let pump = if sinks.is_empty() {
        None
    } else {
        Some(
            Pump::spawn(sinks, shared.clone())
                .map_err(|error| StartError::Thread(error.to_string()))?,
        )
    };

    let stream_config = |channels: u16| StreamConfig {
        channels,
        sample_rate,
        buffer_size: buffer,
    };
    let callbacks = Arc::new(crate::callback_gate::CallbackGate::default());
    let started = Arc::new(AtomicBool::new(false));
    let mut playback = None;
    let input_stream = match input {
        Some((device, channels)) => {
            let width = usize::from(channels);
            let (mut producer, consumer) = RingBuffer::new(BRIDGE_CALLBACKS * max_frames * width);
            let prefill = requested.map_or(max_frames, |frames| frames as usize) * width;
            if let Ok(chunk) = producer.write_chunk_uninit(prefill) {
                chunk.fill_from_iter(std::iter::repeat(0.));
            }
            let mut capture = Capture {
                callbacks: callbacks.clone(),
                producer,
                channels: width,
                started: started.clone(),
                shared: shared.clone(),
            };
            playback = Some(Playback {
                consumer,
                channels: width,
                started: started.clone(),
            });
            let errors = shared.clone();
            let stream = device
                .build_input_stream(
                    stream_config(channels),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| capture.push(data),
                    move |error| failure(&errors, error),
                    None,
                )
                .map_err(device_error)?;
            Some(stream)
        }
        None => None,
    };
    let (back, returned) = sync_channel(1);
    let mut side = Output {
        callbacks: callbacks.clone(),
        cycle: Some(cycle),
        playback,
        channels: usize::from(output_channels),
        frames: 0,
        shared: shared.clone(),
        back,
    };
    let errors = shared.clone();
    let output_stream = output
        .build_output_stream(
            stream_config(output_channels),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| side.process(data),
            move |error| failure(&errors, error),
            None,
        )
        .map_err(device_error)?;
    let streams = Streams {
        output: Some(Box::new(output_stream)),
        input: input_stream.map(|stream| Box::new(stream) as Box<dyn StreamTrait + Send>),
        callbacks,
        back: returned,
    };
    if let Some(stream) = &streams.input {
        stream.play().map_err(device_error)?;
    }
    streams
        .output
        .as_ref()
        .unwrap()
        .play()
        .map_err(device_error)?;
    shared.set_state(DriverRunState::Running);
    let period_frames = requested.unwrap_or(max_frames as u32);
    Ok(assemble(
        endpoint,
        outbox,
        Running {
            configuration: crate::DriverConfig::Cpal(opened),
            instance_name: "CPAL".to_owned(),
            reset_wait: reset_wait(period_frames, sample_rate).max(Duration::from_secs(2)),
            shared,
            driver: Driver::Cpal(streams),
            pump,
            connections,
            layout,
            warnings,
            dummy: None,
        },
    ))
}
