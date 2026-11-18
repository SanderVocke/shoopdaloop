#![forbid(unsafe_code)]
// Without device drivers, the MIDI bridges and parts of the callback are unused.
#![cfg_attr(not(feature = "cpal"), allow(dead_code))]

#[cfg(any(feature = "cpal", test))]
mod callback_gate;
#[cfg(feature = "cpal")]
mod cpal_driver;
mod cycle;
mod dummy;
#[cfg(feature = "jack")]
mod jack_driver;
#[cfg(feature = "jack")]
mod jack_graph;
#[cfg(feature = "jack")]
mod jack_ports;
#[cfg(feature = "jack")]
mod jack_publication;
mod midi;
#[cfg(feature = "cpal")]
mod midir_ports;
mod status;

use std::any::Any;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rt_runtime::host::Renderer;

use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus, Name, MAX_NAME_BYTES};
use shoop_rt_transport::carrier::Outbox;

#[cfg(feature = "cpal")]
pub use cpal_driver::{
    audio_devices, cpal_hosts, discover, preflight_cpal, preflight_cpal_on, resolve_cpal_devices,
    resolve_cpal_devices_on, start_cpal, AudioDevice, CpalConfig, DeviceSelection, Discovery,
};
pub use dummy::{DummyConfig, DummyControl, DummyMode};
#[cfg(feature = "jack")]
pub use jack_driver::JackConfig;
pub use midi::{
    MidiDevices, MidiInputDevice, MidiInputSender, MidiOutputDevice, MidiSink, MidiSource,
    BRIDGE_MESSAGES, MAX_MESSAGE_BYTES,
};
#[cfg(feature = "cpal")]
pub use midir_ports::{midi_ports, open_midi};
pub use status::{RenderPhase, Stats, StreamFailure};

use crate::cycle::Cycle;
use crate::midi::Pump;
use crate::status::Shared;

#[derive(Clone, Debug)]
pub enum DriverConfig {
    Dummy(DummyConfig),
    #[cfg(feature = "cpal")]
    Cpal(CpalConfig),
    #[cfg(feature = "jack")]
    Jack(JackConfig),
}

impl DriverConfig {
    pub fn max_frames_hint(&self) -> usize {
        match self {
            Self::Dummy(d) => d.buffer_frames as usize,
            #[cfg(feature = "cpal")]
            Self::Cpal(c) => c.buffer_frames.unwrap_or(1024) as usize,
            #[cfg(feature = "jack")]
            Self::Jack(_) => 1024,
        }
    }
}
#[derive(Clone, Debug)]
pub struct DriverSettings {
    pub driver: DriverConfig,
    /// The engine to prepare. The host sets the device's part: the sample rate, the
    /// host ports, Master's initial connections to the first outputs (reported as
    /// preferred) and a control lead of at least one buffer. `max_frames` is the
    /// prepared maximum; drivers that accept larger buffers render them in parts.
    pub engine: Template,
}

/// The host ports of an opened driver, as given to the engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostLayout {
    pub ports: HostPorts,
    /// Master channel to host audio output, the outputs reported as preferred.
    pub master_connections: Vec<(u8, u16)>,
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("the buffer of {requested} frames exceeds the prepared maximum of {max}")]
    BufferTooLarge { requested: u32, max: u32 },
    #[error("renderer preparation failed: {0}")]
    Factory(Box<dyn std::error::Error + Send + Sync>),
    #[error("the device does not support {requested} frames per buffer ({min} to {max})")]
    BufferUnsupported { requested: u32, min: u32, max: u32 },
    #[error("invalid driver configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("no audio host named {0}")]
    NoHost(String),
    #[error("no {direction} device{}", name.as_ref().map(|n| format!(" named {n}")).unwrap_or_default())]
    NoDevice {
        direction: &'static str,
        name: Option<String>,
    },
    #[error("audio device error: {0}")]
    Device(String),
    #[error("engine preparation failed: {0:?}")]
    Prepare(rt_runtime::Error),
    #[error("could not start a host thread: {0}")]
    Thread(String),
}

