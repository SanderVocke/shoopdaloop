//! Native engine composition over shared audio drivers.
#![forbid(unsafe_code)]
use shoop_audio_drivers as drivers;
use shoop_rt_engine::{ControlEndpoint, EngineDescription, PrepareError, Render, Stopped};
use shoop_rt_protocol::{DriverKind, DriverStatus};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
};
use std::sync::Arc;
mod benchmark;
#[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
mod carla;
pub use drivers::DriverStatusReport as NativeStatus;
#[cfg(feature = "jack")]
pub use drivers::JackConfig;
#[cfg(feature = "cpal")]
pub use drivers::{
    audio_devices, cpal_hosts, discover, midi_ports, open_midi, preflight_cpal, preflight_cpal_on,
    resolve_cpal_devices, resolve_cpal_devices_on, AudioDevice, CpalConfig, DeviceSelection,
    Discovery,
};
pub use drivers::{
    DriverConfig, DummyConfig, DummyControl, DummyMode, HostLayout, RenderPhase, Stats, StopError,
    StreamFailure,
};
pub use drivers::{
    MidiDevices, MidiInputDevice, MidiInputSender, MidiOutputDevice, MidiSink, MidiSource,
    BRIDGE_MESSAGES, MAX_MESSAGE_BYTES,
};
pub use shoop_host_processors::support as processor_support;
pub type NativeClient = ProtocolClient<InProcess<ControlEndpoint>>;
#[derive(Clone, Debug)]
pub struct NativeConfig {
    pub driver: DriverConfig,
    pub engine: EngineDescription,
}
#[derive(Clone, Debug, Default)]
pub struct CarlaHostOptions {
    pub worker: Option<std::path::PathBuf>,
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    pub ui: Option<shoop_carla::native::CarlaMainThreadUiDispatcher>,
}
pub struct Started {
    pub host: NativeHost,
    pub client: NativeClient,
}
pub struct StartedEndpoint {
    pub host: NativeHost,
    pub endpoint: ControlEndpoint,
    pub outbox: Outbox,
}
pub struct NativeHost {
    inner: drivers::DriverHost,
}
pub struct StopReport {
    pub midi_reset: bool,
    pub phase: RenderPhase,
    pub stopped: Result<Stopped, StopError>,
    pub stats: Stats,
}
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StartError {
    #[error("the buffer of {requested} frames exceeds the prepared maximum of {max}")]
    BufferTooLarge { requested: u32, max: u32 },
    #[error("processor preparation failed: {0}")]
    Processors(shoop_host_processors::Error),
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

impl From<drivers::StartError> for StartError {
    fn from(error: drivers::StartError) -> Self {
        match error {
            drivers::StartError::Factory(error) => match error.downcast::<Self>() {
                Ok(error) => *error,
                Err(error) => Self::Thread(error.to_string()),
            },
            drivers::StartError::BufferTooLarge { requested, max } => {
                Self::BufferTooLarge { requested, max }
            }
            drivers::StartError::BufferUnsupported {
                requested,
                min,
                max,
            } => Self::BufferUnsupported {
                requested,
                min,
                max,
            },
            drivers::StartError::InvalidConfig(message) => Self::InvalidConfig(message),
            drivers::StartError::NoHost(name) => Self::NoHost(name),
            drivers::StartError::NoDevice { direction, name } => Self::NoDevice { direction, name },
            drivers::StartError::Device(message) => Self::Device(message),
            drivers::StartError::Prepare(error) => Self::Prepare(PrepareError::Runtime(error)),
            drivers::StartError::Thread(message) => Self::Thread(message),
        }
    }
}
impl From<PrepareError> for StartError {
    fn from(error: PrepareError) -> Self {
        Self::Prepare(error)
    }
}
impl NativeHost {
    pub fn graph_control(&self) -> Option<shoop_port_graph::Control> {
        self.inner.graph_control()
    }
    pub fn status(&self) -> NativeStatus {
        self.inner.status()
    }
    pub fn instance_name(&self) -> &str {
        self.inner.instance_name()
    }
    pub fn driver_config(&self) -> &DriverConfig {
        self.inner.driver_config()
    }
    pub fn layout(&self) -> &HostLayout {
        self.inner.layout()
    }
    pub fn warnings(&self) -> &[String] {
        self.inner.warnings()
    }
    pub fn dummy(&self) -> Option<&DummyControl> {
        self.inner.dummy()
    }
    pub fn stop(self) -> StopReport {
        let report = self.inner.stop();
        StopReport {
            midi_reset: report.midi_reset,
            phase: report.phase,
            stats: report.stats,
            stopped: report.stopped.and_then(|value| {
                value
                    .downcast::<Stopped>()
                    .map(|v| *v)
                    .map_err(|_| StopError::Lost)
            }),
        }
    }
}
fn assemble(started: drivers::Started) -> Result<Started, StartError> {
    let endpoint = *started
        .endpoint
        .downcast::<ControlEndpoint>()
        .map_err(|_| StartError::InvalidConfig("native factory endpoint mismatch"))?;
    Ok(Started {
        host: NativeHost {
            inner: started.host,
        },
        client: ProtocolClient::new(InProcess::new(endpoint, started.outbox)),
    })
}
pub fn start(config: &NativeConfig) -> Result<Started, StartError> {
    start_with_template(config, template(&config.engine))
}
fn start_with_template(
    config: &NativeConfig,
    engine: drivers::Template,
) -> Result<Started, StartError> {
    assemble(drivers::start(&drivers::DriverSettings {
        driver: config.driver.clone(),
        engine,
    })?)
}
pub fn start_endpoint(config: &NativeConfig) -> Result<StartedEndpoint, StartError> {
    split_endpoint(start(config)?)
}

pub fn start_endpoint_with_carla(
    config: &NativeConfig,
    options: CarlaHostOptions,
) -> Result<StartedEndpoint, StartError> {
    split_endpoint(start_with_carla(config, options)?)
}

pub fn start_with_carla(
    config: &NativeConfig,
    options: CarlaHostOptions,
) -> Result<Started, StartError> {
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    {
        let mut engine = template(&config.engine);
        engine.factory = Arc::new(NativeFactory {
            description: config.engine.clone(),
            carla: options,
        });
        start_with_template(config, engine)
    }
    #[cfg(not(all(feature = "carla", not(target_arch = "wasm32"))))]
    {
        let _ = options;
        start(config)
    }
}

fn split_endpoint(Started { host, client }: Started) -> Result<StartedEndpoint, StartError> {
    let (endpoint, outbox) = client.into_carrier().into_parts();
    Ok(StartedEndpoint {
        host,
        endpoint,
        outbox,
    })
}
#[cfg(feature = "cpal")]
pub fn start_cpal<H>(
    host: &H,
    config: &CpalConfig,
    engine: &EngineDescription,
    midi: MidiDevices,
) -> Result<Started, StartError>
where
    H: cpal::traits::HostTrait,
    <H::Device as cpal::traits::DeviceTrait>::Stream: Send + 'static,
{
    assemble(drivers::start_cpal(host, config, &template(engine), midi)?)
}
pub fn validate_processor_capacity(engine: &EngineDescription) -> Result<u64, StartError> {
    shoop_host_processors::reservation_bytes(engine).map_err(StartError::Processors)
}
pub fn template(engine: &EngineDescription) -> drivers::Template {
    drivers::Template {
        max_frames: engine.max_frames,
        jack_fixed_ports: false,
        jack_capacities: [
            engine.strips() * engine.strip_channels as usize,
            engine.strips() * engine.strip_channels as usize
                + engine.buses as usize * engine.bus_channels as usize,
            engine.strips(),
            engine.strips(),
        ],
        factory: Arc::new(NativeFactory {
            description: engine.clone(),
            #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
            carla: CarlaHostOptions::default(),
        }),
    }
}
struct NativeFactory {
    description: EngineDescription,
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    carla: CarlaHostOptions,
}
impl drivers::Factory for NativeFactory {
    fn prepare(&self, opened: drivers::Opened) -> Result<drivers::Parts, drivers::StartError> {
        prepare(self, opened).map_err(|e| drivers::StartError::Factory(Box::new(e)))
    }
}
fn describe(
    template: &EngineDescription,
    opened: drivers::Opened,
) -> Result<EngineDescription, StartError> {
    let ports = shoop_rt_engine::HostPorts {
        audio_inputs: opened.ports.audio_inputs,
        audio_outputs: opened.ports.audio_outputs,
        midi_inputs: opened.ports.midi_inputs,
        midi_outputs: opened.ports.midi_outputs,
    };
    let kind = opened.driver.kind;
    let sample_rate = opened.driver.sample_rate;
    let buffer = Some(opened.driver.buffer_frames);
    let outputs = opened.output_count;
    let max = u32::try_from(template.max_frames).unwrap_or(u32::MAX);
    // JACK splits server periods into bounded render blocks. Its actual
    // period still determines how far ahead control commands must arrive.
    if let Some(requested) = buffer.filter(|&frames| frames > max && kind != DriverKind::Jack) {
        return Err(StartError::BufferTooLarge { requested, max });
    }
    let buffer_frames = buffer.unwrap_or(max);
    let master_connections: Vec<(u8, u16)> = (0..usize::from(template.master_channels)
        .min(outputs))
        .map(|channel| (channel as u8, channel as u16))
        .collect();
    let mut description = EngineDescription {
        sample_rate,
        host: ports.clone(),
        master_connections: master_connections.clone(),
        control_lead_frames: template.control_lead_frames.max(buffer_frames),
        ..template.clone()
    };
    if let Ok(value) = std::env::var("SHOOP_AUDIO_RENDER") {
        description.audio_render_mode = shoop_rt_engine::AudioRenderMode::parse(&value)
            .map_err(|_| StartError::InvalidConfig("invalid SHOOP_AUDIO_RENDER mode"))?;
    }
    description.audio_bench_only |=
        std::env::var("SHOOP_AUDIO_BENCH_AUDIO_ONLY").as_deref() == Ok("1");
    if description.audio_render_mode != shoop_rt_engine::AudioRenderMode::Current
        && !description.audio_bench_only
    {
        return Err(StartError::InvalidConfig(
            "experimental render modes require SHOOP_AUDIO_BENCH_AUDIO_ONLY=1",
        ));
    }

    if description.audio_bench_only {
        description.host.midi_inputs.clear();
        description.host.midi_outputs.clear();
    }
    if std::env::var_os("SHOOP_AUDIO_RENDER").is_some() {
        eprintln!("audio_render={} driver={kind:?} rate={sample_rate} quantum={} callback={buffer_frames} audio_only={} tracks={} loops={}",
                description.audio_render_mode.name(), description.quantum, description.audio_bench_only,
                description.strips(), description.loops());
    }
    // These drivers deliver bounded render blocks within the physical period.
    if matches!(kind, DriverKind::Dummy | DriverKind::Jack) {
        let minimum = template.quantum.min(template.max_frames);
        while description.max_frames > minimum && description.midi_output_capacity().is_err() {
            description.max_frames = (description.max_frames / 2).max(minimum);
        }
    }

    Ok(description)
}
fn prepare(factory: &NativeFactory, opened: drivers::Opened) -> Result<drivers::Parts, StartError> {
    let driver = opened.driver;
    let description = describe(&factory.description, opened)?;
    let master_connections = description.master_connections.clone();
    let processors =
        shoop_host_processors::prepare(&description).map_err(StartError::Processors)?;
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    let carla_first_slot = processors.instances.len();
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    let (processors, carla_services) = carla::prepare(
        &description,
        driver.buffer_frames as usize,
        &factory.carla,
        processors,
    );
    let mut engine = shoop_rt_engine::prepare_with_processors(&description, driver, processors)?;
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    if !carla_services.is_empty() {
        let controls = carla::Controls::new(carla_first_slot, &carla_services);
        engine
            .endpoint
            .install_hosted_processor_controls(Box::new(controls));
    }
    engine
        .render
        .set_profiling_clock(drivers::profiling_clock());
    if let Some(control) = engine.host.profiling() {
        control.use_outer_callback_budget();
    }
    let outbox = engine.outbox();
    let capacity = description
        .midi_output_capacity()
        .map_err(PrepareError::Runtime)?;
    let benchmark = benchmark::Recorder::prepare(&description)
        .map_err(|e| StartError::Thread(format!("benchmark writer: {e}")))?;
    Ok(drivers::Parts {
        buffers: drivers::BufferConfig {
            host: drivers::HostPorts {
                audio_inputs: description.host.audio_inputs,
                audio_outputs: description.host.audio_outputs,
                midi_inputs: description.host.midi_inputs,
                midi_outputs: description.host.midi_outputs,
            },
            max_frames: description.max_frames,
            midi_events: description.midi_events,
            midi_bytes: description.midi_bytes,
            output_midi_events: capacity.events,
            output_midi_bytes: capacity.bytes,
        },
        renderer: Box::new(NativeRenderer(
            engine.render,
            #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
            carla_services,
        )),
        observer: Arc::new(NativeObserver {
            host: engine.host,
            benchmark,
        }),
        endpoint: Box::new(engine.endpoint),
        outbox,
        master_connections,
    })
}
pub struct NativeRenderer(
    pub Render,
    #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
    Vec<shoop_carla::service::ChainService>,
);
impl rt_runtime::host::Renderer for NativeRenderer {
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
impl drivers::Processor for NativeRenderer {
    fn finish(self: Box<Self>) -> Result<Box<dyn std::any::Any + Send>, rt_runtime::Error> {
        let mut stopped = self.0.into_stopped();
        stopped.cleanup()?;
        #[cfg(all(feature = "carla", not(target_arch = "wasm32")))]
        for service in self.1 {
            let _ = service.stop(std::time::Duration::from_secs(2));
        }
        Ok(Box::new(stopped))
    }
    fn diagnostic_work(&self) -> [u64; 14] {
        let work = self.0.resource_work().1;
        let diagnostics = self.0.diagnostics();
        let graph = self.0.audio_graph_work();
        let pool = self.0.audio_pool_work();
        [
            work.copied_bytes as u64,
            work.validation_steps as u64,
            diagnostics.nodes_processed as u64,
            u64::from(matches!(self.0.phase(), shoop_rt_engine::Phase::Faulted(_))),
            graph.forwarded,
            graph.materialized,
            graph.sample_reads,
            graph.sample_writes,
            graph.arena_peak_samples,
            pool.index_allocated,
            pool.index_reclaimed,
            pool.shared_payloads,
            pool.payload_live,
            pool.index_live,
        ]
    }
}
struct NativeObserver {
    host: shoop_rt_engine::HostStatus,
    benchmark: Option<benchmark::Recorder>,
}
impl drivers::Observer for NativeObserver {
    fn driver(&self, driver: DriverStatus) {
        self.host.set_driver(driver);
    }
    fn xrun(&self) {
        self.host.add_xruns(1);
    }
    fn begin(&self, frame: u64, frames: usize, now: u64) {
        if let Some(c) = self.host.profiling().filter(|c| c.recording()) {
            c.host_begin(frame, frames as u32, now);
        }
    }
    fn input(&self, now: u64) {
        if let Some(c) = self.host.profiling().filter(|c| c.recording()) {
            c.host_input(now);
        }
    }
    fn callback_complete(&self, sample: shoop_rt_protocol::DspMeasurement, now: u64) {
        if let Some(c) = self.host.profiling().filter(|c| c.recording()) {
            if let shoop_rt_protocol::DspMeasurement::Callback {
                frames,
                sample_rate,
                ..
            } = sample
            {
                c.host_end(
                    now,
                    u64::from(frames) * 1_000_000_000 / u64::from(sample_rate),
                );
            }
        }
        self.host.record_dsp(sample);
    }
    fn record(&self, v: [u64; 18]) {
        if let Some(b) = &self.benchmark {
            b.record(v);
        }
    }
    fn recording(&self) -> bool {
        self.benchmark.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test(no_wasm = "native JACK preparation policy")]
    fn jack_server_buffer_sets_control_lead_even_when_rendered_in_smaller_parts() {
        let template = EngineDescription {
            max_frames: 64,
            ..Default::default()
        };
        let opened = |kind| drivers::Opened {
            ports: drivers::HostPorts::numbered(2, 2, 1),
            driver: DriverStatus {
                kind,
                state: shoop_rt_protocol::DriverRunState::Starting,
                sample_rate: 48000,
                buffer_frames: 128,
            },
            output_count: 2,
        };
        let prepared = describe(&template, opened(DriverKind::Jack)).unwrap();
        assert_eq!(prepared.max_frames, 64);
        assert_eq!(prepared.control_lead_frames, 128);
        assert!(describe(&template, opened(DriverKind::Dummy)).is_err());
    }
    #[shoop_test(no_wasm = "native host ownership")]
    fn the_host_handle_can_move_between_threads() {
        fn send<T: Send>() {}
        send::<NativeHost>();
        send::<NativeClient>();
    }
}
