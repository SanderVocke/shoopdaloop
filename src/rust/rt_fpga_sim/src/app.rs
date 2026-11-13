//! The real application service/client driven by an explicit, device-free clock.
use crate::SimResult;
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::{AppIntent, AppSnapshot, GlobalControlAction, LoopAction, LoopState};
use shoop_app_client::{
    Client,
    runtime::{Runtime, Startup},
};
use shoop_app_protocol::local;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{
    ControlEndpoint, EngineDescription,
    testing::{OfflineRender, RuntimeCommand},
};
use shoop_rt_protocol::{ClientMessage, EngineMessage};
use shoop_rt_transport::{
    carrier::{Carrier, CarrierError, InProcess},
    client::ProtocolClient,
};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

pub const STEP: Duration = Duration::from_millis(1);
pub fn description() -> EngineDescription {
    rt_fpga_adapter::engine::description()
}

pub struct TraceCarrier {
    enabled: bool,
    pub inner: InProcess<ControlEndpoint>,
    pub trace: Rc<RefCell<Vec<RuntimeCommand>>>,
    pub messages: Rc<RefCell<Vec<String>>>,
}
impl TraceCarrier {
    fn drain(&mut self) {
        self.trace
            .borrow_mut()
            .extend(self.inner.endpoint_mut().take_runtime_commands());
    }
}
impl Carrier for TraceCarrier {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        if self.enabled {
            self.messages.borrow_mut().push(format!("send {message:?}"));
        }
        let result = self.inner.send(message);
        self.drain();
        result
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        let message = self.inner.receive();
        if let Some(message) = &message
            && self.enabled
        {
            self.messages
                .borrow_mut()
                .push(format!("receive {message:?}"));
        }
        self.drain();
        message
    }
}
pub struct NativeClock {
    pub render: Rc<RefCell<OfflineRender>>,
    pub output: Rc<RefCell<Vec<[f32; 2]>>>,
    pub signal: Rc<RefCell<bool>>,
    remainder: u128,
}
impl NativeClock {
    fn run(&mut self, mut frames: usize) {
        while frames > 0 {
            let mut render = self.render.borrow_mut();
            let count = frames.min(render.max_frames());
            let start = render.next_frame();
            for port in 0..2 {
                for (offset, sample) in render.input_mut(port)[..count].iter_mut().enumerate() {
                    *sample = if *self.signal.borrow() {
                        input(port, start + offset as u64)
                    } else {
                        0.
                    };
                }
            }
            render.process(count).unwrap();
            self.output
                .borrow_mut()
                .extend((0..count).map(|i| [render.output(0)[i], render.output(1)[i]]));
            frames -= count;
        }
    }
}
impl EngineClock for NativeClock {
    fn advance(&mut self, elapsed: Duration) {
        self.remainder += elapsed.as_nanos() * 48_000;
        let frames = (self.remainder / 1_000_000_000) as usize;
        self.remainder %= 1_000_000_000;
        self.run(frames);
    }
    fn wait(&mut self) -> bool {
        self.run(32);
        true
    }
}
pub fn input(port: usize, frame: u64) -> f32 {
    let sample = ((frame.wrapping_mul(17) + port as u64 * 97) % 4096) as i32 - 2048;
    sample as f32 / 16384.
}
pub struct Application {
    pub service: Service,
    pub client: Runtime,
    port: local::ServicePort,
}
impl Application {
    pub fn new(backend: impl shoop_backend::Backend + 'static) -> SimResult<Self> {
        let mut service = Service::new(
            Box::new(backend),
            ServiceOptions {
                master_auto_connect: true,
                loop_smoothing_ms: 3,
            },
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        for _ in 0..10000 {
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::yield_now();
            if service.is_ready() {
                break;
            }
            service.tick(STEP);
            if let Some(error) = service.failure() {
                return Err(format!("app service startup: {error}").into());
            }
        }
        if !service.is_ready() {
            return Err("app service startup timed out".into());
        }
        let (delivery, port) = local::channel();
        let client = Runtime::new(
            Client::new(),
            Box::new(delivery),
            Box::new(shoop_scripting::NullMidiService),
            Startup::Fresh(vec![]),
        )
        .map_err(|e| format!("{e:?}"))?;
        Ok(Self {
            service,
            client,
            port,
        })
    }
    pub fn pump(&mut self, count: usize) {
        for _ in 0..count {
            self.pump_elapsed(STEP);
        }
    }
    pub fn pump_elapsed(&mut self, elapsed: Duration) {
        self.service
            .pump_local(ConnectionId(1), &self.port, elapsed);
        self.client.tick(elapsed);
    }
    pub fn ready(&mut self) -> SimResult<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        for _ in 0..10000 {
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::yield_now();
            self.pump(1);
            if self.service.is_ready() && self.client.state() == shoop_app_client::State::Ready {
                return Ok(());
            }
        }
        Err(format!("app startup did not finish: {:?}", self.client.snapshot()).into())
    }
    pub fn reconnect(&mut self) -> SimResult<()> {
        self.client.detach().map_err(|e| format!("{e:?}"))?;
        self.pump(4);
        let (delivery, port) = local::channel();
        self.client = Runtime::new(
            Client::new(),
            Box::new(delivery),
            Box::new(shoop_scripting::NullMidiService),
            Startup::Connected(vec![]),
        )
        .map_err(|e| format!("{e:?}"))?;
        self.port = port;
        self.ready()
    }
    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.client.snapshot()
    }
    pub fn loop_state(&self) -> LoopState {
        self.snapshot().tracks[0].loops[0].clone()
    }
    pub fn action(&mut self, action: LoopAction) -> SimResult<()> {
        let state = self.snapshot();
        let track = &state.tracks[0];
        self.client
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action,
            })
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }
    pub fn immediate(&mut self) -> SimResult<()> {
        self.client
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .map_err(|e| format!("{e:?}"))?;
        self.pump(4);
        Ok(())
    }
}
pub struct Native {
    pub app: Application,
    pub render: Rc<RefCell<OfflineRender>>,
    pub trace: Rc<RefCell<Vec<RuntimeCommand>>>,
    pub messages: Rc<RefCell<Vec<String>>>,
    pub output: Rc<RefCell<Vec<[f32; 2]>>>,
    pub signal: Rc<RefCell<bool>>,
}
impl Native {
    pub fn new() -> SimResult<Self> {
        let (render, mut client) =
            OfflineRender::new(&description()).map_err(|e| format!("{e:?}"))?;
        client
            .carrier_mut()
            .endpoint_mut()
            .capture_runtime_commands();
        let render = Rc::new(RefCell::new(render));
        let trace = Rc::new(RefCell::new(vec![]));
        let messages = Rc::new(RefCell::new(vec![]));
        let output = Rc::new(RefCell::new(vec![]));
        let signal = Rc::new(RefCell::new(false));
        let client = ProtocolClient::new(TraceCarrier {
            enabled: true,
            inner: client.into_carrier(),
            trace: trace.clone(),
            messages: messages.clone(),
        });
        let clock = NativeClock {
            render: render.clone(),
            output: output.clone(),
            signal: signal.clone(),
            remainder: 0,
        };
        let backend = RtBackend::connect(client, clock)?;
        let mut app = Application::new(backend)?;
        app.ready()?;
        Ok(Self {
            app,
            render,
            trace,
            messages,
            output,
            signal,
        })
    }
}