/// A started host and the client of its engine. Connect the client and pump it
/// from the application thread.
pub struct Started {
    pub host: DriverHost,
    pub endpoint: Box<dyn Any + Send>,
    pub outbox: Outbox,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriverStatusReport {
    pub driver: DriverStatus,
    pub phase: RenderPhase,
    /// The next absolute frame the engine renders.
    pub next_frame: u64,
    pub failure: Option<StreamFailure>,
    pub stats: Stats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StopError {
    #[error("the driver did not return the render owner")]
    Lost,
    #[error("stopped cleanup failed: {0:?}")]
    Cleanup(rt_runtime::Error),
}

pub struct StopReport {
    /// The render owner sent its MIDI reset before callbacks stopped. This needs the
    /// client, and with it the control endpoint, to be dropped before stopping.
    pub midi_reset: bool,
    pub phase: RenderPhase,
    /// The cleaned-up engine, for inspection or release off the audio thread.
    pub stopped: Result<Box<dyn Any + Send>, StopError>,
    pub stats: Stats,
}

/// A running native host. Dropping it stops it.
pub struct DriverHost {
    running: Option<Running>,
}

pub(crate) enum Driver {
    Dummy(dummy::DummyThread),
    #[cfg(feature = "cpal")]
    Cpal(cpal_driver::Streams),
    #[cfg(feature = "jack")]
    Jack(jack_driver::Client),
}

impl Driver {
    fn stop(self) -> Option<Cycle> {
        match self {
            Self::Dummy(thread) => thread.stop(),
            #[cfg(feature = "cpal")]
            Self::Cpal(streams) => streams.stop(),
            #[cfg(feature = "jack")]
            Self::Jack(client) => client.stop(),
        }
    }
}

pub(crate) struct Running {
    pub shared: Arc<Shared>,
    pub configuration: DriverConfig,
    pub driver: Driver,
    pub pump: Option<Pump>,
    pub connections: Vec<Box<dyn Any + Send>>,
    pub layout: HostLayout,
    pub warnings: Vec<String>,
    pub reset_wait: Duration,
    pub dummy: Option<DummyControl>,
    pub instance_name: String,
}

impl Running {
    fn stop(self) -> StopReport {
        let _span = tracing::info_span!("rt_host.native.stop").entered();
        let shared = self.shared;
        shared.stopping.store(true, Ordering::Release);
        shared.host.stopping();
        let deadline = Instant::now() + self.reset_wait;
        while shared.host.requires_reset()
            && shared.phase() == RenderPhase::Running
            && shared.driver().state != DriverRunState::Failed
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        let cycle = self.driver.stop();
        // The final callback may finish while joining the driver.
        let mut midi_reset =
            !shared.host.requires_reset() || shared.phase() != RenderPhase::Running;
        if shared.host.requires_reset() && shared.driver().kind == DriverKind::Jack {
            midi_reset &= shared.reset_flushed.load(Ordering::Acquire)
                && shared.failure().is_none()
                && shared.stats().midi_out_failed == 0;
        }
        drop(self.pump);
        drop(self.connections);
        shared.set_state(DriverRunState::Stopped);
        let stopped = match cycle {
            Some(cycle) => {
                let (render, _) = cycle.into_parts();
                cleanup(render)
            }
            None => Err(StopError::Lost),
        };
        let stats = shared.stats();
        if !midi_reset {
            tracing::warn!("native host stopped without a MIDI reset; drop the client first");
        }
        if let Err(error) = &stopped {
            tracing::warn!(%error, "native host stop");
        }
        tracing::info!(
            callbacks = stats.callbacks,
            xruns = stats.xruns(),
            "native host stopped"
        );
        StopReport {
            midi_reset,
            phase: shared.phase(),
            stopped,
            stats,
        }
    }
}

fn cleanup(render: Box<dyn Processor>) -> Result<Box<dyn Any + Send>, StopError> {
    render.finish().map_err(StopError::Cleanup)
}

impl DriverHost {
    pub fn graph_control(&self) -> Option<shoop_port_graph::Control> {
        match &self.running.as_ref()?.driver {
            #[cfg(feature = "jack")]
            Driver::Jack(client) => Some(client.graph.clone()),
            _ => None,
        }
    }
    fn running(&self) -> &Running {
        self.running
            .as_ref()
            .expect("a host is running until it is stopped")
    }

    pub fn status(&self) -> DriverStatusReport {
        let shared = &self.running().shared;
        DriverStatusReport {
            driver: shared.driver(),
            phase: shared.phase(),
            next_frame: shared.next_frame(),
            failure: shared.failure(),
            stats: shared.stats(),
        }
    }

    pub fn instance_name(&self) -> &str {
        &self.running().instance_name
    }

    /// The opened configuration, retaining resolved devices and automatic-buffer policy.
    pub fn driver_config(&self) -> &DriverConfig {
        &self.running().configuration
    }

    pub fn layout(&self) -> &HostLayout {
        &self.running().layout
    }

    /// Settings that could not be honoured as given, such as missing MIDI ports.
    pub fn warnings(&self) -> &[String] {
        &self.running().warnings
    }

    /// Frame requests of a dummy driver.
    pub fn dummy(&self) -> Option<&DummyControl> {
        self.running().dummy.as_ref()
    }

    /// Stop callbacks and clean up the engine off the audio thread. Drop the client
    /// first: the render owner then silences output and sends a MIDI reset, which
    /// reaches the devices before the streams close.
    pub fn stop(mut self) -> StopReport {
        self.running.take().expect("stopped once").stop()
    }
}

impl Drop for DriverHost {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop();
        }
    }
}

