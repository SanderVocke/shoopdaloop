//! Child-side Carla control and DSP protocol.

use crate::carla_shared_memory::{SharedBlockError, SharedBlockTransport};
use crate::native::{CarlaMainThreadUiService, CarlaNativeHost, CarlaNativeUiHandle};
use anyhow::{anyhow, Context, Result};
use shoop_plugin_protocol::{
    read_frame, write_frame, CarlaChainType, ChainId, ControlRequestKind, ControlResponse,
    ControlResponseKind, LifecycleState, ParentToWorker, PreparedMidiBuffer, ProcessGeneration,
    ProtocolError, ProtocolErrorCode, PrototypeBlockResult, RequestId, WorkerExitKind, WorkerHello,
    WorkerStatus, WorkerToParent, MAX_AUDIO_CHANNELS, MAX_MIDI_EVENTS_PER_BLOCK,
    MAX_NATIVE_MIDI_PACKET_BYTES,
};
use std::{
    ffi::OsStr,
    fmt,
    io::Write,
    net::{SocketAddr, TcpStream, UdpSocket},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const LOG_CAPACITY: usize = 64 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Operations for the processor running in the child. A supplied test processor
/// uses the same shape and control protocol as the Native host.
pub trait ChildPlugin: Send {
    fn editor(&self) -> Option<CarlaNativeUiHandle> {
        None
    }
    fn set_active(&mut self, active: bool);
    fn is_active(&self) -> bool;
    fn set_visible(&mut self, visible: bool) -> Result<()>;
    fn is_visible(&mut self) -> bool;
    fn save_state(&mut self) -> Result<String>;
    fn restore_state(&mut self, state: &str) -> Result<()>;
    fn audio_input_mut(&mut self, channel: usize) -> Option<&mut [f32]>;
    fn audio_output(&self, channel: usize) -> Option<&[f32]>;
    fn set_midi_input(&mut self, events: &[(u32, &[u8])]) -> Result<()>;
    fn fill_midi_output(&mut self, output: &mut PreparedMidiBuffer) -> Result<()>;
    fn process(&mut self, frames: usize) -> Result<()>;
}

impl ChildPlugin for CarlaNativeHost {
    fn editor(&self) -> Option<CarlaNativeUiHandle> {
        self.external_ui()
    }

    fn set_active(&mut self, active: bool) {
        CarlaNativeHost::set_active(self, active)
    }

    fn is_active(&self) -> bool {
        CarlaNativeHost::is_active(self)
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        CarlaNativeHost::set_visible(self, visible)
    }

    fn is_visible(&mut self) -> bool {
        CarlaNativeHost::is_visible(self)
    }

    fn save_state(&mut self) -> Result<String> {
        CarlaNativeHost::save_state(self)
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        CarlaNativeHost::restore_state(self, state)
    }

    fn audio_input_mut(&mut self, channel: usize) -> Option<&mut [f32]> {
        CarlaNativeHost::audio_input_mut(self, channel)
    }

    fn audio_output(&self, channel: usize) -> Option<&[f32]> {
        CarlaNativeHost::audio_output(self, channel)
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        CarlaNativeHost::process(self, frames)
    }

    fn set_midi_input(&mut self, events: &[(u32, &[u8])]) -> Result<()> {
        self.set_midi_input_events(0, events)
    }

    fn fill_midi_output(&mut self, output: &mut PreparedMidiBuffer) -> Result<()> {
        self.visit_midi_output(0, |offset, bytes| Ok(output.push(offset, bytes)?))
    }
}

pub fn parse_nonce(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(anyhow!("Carla worker nonce must contain 64 hex digits"));
    }
    let mut nonce = [0u8; 32];
    for (index, byte) in nonce.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("worker nonce contains invalid hexadecimal")?;
    }
    Ok(nonce)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarlaWorkerTestMode {
    Fake,
    Abort,
    ProcessError,
    Hang,
    DelayOnce,
    UiClose,
    FloodLogs,
    MalformedHandshake,
    HangShutdown,
}

impl std::str::FromStr for CarlaWorkerTestMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "fake" => Ok(Self::Fake),
            "abort" => Ok(Self::Abort),
            "process-error" => Ok(Self::ProcessError),
            "hang" => Ok(Self::Hang),
            "delay-once" => Ok(Self::DelayOnce),
            "ui-close" => Ok(Self::UiClose),
            "flood-logs" => Ok(Self::FloodLogs),
            "malformed-handshake" => Ok(Self::MalformedHandshake),
            "hang-shutdown" => Ok(Self::HangShutdown),
            _ => Err(anyhow!("unknown Carla worker test mode {value:?}")),
        }
    }
}