pub const FLOAT_MAX_ERROR: f32 = 0.000_02;
pub const FLOAT_RMS_ERROR: f32 = 0.000_005;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub format_version: u32,
    pub name: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub ticks: usize,
    pub action: Action,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Advance,
    ConnectInput,
    ConnectOutput,
    Record,
    Play,
    Stop,
    Clear,
    Details,
    GlobalSync { enabled: bool },
    LoopGain { value: f32 },
    LoopBalance { value: f32 },
    InputGain { value: f32 },
    InputBalance { value: f32 },
    OutputBalance { value: f32 },
    MasterGain { value: f32 },
    MasterBalance { value: f32 },
    MasterMute { enabled: bool },
    AutoMonitor { enabled: bool },
    OutputGain { value: f32 },
    Mute { enabled: bool },
    Monitor { enabled: bool },
}
impl Application {
    pub fn apply(&mut self, action: &Action) -> SimResult<()> {
        use shoop_app_api::*;
        let snapshot = self.snapshot();
        let track = &snapshot.tracks[0];
        let loop_id = track.loops[0].id;
        let intent = match *action {
            Action::Advance => return Ok(()),
            Action::Record => return self.action(LoopAction::RecordClicked),
            Action::Play => return self.action(LoopAction::PlayClicked),
            Action::Stop => return self.action(LoopAction::StopClicked),
            Action::Details => {
                return self.action(LoopAction::IconClicked(SelectionModifiers::default()));
            }
            Action::Clear => {
                AppIntent::Global(GlobalControlAction::ClearRecordings { include_sync: true })
            }
            Action::GlobalSync { enabled } => {
                AppIntent::Global(GlobalControlAction::SetSync(enabled))
            }
            Action::LoopGain { value } => AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::GainChanged(value),
            },
            Action::LoopBalance { value } => AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::BalanceChanged(value),
            },
            Action::InputGain { value } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::InputGainChanged(value),
            },
            Action::InputBalance { value } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::InputBalanceChanged(value),
            },
            Action::OutputBalance { value } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OutputBalanceChanged(value),
            },
            Action::MasterGain { value } => AppIntent::Bus {
                bus_id: snapshot.buses[0].id,
                action: BusAction::GainChanged(value),
            },
            Action::MasterBalance { value } => AppIntent::Bus {
                bus_id: snapshot.buses[0].id,
                action: BusAction::BalanceChanged(value),
            },
            Action::MasterMute { enabled } => AppIntent::Bus {
                bus_id: snapshot.buses[0].id,
                action: BusAction::MuteChanged(enabled),
            },
            Action::AutoMonitor { enabled } => {
                AppIntent::Global(GlobalControlAction::SetAutoArmTrackInputs(enabled))
            }
            Action::OutputGain { value } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OutputGainChanged(value),
            },
            Action::Mute { enabled } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OutputMuteChanged(enabled),
            },
            Action::Monitor { enabled } => AppIntent::Track {
                track_id: track.id,
                action: TrackAction::InputMonitoringChanged {
                    enabled,
                    respect_auto_mute: false,
                },
            },
            Action::ConnectOutput => {
                let port=snapshot.connections.application_ports.iter().find(|p|p.data_type==PortDataType::Audio&&p.direction==PortDirection::Output&&matches!(p.owner,ApplicationPortOwner::Track {track_id,..} if track_id==track.id)).ok_or("missing logical strip output")?;
                let channel = snapshot
                    .buses
                    .first()
                    .and_then(|b| b.channels.first())
                    .ok_or("missing Master channel")?;
                AppIntent::SetMixerRouteConnected {
                    source_port_id: port.id,
                    destination_channel_id: channel.id,
                    connected: true,
                }
            }
            Action::ConnectInput => {
                let port=snapshot.connections.application_ports.iter().find(|p|p.data_type==PortDataType::Audio&&p.direction==PortDirection::Input&&matches!(p.owner,ApplicationPortOwner::Track {track_id,..} if track_id==track.id)).ok_or("missing logical strip input")?;
                let host = snapshot
                    .connections
                    .host_ports
                    .iter()
                    .find(|p| {
                        p.available
                            && p.data_type == PortDataType::Audio
                            && p.direction == PortDirection::Output
                    })
                    .ok_or("missing synthetic input port")?;
                AppIntent::SetPortConnected {
                    port_id: port.id,
                    host_port_id: host.id.clone(),
                    connected: true,
                }
            }
        };
        self.client.dispatch(intent).map_err(|e| format!("{e:?}"))?;
        Ok(())
    }
}
impl Native {
    pub fn replay(&mut self, trace: &Trace) -> SimResult<Vec<LoopState>> {
        if trace.format_version != 1 || trace.steps.is_empty() {
            return Err("invalid app trace".into());
        }
        *self.signal.borrow_mut() = true;
        let mut states = vec![];
        for (index, step) in trace.steps.iter().enumerate() {
            self.app
                .apply(&step.action)
                .map_err(|e| format!("action {index}: {e}"))?;
            self.app.pump(step.ticks);
            states.push(self.app.loop_state());
        }
        Ok(states)
    }
}