/// Open the configured driver and start rendering.
pub fn start(config: &DriverSettings) -> Result<Started, StartError> {
    let _span = tracing::info_span!("rt_host.native.start").entered();
    let started = match &config.driver {
        DriverConfig::Dummy(dummy) => dummy::start(dummy, &config.engine),
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(cpal) => cpal_driver::start(cpal, &config.engine),
        #[cfg(feature = "jack")]
        DriverConfig::Jack(jack) => jack_driver::start(jack, &config.engine),
    };
    match &started {
        Ok(started) => {
            let status = started.host.status();
            tracing::info!(
                driver = ?status.driver.kind,
                sample_rate = status.driver.sample_rate,
                buffer_frames = status.driver.buffer_frames,
                warnings = started.host.warnings().len(),
                "native host started"
            );
            for warning in started.host.warnings() {
                tracing::warn!(%warning, "native host");
            }
        }
        Err(error) => tracing::warn!(%error, "native host failed to start"),
    }
    started
}

pub const MAX_HOST_PORTS: usize = 1024;

pub trait Processor: Renderer + Send {
    fn finish(self: Box<Self>) -> Result<Box<dyn Any + Send>, rt_runtime::Error>;
    fn diagnostic_work(&self) -> [u64; 14] {
        [0; 14]
    }
}
pub trait Observer: Send + Sync {
    fn stopping(&self) {}
    fn requires_reset(&self) -> bool {
        true
    }
    fn driver(&self, _driver: DriverStatus) {}
    fn xrun(&self) {}
    fn callback_complete(&self, _sample: shoop_rt_protocol::DspMeasurement, _now: u64) {}
    fn begin(&self, _frame: u64, _frames: usize, _now: u64) {}
    fn input(&self, _now: u64) {}
    fn record(&self, _values: [u64; 18]) {}
    fn recording(&self) -> bool {
        false
    }
}
impl Observer for () {}