impl fmt::Display for CarlaWorkerTestMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Fake => "fake",
            Self::Abort => "abort",
            Self::ProcessError => "process-error",
            Self::Hang => "hang",
            Self::DelayOnce => "delay-once",
            Self::UiClose => "ui-close",
            Self::FloodLogs => "flood-logs",
            Self::MalformedHandshake => "malformed-handshake",
            Self::HangShutdown => "hang-shutdown",
        })
    }
}

#[derive(Debug, Clone)]
pub struct CarlaWorkerOptions {
    pub address: SocketAddr,
    pub nonce: [u8; 32],
    pub chain_id: ChainId,
    pub generation: ProcessGeneration,
    pub shared_memory_path: PathBuf,
    pub test_mode: Option<CarlaWorkerTestMode>,
}

pub fn worker_options<S: AsRef<OsStr>>(
    args: impl IntoIterator<Item = S>,
) -> Result<Option<CarlaWorkerOptions>> {
    let args: Vec<_> = args.into_iter().collect();
    if !args
        .iter()
        .any(|argument| argument.as_ref() == "--carla-worker")
    {
        return Ok(None);
    }
    let value = |name: &str| -> Result<&OsStr> {
        let prefix = format!("{name}=");
        for (index, argument) in args.iter().enumerate() {
            let argument = argument.as_ref();
            if argument == name {
                return args
                    .get(index + 1)
                    .map(AsRef::as_ref)
                    .ok_or_else(|| anyhow!("missing value for {name}"));
            }
            if let Some(value) = argument
                .to_str()
                .and_then(|value| value.strip_prefix(&prefix))
            {
                return Ok(OsStr::new(value));
            }
        }
        Err(anyhow!("missing Carla worker argument {name}"))
    };
    let text = |name| {
        value(name)?
            .to_str()
            .ok_or_else(|| anyhow!("invalid text for {name}"))
    };
    let test_mode_flag = "--carla-worker-test-mode";
    let test_mode = args
        .iter()
        .any(|argument| {
            argument.as_ref().to_str().is_some_and(|value| {
                value == test_mode_flag || value.starts_with("--carla-worker-test-mode=")
            })
        })
        .then(|| text(test_mode_flag)?.parse())
        .transpose()?;
    Ok(Some(CarlaWorkerOptions {
        nonce: parse_nonce(text("--carla-worker-nonce")?)?,
        address: text("--carla-worker-address")?
            .parse()
            .context("invalid Carla worker address")?,
        chain_id: ChainId(text("--carla-worker-chain-id")?.parse()?),
        generation: ProcessGeneration(text("--carla-worker-generation")?.parse()?),
        shared_memory_path: value("--carla-worker-shared-memory")?.into(),
        test_mode,
    }))
}

fn response(
    options: &CarlaWorkerOptions,
    request_id: RequestId,
    kind: ControlResponseKind,
) -> WorkerToParent {
    WorkerToParent::Control(ControlResponse {
        request_id,
        chain_id: options.chain_id,
        generation: options.generation,
        kind,
    })
}

fn protocol_error(error: impl fmt::Display) -> ControlResponseKind {
    ControlResponseKind::Error(ProtocolError {
        code: ProtocolErrorCode::HostFailure,
        message: error.to_string(),
    })
}

struct SharedMemoryCleanup(PathBuf);

impl Drop for SharedMemoryCleanup {
    fn drop(&mut self) {
        // On Unix the worker may unlink while the parent mapping is still open;
        // on Windows this succeeds after an abnormal parent exit and otherwise
        // the parent's NamedTempFile performs the normal cleanup.
        let _ = std::fs::remove_file(&self.0);
    }
}