/// The same application bootstrap with the hardware queue consumer.
pub struct Fpga {
    pub app: Application,
    pub hardware: crate::app_worker::Handle,
    pub readiness: Duration,
    pub trace: Rc<RefCell<Vec<RuntimeCommand>>>,
    pub messages: Rc<RefCell<Vec<String>>>,
    pub output: Arc<std::sync::Mutex<Vec<[f32; 2]>>>,
    pub failure: Arc<std::sync::Mutex<Option<String>>>,
}
impl Fpga {
    pub fn new(artifacts: std::path::PathBuf, mode: crate::app_worker::RunMode) -> SimResult<Self> {
        Self::with_trace(artifacts, mode, None)
    }
    pub fn with_trace(
        artifacts: std::path::PathBuf,
        mode: crate::app_worker::RunMode,
        wave: Option<std::path::PathBuf>,
    ) -> SimResult<Self> {
        Self::with_transport(artifacts, mode, wave, crate::app_worker::Transport::Pins)
    }
    pub fn with_transport(
        artifacts: std::path::PathBuf,
        mode: crate::app_worker::RunMode,
        wave: Option<std::path::PathBuf>,
        transport: crate::app_worker::Transport,
    ) -> SimResult<Self> {
        let ready_started = std::time::Instant::now();
        let mut prepared =
            crate::app_worker::Worker::prepare_with(artifacts, mode, wave, transport)?;
        let enabled = mode == crate::app_worker::RunMode::Stepped;
        if enabled {
            prepared.endpoint.capture_runtime_commands();
        }
        let outbox = prepared.outbox();
        let trace = Rc::new(RefCell::new(vec![]));
        let messages = Rc::new(RefCell::new(vec![]));
        let output = prepared.execution.output.clone();
        let failure = prepared.execution.failure.clone();
        let hardware = prepared.execution.handle();
        let client = ProtocolClient::new(TraceCarrier {
            enabled,
            inner: InProcess::new(prepared.endpoint, outbox),
            trace: trace.clone(),
            messages: messages.clone(),
        });
        let backend = RtBackend::connect(client, prepared.execution)?;
        let mut app = Application::new(backend).map_err(|e| {
            format!(
                "{e}; simulation failure={:?}; commands={:#?}",
                failure.lock().unwrap(),
                messages
                    .borrow()
                    .iter()
                    .filter(|m| m.contains("Result") || m.contains("Fault"))
                    .collect::<Vec<_>>()
            )
        })?;
        app.ready().map_err(|e| {
            format!(
                "{e}; simulation failure={:?}; messages={:#?}",
                failure.lock().unwrap(),
                messages
                    .borrow()
                    .iter()
                    .filter(|m| m.contains("Result") || m.contains("Fault"))
                    .collect::<Vec<_>>()
            )
        })?;
        Ok(Self {
            app,
            hardware,
            readiness: ready_started.elapsed(),
            trace,
            messages,
            output,
            failure,
        })
    }
}

