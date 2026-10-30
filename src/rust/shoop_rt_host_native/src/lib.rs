//! The native host of the new engine backend.
//!
//! The host opens a driver, prepares the engine for the opened device and renders it
//! on the driver's real-time thread. It contains no engine or application semantics:
//! [`start`] returns the [`NativeHost`], which owns the render side, and a protocol
//! client over the in-process carrier, which owns the control endpoint and runs on
//! the caller's thread.
//!
//! Drivers:
//! - the dummy driver renders on its own thread, at the buffer period or only the
//!   frames a caller requests;
//! - with the `cpal` feature, a CPAL output stream renders the engine in its
//!   callback, a separate input stream feeds it through a lock-free bridge, and
//!   midir ports exchange MIDI with the callback through bounded bridges.
//!
//! Changing the device, sample rate or buffer size is a lifecycle operation: stop the
//! host, then start another.
#![forbid(unsafe_code)]
// Without device drivers, the MIDI bridges and parts of the callback are unused.
#![cfg_attr(not(feature = "cpal"), allow(dead_code))]

#[cfg(feature = "cpal")]
mod cpal_driver;
mod cycle;
mod dummy;
mod midi;
#[cfg(feature = "cpal")]
mod midir_ports;
mod status;

use std::any::Any;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use shoop_rt_engine::{
    prepare, ControlEndpoint, EngineDescription, HostPorts, PrepareError, Render, Stopped,
};
use shoop_rt_protocol::carrier::{InProcess, Outbox};
use shoop_rt_protocol::client::ProtocolClient;
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus, Name, MAX_NAME_BYTES};

#[cfg(feature = "cpal")]
pub use cpal_driver::{
    audio_devices, cpal_hosts, discover, start_cpal, AudioDevice, CpalConfig, DeviceSelection,
    Discovery,
};
pub use dummy::{DummyConfig, DummyControl, DummyMode};
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

/// A protocol client of a native host's engine.
pub type NativeClient = ProtocolClient<InProcess<ControlEndpoint>>;

#[derive(Clone, Debug)]
pub enum DriverConfig {
    Dummy(DummyConfig),
    #[cfg(feature = "cpal")]
    Cpal(CpalConfig),
}

#[derive(Clone, Debug)]
pub struct NativeConfig {
    pub driver: DriverConfig,
    /// The engine to prepare. The host sets the device's part: the sample rate, the
    /// host ports, Master's initial connections to the first outputs (reported as
    /// preferred) and a control lead of at least one buffer. `max_frames` is the
    /// prepared maximum; the driver's buffer must not exceed it.
    pub engine: EngineDescription,
}

/// The host ports of an opened driver, as given to the engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostLayout {
    pub ports: HostPorts,
    /// Master channel to host audio output, the outputs reported as preferred.
    pub master_connections: Vec<(u8, u16)>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StartError {
    #[error("the buffer of {requested} frames exceeds the prepared maximum of {max}")]
    BufferTooLarge { requested: u32, max: u32 },
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
    Prepare(PrepareError),
    #[error("could not start a host thread: {0}")]
    Thread(String),
}

impl From<PrepareError> for StartError {
    fn from(error: PrepareError) -> Self {
        Self::Prepare(error)
    }
}