#[derive(Clone)]
pub struct Template {
    pub max_frames: usize,
    pub jack_capacities: [usize; 4],
    pub jack_fixed_ports: bool,
    pub factory: Arc<dyn Factory>,
}
impl std::fmt::Debug for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Template")
            .field("max_frames", &self.max_frames)
            .field("jack_capacities", &self.jack_capacities)
            .finish_non_exhaustive()
    }
}
pub struct Opened {
    pub ports: HostPorts,
    pub driver: DriverStatus,
    pub output_count: usize,
}
pub trait Factory: Send + Sync {
    fn prepare(&self, opened: Opened) -> Result<Parts, StartError>;
}
#[derive(Clone, Debug)]
pub struct BufferConfig {
    pub host: HostPorts,
    pub max_frames: usize,
    pub midi_events: usize,
    pub midi_bytes: usize,
    pub output_midi_events: usize,
    pub output_midi_bytes: usize,
}
pub struct MidiCapacity {
    pub events: usize,
    pub bytes: usize,
}
impl BufferConfig {
    pub fn midi_output_capacity(&self) -> Result<MidiCapacity, rt_runtime::Error> {
        Ok(MidiCapacity {
            events: self.output_midi_events,
            bytes: self.output_midi_bytes,
        })
    }
}
pub struct Parts {
    pub buffers: BufferConfig,
    pub renderer: Box<dyn Processor>,
    pub observer: Arc<dyn Observer>,
    pub endpoint: Box<dyn Any + Send>,
    pub outbox: Outbox,
    pub master_connections: Vec<(u8, u16)>,
}
pub(crate) struct Prepared {
    pub description: BufferConfig,
    pub endpoint: Box<dyn Any + Send>,
    pub outbox: Outbox,
    pub render: Box<dyn Processor>,
    pub shared: Arc<Shared>,
    pub layout: HostLayout,
}
impl Prepared {
    pub fn new(
        template: &Template,
        ports: HostPorts,
        kind: DriverKind,
        rate: u32,
        buffer: Option<u32>,
    ) -> Result<Self, StartError> {
        let outputs = ports.audio_outputs.len();
        Self::with_output_count(template, ports, kind, rate, buffer, outputs)
    }
    pub fn with_output_count(
        template: &Template,
        ports: HostPorts,
        kind: DriverKind,
        sample_rate: u32,
        buffer: Option<u32>,
        output_count: usize,
    ) -> Result<Self, StartError> {
        let driver = DriverStatus {
            kind,
            state: DriverRunState::Starting,
            sample_rate,
            buffer_frames: buffer.unwrap_or(template.max_frames as u32),
        };
        let parts = template.factory.prepare(Opened {
            ports: ports.clone(),
            driver,
            output_count,
        })?;
        let shared = Arc::new(Shared::new(parts.observer, driver));
        Ok(Self {
            description: parts.buffers,
            endpoint: parts.endpoint,
            outbox: parts.outbox,
            render: parts.renderer,
            shared,
            layout: HostLayout {
                ports,
                master_connections: parts.master_connections,
            },
        })
    }
}
pub(crate) fn assemble(endpoint: Box<dyn Any + Send>, outbox: Outbox, running: Running) -> Started {
    Started {
        host: DriverHost {
            running: Some(running),
        },
        endpoint,
        outbox,
    }
}
pub use status::{profiling_clock, profiling_now};

/// How long stopping waits for the render owner to send its MIDI reset.
pub(crate) fn reset_wait(buffer_frames: u32, sample_rate: u32) -> Duration {
    let period = Duration::from_secs_f64(f64::from(buffer_frames) / f64::from(sample_rate.max(1)));
    (4 * period).max(Duration::from_millis(50))
}

/// `base` followed by `suffix`, with `base` shortened to fit a port name.
pub(crate) fn port_name(base: &str, suffix: &str) -> Name {
    let mut end = base.len().min(MAX_NAME_BYTES.saturating_sub(suffix.len()));
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    Name::new(&format!("{}{suffix}", &base[..end]))
        .unwrap_or_else(|| Name::new("").expect("empty name"))
}

/// Names of the host's fixed port slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPorts {
    pub audio_inputs: Vec<Name>,
    pub audio_outputs: Vec<Name>,
    pub midi_inputs: Vec<Name>,
    pub midi_outputs: Vec<Name>,
}

impl HostPorts {
    /// Numbered ports such as `audio_in_1`.
    pub fn numbered(audio_inputs: usize, audio_outputs: usize, midi: usize) -> Self {
        let names = |prefix: &str, count: usize| {
            (0..count)
                .map(|index| Name::new(&format!("{prefix}_{}", index + 1)).expect("short name"))
                .collect()
        };
        Self {
            audio_inputs: names("audio_in", audio_inputs),
            audio_outputs: names("audio_out", audio_outputs),
            midi_inputs: names("midi_in", midi),
            midi_outputs: names("midi_out", midi),
        }
    }
}