struct SharedWorkerGuard {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for SharedWorkerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct WorkerHost {
    plugin: Box<dyn ChildPlugin>,
    channels: usize,
}

fn run_shared_worker(
    mut transport: SharedBlockTransport,
    host: Arc<Mutex<Option<WorkerHost>>>,
    stop: Arc<AtomicBool>,
    processed_blocks: Arc<AtomicU64>,
    notification: UdpSocket,
    notification_token: [u8; 16],
) {
    let mut midi_inputs =
        PreparedMidiBuffer::new(MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES);
    let mut midi_outputs =
        PreparedMidiBuffer::new(MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES);
    let mut notification_buffer = [0_u8; 16];
    while !stop.load(Ordering::Acquire) {
        let Some(token) = transport.worker_take() else {
            match notification.recv(&mut notification_buffer) {
                Ok(read)
                    if read == notification_token.len()
                        && notification_buffer == notification_token => {}
                Ok(_) => continue,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(_) => break,
            }
            continue;
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let mut host = host.lock().unwrap_or_else(|error| error.into_inner());
            let host = host
                .as_mut()
                .ok_or_else(|| anyhow!("shared block arrived before Carla instantiation"))?;
            let channels = host.channels;
            let host = host.plugin.as_mut();
            if transport.worker_audio_input_channels(token) != channels
                || transport.worker_audio_output_channels(token) != channels
            {
                return Err(anyhow!(
                    "shared block channel layout does not match Carla host"
                ));
            }
            for channel in 0..channels {
                let destination = host
                    .audio_input_mut(channel)
                    .ok_or_else(|| anyhow!("Carla audio input {channel} disappeared"))?;
                transport.worker_copy_audio_input(token, channel, destination)?;
            }
            let (midi_pool, midi_count) = midi_inputs.storage_mut();
            transport.worker_read_midi_reusing(token, midi_pool, midi_count)?;
            let mut refs = [(0_u32, &[][..]); MAX_MIDI_EVENTS_PER_BLOCK];
            for (destination, event) in refs.iter_mut().zip(midi_inputs.as_slice()) {
                *destination = (event.frame_offset, event.data.as_slice());
            }
            host.set_midi_input(&refs[..midi_inputs.as_slice().len()])?;
            host.process(token.frames)?;
            midi_outputs.clear();
            host.fill_midi_output(&mut midi_outputs)?;
            let mut audio_outputs = [&[][..]; MAX_AUDIO_CHANNELS];
            for (channel, output) in audio_outputs.iter_mut().enumerate().take(channels) {
                *output = host
                    .audio_output(channel)
                    .ok_or_else(|| anyhow!("Carla audio output {channel} disappeared"))?;
            }
            transport.worker_complete(
                token,
                &audio_outputs[..channels],
                midi_outputs.as_slice(),
            )?;
            processed_blocks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error))
                if matches!(
                    error.downcast_ref::<SharedBlockError>(),
                    Some(SharedBlockError::DeadlineMiss | SharedBlockError::StaleCompletion)
                ) =>
            {
                // The parent owns deadline accounting and bounded fallback.
                // A stale realtime block is recoverable and must not turn one
                // scheduling miss into a supervised-process crash.
            }
            Ok(Err(error)) => {
                eprintln!("Carla shared-memory worker failed: {error:#}");
                std::process::exit(70);
            }
            Err(_) => {
                eprintln!("Carla shared-memory worker panicked");
                std::process::abort();
            }
        }
    }
}

pub fn run(options: CarlaWorkerOptions) -> Result<()> {
    if options.test_mode.is_some() {
        return Err(anyhow!(
            "Carla worker test mode requires a supplied test processor"
        ));
    }
    run_with_factory(options, |_, _| {
        Err(anyhow!("Carla test processor unavailable"))
    })
}