impl Fpga {
    pub fn replay(&mut self, trace: &Trace) -> SimResult<Vec<LoopState>> {
        if trace.format_version != 1 || trace.steps.is_empty() {
            return Err("invalid app trace".into());
        }
        self.hardware.signal(true)?;
        let mut states = vec![];
        for (index, step) in trace.steps.iter().enumerate() {
            self.app
                .apply(&step.action)
                .map_err(|e| format!("action {index}: {e}"))?;
            self.app.pump(step.ticks);
            if let Some(error) = self.failure.lock().unwrap().as_ref() {
                return Err(error.clone().into());
            }
            states.push(self.app.loop_state());
        }
        Ok(states)
    }
}

#[derive(Debug)]
pub struct Comparison {
    pub actions: usize,
    pub frames: usize,
    pub max_error: f32,
    pub rms_error: f64,
}
pub fn compare(fpga: &mut Fpga, trace: &Trace) -> SimResult<Comparison> {
    if trace.format_version != 1 || trace.steps.is_empty() {
        return Err("invalid app trace".into());
    }
    let mut native = Native::new()?;
    *native.signal.borrow_mut() = true;
    fpga.hardware.signal(true)?;
    let mut report = Comparison {
        actions: 0,
        frames: 0,
        max_error: 0.,
        rms_error: 0.,
    };
    for (index, step) in trace.steps.iter().enumerate() {
        native.app.apply(&step.action)?;
        fpga.app.apply(&step.action)?;
        native.app.pump(step.ticks);
        fpga.app.pump(step.ticks);
        let frame = fpga.hardware.frame();
        if let Some(error) = fpga.failure.lock().unwrap().as_ref() {
            return Err(format!("action {index}, frame {frame}: {error}").into());
        }
        let n = native.app.loop_state();
        let h = fpga.app.loop_state();
        let logical = |s: &LoopState| {
            (
                s.mode,
                s.empty,
                s.length_frames,
                s.position,
                s.content_revision,
                s.repeat_sync,
                s.next_mode,
                s.next_transition_delay,
                s.gain,
                s.balance,
                s.capture_incomplete,
            )
        };
        if logical(&h) != logical(&n) {
            return Err(format!("action {index}, frame {frame}: loop observation mismatch; native={n:?}, hardware={h:?}").into());
        }
        let n = native.app.snapshot();
        let h = fpga.app.snapshot();
        if let (Some(n), Some(h)) = (&n.details, &h.details) {
            if !n.loading && !h.loading {
                if n.channels.len() != h.channels.len() {
                    return Err(format!("action {index}: waveform channel mismatch").into());
                }
                for (n, h) in n.channels.iter().zip(&h.channels) {
                    if n.samples.len() != h.samples.len()
                        || n.samples.iter().zip(h.samples.iter()).any(|(n, h)| {
                            !n.is_finite() || !h.is_finite() || (n - h).abs() > FLOAT_MAX_ERROR
                        })
                    {
                        return Err(
                            format!("action {index}, frame {frame}: waveform mismatch").into()
                        );
                    }
                }
            }
        }
        let n = native.output.borrow();
        let h = fpga.output.lock().unwrap();
        if n.len() != h.len() {
            return Err(format!(
                "action {index}: output frame count mismatch ({} vs {})",
                n.len(),
                h.len()
            )
            .into());
        }
        let mut squared = 0.;
        let mut max = 0f32;
        for (frame, (n, h)) in n.iter().zip(h.iter()).enumerate() {
            for channel in 0..2 {
                let error = (n[channel] - h[channel]).abs();
                if !error.is_finite() || error > FLOAT_MAX_ERROR {
                    return Err(format!("action {index}, output frame {frame}, channel {channel}: native={}, hardware={}, error={error}",n[channel],h[channel]).into());
                }
                squared += f64::from(error).powi(2);
                max = max.max(error);
            }
        }
        let rms = (squared / (2 * n.len()).max(1) as f64).sqrt();
        if !rms.is_finite() || rms > f64::from(FLOAT_RMS_ERROR) {
            return Err(format!("action {index}: audio RMS error {rms}").into());
        }
        report = Comparison {
            actions: index + 1,
            frames: n.len(),
            max_error: max,
            rms_error: rms,
        };
    }
    Ok(report)
}
