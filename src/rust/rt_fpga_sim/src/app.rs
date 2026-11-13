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
    ControlEndpoint, EngineDescription, HostPorts, Segment,
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
    EngineDescription {
        host: HostPorts::numbered(2, 2, 0),
        segments: vec![Segment {
            strips: 1,
            loops: 1,
        }],
        buses: 1,
        bus_channels: 2,
        master_connections: vec![],
        telemetry_interval_frames: 0,
        reliable_credits: 32,
        bulk_credits: 2,
        pending_transitions: 16,
        ..EngineDescription::default()
    }
}

pub struct TraceCarrier {
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
        self.messages.borrow_mut().push(format!("send {message:?}"));
        let result = self.inner.send(message);
        self.drain();
        result
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        let message = self.inner.receive();
        if let Some(message) = &message {
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
        for _ in 0..1000 {
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
            self.service.pump_local(ConnectionId(1), &self.port, STEP);
            self.client.tick(STEP);
        }
    }
    pub fn ready(&mut self) -> SimResult<()> {
        for _ in 0..1000 {
            self.pump(1);
            if self.service.is_ready() && self.client.state() == shoop_app_client::State::Ready {
                return Ok(());
            }
        }
        Err(format!("app startup did not finish: {:?}", self.client.snapshot()).into())
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