/// A started host and the client of its engine. Connect the client and pump it
/// from the application thread.
pub struct Started {
    pub host: NativeHost,
    pub client: NativeClient,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeStatus {
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
    pub stopped: Result<Stopped, StopError>,
    pub stats: Stats,
}

/// A running native host. Dropping it stops it.
pub struct NativeHost {
    running: Option<Running>,
}

pub(crate) enum Driver {
    Dummy(dummy::DummyThread),
    #[cfg(feature = "cpal")]
    Cpal(cpal_driver::Streams),
}

impl Driver {
    fn stop(self) -> Option<Cycle> {
        match self {
            Self::Dummy(thread) => thread.stop(),
            #[cfg(feature = "cpal")]
            Self::Cpal(streams) => streams.stop(),
        }
    }
}

pub(crate) struct Running {
    pub shared: Arc<Shared>,
    pub driver: Driver,
    pub pump: Option<Pump>,
    pub connections: Vec<Box<dyn Any + Send>>,
    pub layout: HostLayout,
    pub warnings: Vec<String>,
    pub reset_wait: Duration,
    pub dummy: Option<DummyControl>,
}

impl Running {
    fn stop(self) -> StopReport {
        let _span = tracing::info_span!("rt_host.native.stop").entered();
        let shared = self.shared;
        shared.stopping.store(true, Ordering::Release);
        let deadline = Instant::now() + self.reset_wait;
        while shared.phase() == RenderPhase::Running
            && shared.driver().state != DriverRunState::Failed
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        let midi_reset = shared.phase() != RenderPhase::Running;
        let cycle = self.driver.stop();
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

fn cleanup(render: Render) -> Result<Stopped, StopError> {
    let mut stopped = render.into_stopped();
    stopped.cleanup().map_err(StopError::Cleanup)?;
    Ok(stopped)
}

impl NativeHost {
    fn running(&self) -> &Running {
        self.running
            .as_ref()
            .expect("a host is running until it is stopped")
    }

    pub fn status(&self) -> NativeStatus {
        let shared = &self.running().shared;
        NativeStatus {
            driver: shared.driver(),
            phase: shared.phase(),
            next_frame: shared.next_frame(),
            failure: shared.failure(),
            stats: shared.stats(),
        }
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

impl Drop for NativeHost {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop();
        }
    }
}

/// Open the configured driver and start rendering.
pub fn start(config: &NativeConfig) -> Result<Started, StartError> {
    let _span = tracing::info_span!("rt_host.native.start").entered();
    let started = match &config.driver {
        DriverConfig::Dummy(dummy) => dummy::start(dummy, &config.engine),
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(cpal) => cpal_driver::start(cpal, &config.engine),
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

/// An engine prepared for an opened driver.
pub(crate) struct Prepared {
    pub description: EngineDescription,
    pub endpoint: ControlEndpoint,
    pub outbox: Outbox,
    pub render: Render,
    pub shared: Arc<Shared>,
    pub layout: HostLayout,
}

impl Prepared {
    /// `buffer` is the driver's requested callback size, if it has one.
    pub fn new(
        template: &EngineDescription,
        ports: HostPorts,
        kind: DriverKind,
        sample_rate: u32,
        buffer: Option<u32>,
    ) -> Result<Self, StartError> {
        let max = u32::try_from(template.max_frames).unwrap_or(u32::MAX);
        if let Some(requested) = buffer.filter(|&frames| frames > max) {
            return Err(StartError::BufferTooLarge { requested, max });
        }
        let buffer_frames = buffer.unwrap_or(max);
        let outputs = ports.audio_outputs.len();
        let master_connections: Vec<(u8, u16)> = (0..usize::from(template.master_channels)
            .min(outputs))
            .map(|channel| (channel as u8, channel as u16))
            .collect();
        let description = EngineDescription {
            sample_rate,
            host: ports.clone(),
            master_connections: master_connections.clone(),
            control_lead_frames: template.control_lead_frames.max(buffer_frames),
            ..template.clone()
        };
        let driver = DriverStatus {
            kind,
            state: DriverRunState::Starting,
            sample_rate,
            buffer_frames,
        };
        let engine = prepare(&description, driver)?;
        let outbox = engine.outbox();
        let shared = Arc::new(Shared::new(engine.host, driver));
        Ok(Self {
            description,
            endpoint: engine.endpoint,
            outbox,
            render: engine.render,
            shared,
            layout: HostLayout {
                ports,
                master_connections,
            },
        })
    }
}

pub(crate) fn assemble(endpoint: ControlEndpoint, outbox: Outbox, running: Running) -> Started {
    Started {
        host: NativeHost {
            running: Some(running),
        },
        client: ProtocolClient::new(InProcess::new(endpoint, outbox)),
    }
}

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

#[cfg(test)]
mod tests {
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

    #[shoop_test(no_wasm = "the native host does not build for wasm")]
    fn the_host_handle_can_move_between_threads() {
        fn send<T: Send>() {}
        send::<super::NativeHost>();
        send::<super::NativeClient>();
    }
}