#[cfg(test)]
fn test_buffers(description: &shoop_rt_engine::EngineDescription) -> BufferConfig {
    let capacity = description.midi_output_capacity().unwrap();
    BufferConfig {
        host: HostPorts {
            audio_inputs: description.host.audio_inputs.clone(),
            audio_outputs: description.host.audio_outputs.clone(),
            midi_inputs: description.host.midi_inputs.clone(),
            midi_outputs: description.host.midi_outputs.clone(),
        },
        max_frames: description.max_frames,
        midi_events: description.midi_events,
        midi_bytes: description.midi_bytes,
        output_midi_events: capacity.events,
        output_midi_bytes: capacity.bytes,
    }
}

#[cfg(test)]
struct TestRender(shoop_rt_engine::Render);
#[cfg(test)]
impl rt_runtime::host::Renderer for TestRender {
    fn next_frame(&self) -> rt_runtime::Frame {
        self.0.next_frame()
    }
    fn process(
        &mut self,
        audio: rt_runtime::buffer::AudioBlock<'_, '_>,
        midi: rt_runtime::buffer::MidiBlock<'_>,
    ) -> Result<rt_runtime::host::Report, rt_runtime::Error> {
        self.0.process(audio, midi)
    }
    fn midi_recovery(&self, port: usize) -> Option<&rt_runtime::buffer::MidiBuffer> {
        self.0.midi_recovery(port)
    }
}
#[cfg(test)]
impl Processor for TestRender {
    fn finish(self: Box<Self>) -> Result<Box<dyn Any + Send>, rt_runtime::Error> {
        let mut stopped = self.0.into_stopped();
        stopped.cleanup()?;
        Ok(Box::new(stopped))
    }
}
#[cfg(test)]
fn test_template(description: &shoop_rt_engine::EngineDescription) -> Template {
    struct Fixture(shoop_rt_engine::EngineDescription);
    impl Factory for Fixture {
        fn prepare(&self, opened: Opened) -> Result<Parts, StartError> {
            let p = opened.ports;
            let mut description = shoop_rt_engine::EngineDescription {
                master_connections: (0..usize::from(self.0.master_channels)
                    .min(opened.output_count))
                    .map(|c| (c as u8, c as u16))
                    .collect(),
                sample_rate: opened.driver.sample_rate,
                host: shoop_rt_engine::HostPorts {
                    audio_inputs: p.audio_inputs,
                    audio_outputs: p.audio_outputs,
                    midi_inputs: p.midi_inputs,
                    midi_outputs: p.midi_outputs,
                },
                ..self.0.clone()
            };
            while description.midi_output_capacity().is_err()
                && description.max_frames > description.quantum
            {
                description.max_frames = (description.max_frames / 2).max(description.quantum);
            }
            let engine = shoop_rt_engine::prepare(&description, opened.driver)
                .map_err(|e| StartError::Thread(format!("fixture preparation: {e:?}")))?;
            let outbox = engine.outbox();
            Ok(Parts {
                buffers: test_buffers(&description),
                renderer: Box::new(TestRender(engine.render)),
                observer: Arc::new(()),
                endpoint: Box::new(engine.endpoint),
                outbox,
                master_connections: Vec::new(),
            })
        }
    }
    Template {
        max_frames: description.max_frames,
        jack_capacities: [0; 4],
        jack_fixed_ports: false,
        factory: Arc::new(Fixture(description.clone())),
    }
}

#[cfg(test)]
mod name_tests {
    use super::port_name;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test(no_wasm = "the native host does not build for wasm")]
    fn long_device_names_are_shortened_to_keep_the_channel() {
        let name = port_name(&"é".repeat(40), ":playback_1");
        assert!(name.as_str().ends_with(":playback_1"));
        assert!(name.as_str().len() <= shoop_rt_protocol::MAX_NAME_BYTES);
        assert_eq!(
            port_name("Speakers", ":playback_2").as_str(),
            "Speakers:playback_2"
        );
    }
}