pub fn run_with_factory(
    options: CarlaWorkerOptions,
    test_factory: impl Fn(CarlaWorkerTestMode, CarlaChainType) -> Result<Box<dyn ChildPlugin>>,
) -> Result<()> {
    if !options.address.ip().is_loopback() {
        return Err(anyhow!("Carla parent address must be loopback"));
    }
    if options.chain_id.0 == 0 || options.generation.0 == 0 {
        return Err(anyhow!("Carla worker identity must be nonzero"));
    }
    let (mut ui_service, ui_dispatcher) = CarlaMainThreadUiService::new();
    let mut stream = TcpStream::connect_timeout(&options.address, STARTUP_TIMEOUT)
        .context("worker could not connect to parent")?;
    stream.set_nodelay(true)?;
    // A short timeout lets the control owner service Carla's UI without depending
    // on control traffic. Timeouts are idle ticks, not worker failures.
    stream.set_read_timeout(Some(Duration::from_millis(30)))?;
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
    let notification = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    notification.set_read_timeout(Some(Duration::from_millis(10)))?;
    let mut hello = WorkerHello::current(options.nonce, options.generation);
    hello.notification_port = notification.local_addr()?.port();
    if options.test_mode == Some(CarlaWorkerTestMode::MalformedHandshake) {
        hello.protocol_version = hello.protocol_version.saturating_add(1);
    }
    if options.test_mode == Some(CarlaWorkerTestMode::FloodLogs) {
        let flood = vec![b'x'; LOG_CAPACITY * 4];
        std::io::stdout().write_all(&flood)?;
        std::io::stderr().write_all(&flood)?;
    }
    write_frame(
        &mut stream,
        &response(
            &options,
            RequestId(1),
            ControlResponseKind::Handshake(hello),
        ),
    )?;

    let shared_transport = SharedBlockTransport::open(
        &options.shared_memory_path,
        options.generation,
        &options.nonce,
    )?;
    let _shared_memory_cleanup = SharedMemoryCleanup(options.shared_memory_path.clone());
    let host: Arc<Mutex<Option<WorkerHost>>> = Arc::new(Mutex::new(None));
    let mut external_ui: Option<CarlaNativeUiHandle> = None;
    let stop = Arc::new(AtomicBool::new(false));
    let processed_blocks = Arc::new(AtomicU64::new(0));
    let shared_thread = thread::Builder::new()
        .name("carla-worker-realtime".to_owned())
        .spawn({
            let host = Arc::clone(&host);
            let stop = Arc::clone(&stop);
            let processed_blocks = Arc::clone(&processed_blocks);
            let notification_token: [u8; 16] = options.nonce[..16]
                .try_into()
                .expect("nonce prefix has fixed size");
            move || {
                run_shared_worker(
                    shared_transport,
                    host,
                    stop,
                    processed_blocks,
                    notification,
                    notification_token,
                )
            }
        })?;
    let _shared_worker = SharedWorkerGuard {
        stop,
        thread: Some(shared_thread),
    };
    let mut status = WorkerStatus {
        lifecycle: LifecycleState::Starting,
        generation: options.generation,
        ..Default::default()
    };
    let mut message_midi =
        PreparedMidiBuffer::new(MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES);
    loop {
        ui_service.pump();
        let mut available = [0_u8; 1];
        match stream.peek(&mut available) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        // Read an entire message or close the connection at the control deadline.
        stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
        let result = read_frame(&mut stream);
        stream.set_read_timeout(Some(Duration::from_millis(30)))?;
        let message: ParentToWorker = match result {
            Ok(message) => message,
            Err(shoop_plugin_protocol::WireError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        match message {
            ParentToWorker::Control(request) => {
                if let Err(error) = request.validate() {
                    write_frame(
                        &mut stream,
                        &response(&options, request.request_id, protocol_error(error)),
                    )?;
                    continue;
                }
                if request.chain_id != options.chain_id || request.generation != options.generation
                {
                    write_frame(
                        &mut stream,
                        &response(
                            &options,
                            request.request_id,
                            ControlResponseKind::Error(ProtocolError {
                                code: ProtocolErrorCode::StaleGeneration,
                                message: "request identity does not match worker".to_owned(),
                            }),
                        ),
                    )?;
                    continue;
                }
                let request_id = request.request_id;
                if let (ControlRequestKind::SetVisible(visible), Some(ui)) =
                    (&request.kind, external_ui.as_ref())
                {
                    let kind = match ui.set_visible(*visible) {
                        Ok(()) => {
                            status.visible = ui.is_visible();
                            ControlResponseKind::Ack
                        }
                        Err(error) => protocol_error(error),
                    };
                    write_frame(&mut stream, &response(&options, request_id, kind))?;
                    continue;
                }
                let mut host = host.lock().unwrap_or_else(|error| error.into_inner());
                let kind = match request.kind {
                    ControlRequestKind::Instantiate {
                        chain_type,
                        sample_rate,
                        nominal_buffer_size,
                    } => {
                        let created = if let Some(mode) = options.test_mode {
                            test_factory(mode, chain_type)
                        } else {
                            CarlaNativeHost::instantiate_with_ui_dispatcher(
                                chain_type,
                                sample_rate,
                                nominal_buffer_size,
                                ui_dispatcher.clone(),
                            )
                            .map(|processor| Box::new(processor) as Box<dyn ChildPlugin>)
                        };
                        match created {
                            Ok(plugin) => {
                                external_ui = plugin.editor();
                                *host = Some(WorkerHost {
                                    plugin,
                                    channels: usize::from(chain_type.audio_channels()),
                                });
                                status.lifecycle = LifecycleState::Running;
                                status.ready = true;
                                ControlResponseKind::Ack
                            }
                            Err(error) => {
                                status.lifecycle = LifecycleState::Unavailable;
                                status.exit_kind = WorkerExitKind::StartupFailure;
                                protocol_error(error)
                            }
                        }
                    }
                    ControlRequestKind::SetActive(active) => match host.as_mut() {
                        Some(host) => {
                            host.plugin.set_active(active);
                            status.active = active;
                            ControlResponseKind::Ack
                        }
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::SetVisible(visible) => {
                        if let Some(ui) = &external_ui {
                            match ui.set_visible(visible) {
                                Ok(()) => {
                                    status.visible = ui.is_visible();
                                    ControlResponseKind::Ack
                                }
                                Err(error) => protocol_error(error),
                            }
                        } else {
                            match host.as_mut() {
                                Some(host) => match host.plugin.set_visible(visible) {
                                    Ok(()) => {
                                        status.visible = host.plugin.is_visible();
                                        ControlResponseKind::Ack
                                    }
                                    Err(error) => protocol_error(error),
                                },
                                None => protocol_error("Carla host is not instantiated"),
                            }
                        }
                    }
                    ControlRequestKind::SaveState => match host.as_mut() {
                        Some(host) => match host.plugin.save_state() {
                            Ok(state) => ControlResponseKind::State(state),
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::RestoreState(state) => match host.as_mut() {
                        Some(host) => match host.plugin.restore_state(&state) {
                            Ok(()) => ControlResponseKind::Ack,
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::Status => {
                        if let Some(host) = host.as_mut() {
                            status.active = host.plugin.is_active();
                            status.visible = external_ui
                                .as_ref()
                                .map(|ui| ui.is_visible())
                                .unwrap_or_else(|| host.plugin.is_visible());
                        }
                        status.processed_blocks = processed_blocks.load(Ordering::Relaxed);
                        ControlResponseKind::Status(status.clone())
                    }
                    ControlRequestKind::Ping => ControlResponseKind::Pong,
                    ControlRequestKind::Shutdown => {
                        if options.test_mode == Some(CarlaWorkerTestMode::HangShutdown) {
                            thread::sleep(
                                CONTROL_TIMEOUT + SHUTDOWN_TIMEOUT + Duration::from_secs(2),
                            );
                        }
                        write_frame(
                            &mut stream,
                            &response(&options, request_id, ControlResponseKind::Ack),
                        )?;
                        return Ok(());
                    }
                    ControlRequestKind::Handshake(_) => {
                        protocol_error("worker handshake is initiated by the worker")
                    }
                };
                write_frame(&mut stream, &response(&options, request_id, kind))?;
            }
            ParentToWorker::Process(block) => {
                if let Err(error) = block.validate() {
                    return Err(error.into());
                }
                if block.generation != options.generation {
                    status.stale_completions = status.stale_completions.saturating_add(1);
                    continue;
                }
                let mut host = host.lock().unwrap_or_else(|error| error.into_inner());
                let host = host
                    .as_mut()
                    .ok_or_else(|| anyhow!("received process block before instantiation"))?;
                let channels = host.channels;
                let host = host.plugin.as_mut();
                if block.audio_inputs.len() != channels {
                    return Err(anyhow!(
                        "audio input channel count does not match Carla host"
                    ));
                }
                for (index, source) in block.audio_inputs.iter().enumerate() {
                    let destination = host
                        .audio_input_mut(index)
                        .ok_or_else(|| anyhow!("Carla audio input {index} disappeared"))?;
                    destination[..block.frames as usize].copy_from_slice(source);
                }
                let midi: Vec<_> = block
                    .midi_inputs
                    .iter()
                    .map(|event| (event.frame_offset, event.data.as_slice()))
                    .collect();
                host.set_midi_input(&midi)?;
                host.process(block.frames as usize)?;
                let mut audio_outputs = Vec::with_capacity(channels);
                for index in 0..channels {
                    audio_outputs.push(
                        host.audio_output(index)
                            .ok_or_else(|| anyhow!("Carla audio output {index} disappeared"))?
                            [..block.frames as usize]
                            .to_vec(),
                    );
                }
                message_midi.clear();
                host.fill_midi_output(&mut message_midi)?;
                processed_blocks.fetch_add(1, Ordering::Relaxed);
                let result = PrototypeBlockResult {
                    sequence: block.sequence,
                    generation: options.generation,
                    frames: block.frames,
                    audio_outputs,
                    midi_outputs: message_midi.as_slice().to_vec(),
                };
                result.validate()?;
                write_frame(&mut stream, &WorkerToParent::Process(result))?;
            }
        }
    }
}
