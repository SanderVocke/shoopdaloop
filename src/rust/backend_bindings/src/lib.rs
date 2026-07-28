#![allow(non_camel_case_types, dead_code)]

use anyhow::{anyhow, Result};
use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use shoop_engine as engine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum AudioDriverType {
    Jack = 0,
    JackTest = 1,
    Dummy = 2,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum LoopMode {
    Unknown = 0,
    Stopped = 1,
    Playing = 2,
    Recording = 3,
    Replacing = 4,
    PlayingDryThroughWet = 5,
    RecordingDryIntoWet = 6,
}

impl From<LoopMode> for engine::LoopMode {
    fn from(v: LoopMode) -> Self {
        match v {
            LoopMode::Unknown => Self::Unknown,
            LoopMode::Stopped => Self::Stopped,
            LoopMode::Playing => Self::Playing,
            LoopMode::Recording => Self::Recording,
            LoopMode::Replacing => Self::Replacing,
            LoopMode::PlayingDryThroughWet => Self::PlayingDryThroughWet,
            LoopMode::RecordingDryIntoWet => Self::RecordingDryIntoWet,
        }
    }
}
impl From<engine::LoopMode> for LoopMode {
    fn from(v: engine::LoopMode) -> Self {
        match v {
            engine::LoopMode::Unknown => Self::Unknown,
            engine::LoopMode::Stopped => Self::Stopped,
            engine::LoopMode::Playing => Self::Playing,
            engine::LoopMode::Recording => Self::Recording,
            engine::LoopMode::Replacing => Self::Replacing,
            engine::LoopMode::PlayingDryThroughWet => Self::PlayingDryThroughWet,
            engine::LoopMode::RecordingDryIntoWet => Self::RecordingDryIntoWet,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum ChannelMode {
    Disabled = 0,
    Direct = 1,
    Dry = 2,
    Wet = 3,
}
impl From<ChannelMode> for engine::ChannelMode {
    fn from(v: ChannelMode) -> Self {
        match v {
            ChannelMode::Disabled => Self::Disabled,
            ChannelMode::Direct => Self::Direct,
            ChannelMode::Dry => Self::Dry,
            ChannelMode::Wet => Self::Wet,
        }
    }
}
impl From<engine::ChannelMode> for ChannelMode {
    fn from(v: engine::ChannelMode) -> Self {
        match v {
            engine::ChannelMode::Disabled => Self::Disabled,
            engine::ChannelMode::Direct => Self::Direct,
            engine::ChannelMode::Dry => Self::Dry,
            engine::ChannelMode::Wet => Self::Wet,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDirection {
    Input = 0,
    Output = 1,
    Any = 2,
}
impl From<PortDirection> for engine::PortDirection {
    fn from(v: PortDirection) -> Self {
        match v {
            PortDirection::Input => Self::Input,
            PortDirection::Output => Self::Output,
            PortDirection::Any => Self::Any,
        }
    }
}
impl From<engine::PortDirection> for PortDirection {
    fn from(v: engine::PortDirection) -> Self {
        match v {
            engine::PortDirection::Input => Self::Input,
            engine::PortDirection::Output => Self::Output,
            engine::PortDirection::Any => Self::Any,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDataType {
    Audio = 0,
    Midi = 1,
    Any = 2,
}
impl From<PortDataType> for engine::PortDataType {
    fn from(v: PortDataType) -> Self {
        match v {
            PortDataType::Audio => Self::Audio,
            PortDataType::Midi => Self::Midi,
            PortDataType::Any => Self::Any,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortConnectabilityKind {
    None = 0,
    Internal = 1,
    External = 2,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct PortConnectability {
    pub internal: bool,
    pub external: bool,
}
impl From<engine::PortConnectability> for PortConnectability {
    fn from(v: engine::PortConnectability) -> Self {
        Self {
            internal: v.contains(engine::PortConnectability::INTERNAL),
            external: v.contains(engine::PortConnectability::EXTERNAL),
        }
    }
}
impl PortConnectability {
    pub fn from_ffi(v: u32) -> Self {
        Self {
            internal: v & 1 != 0,
            external: v & 2 != 0,
        }
    }
    pub fn to_ffi(&self) -> u32 {
        (self.internal as u32) | ((self.external as u32) << 1)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum FXChainType {
    CarlaRack = 0,
    CarlaPatchbay = 1,
    CarlaPatchbay16x = 2,
    Test2x2x1 = 3,
}
impl FXChainType {
    pub fn to_ffi(&self) -> shoop_fx_chain_type_t {
        *self
    }
}
impl TryFrom<u32> for FXChainType {
    type Error = num_enum::TryFromPrimitiveError<FXChainType>;
    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        FXChainType::try_from(value as i32)
    }
}
pub type shoop_fx_chain_type_t = FXChainType;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum BackendResult {
    Success = 0,
    Failure = 1,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum LogLevel {
    DebugTrace = 0,
    AlwaysTrace = 1,
    Debug = 2,
    Info = 3,
    Warn = 4,
    Err = 5,
}

pub struct Logger {
    name: String,
}
impl Logger {
    pub fn new(name: &str) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
        })
    }
    pub fn should_log(&self, level: LogLevel) -> bool {
        matches!(level, LogLevel::Info | LogLevel::Warn | LogLevel::Err)
    }
    pub fn log(&self, level: LogLevel, msg: &str) {
        if !self.should_log(level) {
            return;
        }
        let level = match level {
            LogLevel::DebugTrace | LogLevel::AlwaysTrace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warning",
            LogLevel::Err => "error",
        };
        use std::io::Write;
        let _ = writeln!(std::io::stdout(), "[{}] [{}] {}", self.name, level, msg);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub time: i32,
    pub data: Vec<u8>,
}
impl MidiEvent {
    pub fn new(time: i32, data: impl Into<Vec<u8>>) -> Self {
        Self {
            time,
            data: data.into(),
        }
    }
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug, Clone)]
pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

#[derive(Debug, Clone)]
pub struct ProfilingReportItem {
    pub key: String,
    pub n_samples: f32,
    pub average: f32,
    pub worst: f32,
    pub most_recent: f32,
}
#[derive(Debug, Clone, Default)]
pub struct ProfilingReport {
    pub items: Vec<ProfilingReportItem>,
}

#[derive(Debug, Clone)]
pub struct BackendSessionState {
    pub audio_driver: *mut (),
    pub n_audio_buffers_created: u32,
    pub n_audio_buffers_available: u32,
}

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
}
impl jack::ProcessHandler for JackProcess {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;
        let Some(shared) = self.shared.upgrade() else {
            return jack::Control::Continue;
        };
        let mut session = shared.lock();
        session.set_sample_rate(self.sample_rate);
        session.set_buffer_size(n_frames as u32);
        let _ = session.apply_graph_changes();
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

        let _ = session.process(n_frames);

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
}
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
        };
        self.active_client = Some(
            client
                .activate_async(notifications, process)
                .map_err(|e| anyhow!("Failed to activate JACK client: {e}"))?,
        );
        Ok(())
    }
}

struct SharedSession {
    session: Mutex<engine::Session>,
    external: Mutex<Option<Arc<Mutex<engine::DummyExternalConnections>>>>,
    jack: Mutex<Option<Arc<Mutex<JackBackend>>>>,
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
    pub fn create_fx_chain(
        &self,
        _chain_type: shoop_fx_chain_type_t,
        title: &str,
    ) -> Result<FXChain> {
        Ok(FXChain {
            shared: self.shared.clone(),
            title: title.to_string(),
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
impl std::fmt::Debug for DummyAudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DummyAudioDriverSettings")
            .field("client_name", &self.client_name)
            .field("sample_rate", &self.sample_rate)
            .field("buffer_size", &self.buffer_size)
            .finish()
    }
}
pub enum AudioDriverSettings {
    Jack(JackAudioDriverSettings),
    Dummy(DummyAudioDriverSettings),
}
impl std::fmt::Debug for AudioDriverSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jack(s) => f.debug_tuple("Jack").field(s).finish(),
            Self::Dummy(s) => f.debug_tuple("Dummy").field(s).finish(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioDriverState {
    pub dsp_load_percent: f32,
    pub xruns_since_last: u32,
    pub maybe_instance_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub active: u32,
    pub last_processed: u32,
}

type ProcessCallback = unsafe extern "C" fn();
fn driver_uses_dummy_processing(driver_type: AudioDriverType) -> bool {
    matches!(
        driver_type,
        AudioDriverType::Dummy | AudioDriverType::JackTest
    )
}
struct DriverInner {
    driver_type: AudioDriverType,
    settings: engine::DriverSettings,
    active: bool,
    controlled: bool,
    requested: u32,
    last_processed: u32,
    session: Option<Weak<SharedSession>>,
    external: Arc<Mutex<engine::DummyExternalConnections>>,
    jack: Option<Arc<Mutex<JackBackend>>>,
}
pub struct AudioDriver {
    inner: Mutex<DriverInner>,
}
unsafe impl Send for AudioDriver {}
unsafe impl Sync for AudioDriver {}
impl AudioDriver {
    pub fn new(
        driver_type: AudioDriverType,
        _maybe_callback: Option<ProcessCallback>,
    ) -> Result<Self> {
        Ok(Self {
            inner: Mutex::new(DriverInner {
                driver_type,
                settings: engine::DriverSettings::default(),
                active: false,
                controlled: false,
                requested: 0,
                last_processed: 0,
                session: None,
                external: Arc::new(Mutex::new(engine::DummyExternalConnections::default())),
                jack: None,
            }),
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
    fn attach_session(&self, shared: &Arc<SharedSession>) {
        self.inner.lock().unwrap().session = Some(Arc::downgrade(shared));
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
            })));
        } else {
            i.jack = None;
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
        queue: Arc<Mutex<Vec<MidiEvent>>>,
    ) -> Result<()> {
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
    pub fn wait_process(&self) {}
    pub fn get_state(&self) -> AudioDriverState {
        let should_run = {
            let i = self.inner.lock().unwrap();
            driver_uses_dummy_processing(i.driver_type)
                && i.active
                && ((!i.controlled) || i.requested > 0)
        };
        if should_run {
            self.dummy_run_requested_frames();
        }
        let i = self.inner.lock().unwrap();
        let (last_processed, xruns_since_last) = if let Some(j) = i.jack.as_ref() {
            let j = j.lock().unwrap_or_else(|e| e.into_inner());
            (
                j.last_processed.load(Ordering::Relaxed),
                j.xruns.swap(0, Ordering::Relaxed),
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
    pub fn dummy_request_controlled_frames(&self, n: u32) {
        self.inner.lock().unwrap().requested += n;
    }
    pub fn dummy_n_requested_frames(&self) -> u32 {
        self.inner.lock().unwrap().requested
    }
    pub fn dummy_run_requested_frames(&self) {
        loop {
            let (session, n) = {
                let mut i = self.inner.lock().unwrap();
                if !i.active {
                    i.last_processed = 0;
                    return;
                }
                let n = if i.controlled {
                    i.requested.min(i.settings.buffer_size)
                } else {
                    i.settings.buffer_size
                };
                if n == 0 {
                    i.last_processed = 0;
                    return;
                }
                if i.controlled {
                    i.requested -= n;
                }
                i.last_processed = n;
                (i.session.as_ref().and_then(|w| w.upgrade()), n)
            };
            if let Some(shared) = session {
                let mut s = shared.lock();
                s.set_sample_rate(self.get_sample_rate());
                s.set_buffer_size(self.get_buffer_size());
                s.apply_graph_changes().ok();
                let _ = s.process(n as usize);
            }
            if !self.dummy_is_controlled() {
                return;
            }
        }
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
pub fn driver_type_supported(driver_type: AudioDriverType) -> bool {
    matches!(
        driver_type,
        AudioDriverType::Dummy | AudioDriverType::Jack | AudioDriverType::JackTest
    )
}

#[derive(Clone)]
pub struct Loop {
    shared: Arc<SharedSession>,
    idx: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LoopState {
    pub mode: LoopMode,
    pub length: u32,
    pub position: u32,
    pub maybe_next_mode: Option<LoopMode>,
    pub maybe_next_mode_delay: Option<u32>,
}
impl Default for LoopState {
    fn default() -> Self {
        Self {
            mode: LoopMode::Unknown,
            length: 0,
            position: 0,
            maybe_next_mode: None,
            maybe_next_mode_delay: None,
        }
    }
}
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
pub struct AudioChannelState {
    pub mode: ChannelMode,
    pub gain: f32,
    pub output_peak: f32,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<u32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}
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
            played_back_sample: c.played_back_sample().map(|v| v as u32),
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
pub struct MidiChannelState {
    pub mode: ChannelMode,
    pub n_events_triggered: u32,
    pub n_notes_active: u32,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<u32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}
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
            played_back_sample: c.played_back_sample().map(|v| v as u32),
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
#[derive(Debug)]
pub struct AudioPortState {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain: f32,
    pub muted: u32,
    pub passthrough_muted: u32,
    pub ringbuffer_n_samples: u32,
    pub name: String,
}
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
            PortDirection::Input => PortConnectability {
                internal: false,
                external: true,
            },
            PortDirection::Output => PortConnectability {
                internal: true,
                external: false,
            },
            PortDirection::Any => PortConnectability {
                internal: true,
                external: true,
            },
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability {
                internal: true,
                external: false,
            },
            PortDirection::Output => PortConnectability {
                internal: false,
                external: true,
            },
            PortDirection::Any => PortConnectability {
                internal: true,
                external: true,
            },
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
            muted: a.muted() as u32,
            passthrough_muted: a.passthrough_muted() as u32,
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
#[derive(Debug)]
pub struct MidiPortState {
    pub n_input_events: u32,
    pub n_input_notes_active: u32,
    pub n_output_events: u32,
    pub n_output_notes_active: u32,
    pub muted: u32,
    pub passthrough_muted: u32,
    pub ringbuffer_n_samples: u32,
    pub name: String,
}
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
            PortDirection::Input => PortConnectability {
                internal: false,
                external: true,
            },
            PortDirection::Output => PortConnectability {
                internal: true,
                external: false,
            },
            PortDirection::Any => PortConnectability {
                internal: true,
                external: true,
            },
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        match self.direction {
            PortDirection::Input => PortConnectability {
                internal: true,
                external: false,
            },
            PortDirection::Output => PortConnectability {
                internal: false,
                external: true,
            },
            PortDirection::Any => PortConnectability {
                internal: true,
                external: true,
            },
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
            muted: m.muted() as u32,
            passthrough_muted: m.passthrough_muted() as u32,
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
        driver.register_decoupled_midi_port(name, *direction, queue.clone())?;
        Ok(Self {
            name: name.to_string(),
            direction: *direction,
            port_id: engine::PortId(NEXT_DECOUPLED_PORT_ID.fetch_add(1, Ordering::Relaxed)),
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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FXChainState {
    pub ready: u32,
    pub active: u32,
    pub visible: u32,
}
pub struct FXChain {
    shared: Arc<SharedSession>,
    title: String,
    state: Arc<Mutex<FXChainState>>,
}
impl FXChain {
    pub fn available(&self) -> bool {
        true
    }
    pub fn set_visible(&self, visible: bool) {
        self.state.lock().unwrap().visible = visible as u32
    }
    pub fn set_active(&self, active: bool) {
        self.state.lock().unwrap().active = active as u32;
        self.shared
            .lock()
            .set_test_fx_active(self.title.clone(), active);
    }
    pub fn get_state(&self) -> Option<FXChainState> {
        let mut s = self.state.lock().unwrap().clone();
        s.ready = 1;
        Some(s)
    }
    pub fn get_state_str(&self) -> Option<String> {
        Some(String::new())
    }
    pub fn restore_state(&self, _state: &str) {}
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
        Some(self.make_audio_port(
            format!("{}:audio_in_{}", self.title, idx),
            PortDirection::Output,
        ))
    }
    pub fn get_audio_output_port(&self, idx: u32) -> Option<AudioPort> {
        Some(self.make_audio_port(
            format!("{}:audio_out_{}", self.title, idx),
            PortDirection::Input,
        ))
    }
    pub fn get_midi_input_port(&self, idx: u32) -> Option<MidiPort> {
        Some(self.make_midi_port(
            format!("{}:midi_in_{}", self.title, idx),
            PortDirection::Output,
        ))
    }
    pub fn get_midi_output_port(&self, idx: u32) -> Option<MidiPort> {
        Some(self.make_midi_port(
            format!("{}:midi_out_{}", self.title, idx),
            PortDirection::Input,
        ))
    }
}

pub struct MultichannelAudio {
    n_channels: u32,
    n_frames: u32,
    data: Mutex<Vec<f32>>,
}
impl MultichannelAudio {
    pub fn new(n_channels: u32, n_frames: u32) -> Result<Self> {
        Ok(Self {
            n_channels,
            n_frames,
            data: Mutex::new(vec![0.0; (n_channels * n_frames) as usize]),
        })
    }
    pub fn resample(&self, new_n_frames: u32) -> Result<Self> {
        let src = self.data.lock().unwrap();
        let interleaved = src.clone();
        let out = engine::resample::resample_interleaved(
            &interleaved,
            self.n_channels as usize,
            new_n_frames as usize,
        )
        .map_err(|e| anyhow!(e))?;
        let r = Self::new(self.n_channels, new_n_frames)?;
        {
            let mut dst = r.data.lock().unwrap();
            *dst = out;
        }
        Ok(r)
    }
    pub fn at(&self, frame: u32, channel: u32) -> Result<f32> {
        Ok(self.data.lock().unwrap()[(frame * self.n_channels + channel) as usize])
    }
    pub fn set(&self, frame: u32, channel: u32, value: f32) -> Result<()> {
        self.data.lock().unwrap()[(frame * self.n_channels + channel) as usize] = value;
        Ok(())
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
    }
}
