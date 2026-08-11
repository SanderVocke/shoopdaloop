use crate::carla_native::CarlaNativeHost;
use crate::carla_processor::{
    CarlaGenerationLog, CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo,
    CarlaProcessorLifecycle, FakeCarlaProcessor, FakeProcessorBehavior,
};
use crate::carla_shared_memory::SharedBlockTransport;
use crate::realtime_lock_guard::Mutex;
use crate::FXChainType;
use anyhow::{anyhow, Context, Result};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, ChainId, ControlRequest,
    ControlRequestKind, ControlResponse, ControlResponseKind, LifecycleState, MidiEvent,
    ParentToWorker, ProcessGeneration, ProtocolError, ProtocolErrorCode, PrototypeBlockResult,
    RequestId, WorkerExitKind, WorkerHello, WorkerStatus, WorkerToParent, MAX_AUDIO_CHANNELS,
    MAX_BLOCK_FRAMES, MAX_MIDI_EVENTS_PER_BLOCK,
};
use std::collections::VecDeque;
use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const LOG_CAPACITY: usize = 64 * 1024;
const MAX_LOG_GENERATIONS: usize = 8;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const UI_CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

fn protocol_chain_type(chain_type: FXChainType) -> Result<CarlaChainType> {
    match chain_type {
        FXChainType::CarlaRack => Ok(CarlaChainType::Rack),
        FXChainType::CarlaPatchbay => Ok(CarlaChainType::Patchbay),
        FXChainType::CarlaPatchbay16x => Ok(CarlaChainType::Patchbay16x),
        FXChainType::Test2x2x1 | FXChainType::TinySynthFx => {
            Err(anyhow!("FX chain is not a Carla worker type"))
        }
    }
}

fn engine_chain_type(chain_type: CarlaChainType) -> FXChainType {
    match chain_type {
        CarlaChainType::Rack => FXChainType::CarlaRack,
        CarlaChainType::Patchbay => FXChainType::CarlaPatchbay,
        CarlaChainType::Patchbay16x => FXChainType::CarlaPatchbay16x,
    }
}

fn nonce_hex(nonce: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in nonce {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

pub fn parse_nonce(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        return Err(anyhow!("worker nonce must contain 64 hexadecimal digits"));
    }
    let mut nonce = [0u8; 32];
    for (index, byte) in nonce.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("worker nonce contains invalid hexadecimal")?;
    }
    Ok(nonce)
}

fn new_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    nonce[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    nonce[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    nonce
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarlaWorkerTestMode {
    Fake,
    Abort,
    ProcessError,
    Hang,
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

type WorkerHost = Box<dyn CarlaProcessor>;

fn run_shared_worker(
    mut transport: SharedBlockTransport,
    host: Arc<Mutex<Option<WorkerHost>>>,
    stop: Arc<AtomicBool>,
    processed_blocks: Arc<AtomicU64>,
    notification: UdpSocket,
    notification_token: [u8; 16],
) {
    let mut midi_inputs = CarlaMidiBuffer::new(
        MAX_MIDI_EVENTS_PER_BLOCK,
        crate::midi_storage::MAX_MSG_BYTES,
    );
    let mut midi_outputs = CarlaMidiBuffer::new(
        MAX_MIDI_EVENTS_PER_BLOCK,
        crate::midi_storage::MAX_MSG_BYTES,
    );
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
            let info = host.info();
            if transport.worker_audio_input_channels(token) != info.audio_inputs
                || transport.worker_audio_output_channels(token) != info.audio_outputs
            {
                return Err(anyhow!(
                    "shared block channel layout does not match Carla host"
                ));
            }
            for channel in 0..info.audio_inputs {
                let destination = host
                    .audio_input_mut(channel)
                    .ok_or_else(|| anyhow!("Carla audio input {channel} disappeared"))?;
                transport.worker_copy_audio_input(token, channel, destination)?;
            }
            let (midi_pool, midi_count) = midi_inputs.storage_mut();
            transport.worker_read_midi_reusing(token, midi_pool, midi_count)?;
            if info.midi_inputs > 0 {
                let mut refs = [(0_u32, &[][..]); MAX_MIDI_EVENTS_PER_BLOCK];
                for (destination, event) in refs.iter_mut().zip(midi_inputs.as_slice()) {
                    *destination = (event.frame_offset, event.data.as_slice());
                }
                host.set_midi_input_events(0, &refs[..midi_inputs.as_slice().len()])?;
            }
            host.process(token.frames)?;
            midi_outputs.clear();
            if info.midi_outputs > 0 {
                host.fill_midi_output_events(0, &mut midi_outputs)?;
            }
            let mut audio_outputs = [&[][..]; MAX_AUDIO_CHANNELS];
            for (channel, output) in audio_outputs
                .iter_mut()
                .enumerate()
                .take(info.audio_outputs)
            {
                *output = host
                    .audio_output(channel)
                    .ok_or_else(|| anyhow!("Carla audio output {channel} disappeared"))?;
            }
            transport.worker_complete(
                token,
                &audio_outputs[..info.audio_outputs],
                midi_outputs.as_slice(),
            )?;
            processed_blocks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        match result {
            Ok(Ok(())) => {}
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

fn instantiate_worker_host(
    options: &CarlaWorkerOptions,
    chain_type: CarlaChainType,
    sample_rate: u32,
    nominal_buffer_size: u32,
) -> Result<WorkerHost> {
    if let Some(mode) = options.test_mode {
        let mut fake = FakeCarlaProcessor::new(
            engine_chain_type(chain_type),
            chain_type.audio_channels() as usize,
            MAX_BLOCK_FRAMES,
        );
        let behavior = match mode {
            CarlaWorkerTestMode::Abort => FakeProcessorBehavior {
                panic_processing: true,
                ..Default::default()
            },
            CarlaWorkerTestMode::ProcessError => FakeProcessorBehavior {
                fail_processing: true,
                ..Default::default()
            },
            CarlaWorkerTestMode::Hang => FakeProcessorBehavior {
                process_delay: Duration::from_secs(5),
                ..Default::default()
            },
            _ => FakeProcessorBehavior::default(),
        };
        fake.set_behavior(behavior);
        Ok(Box::new(fake))
    } else {
        CarlaNativeHost::instantiate(
            engine_chain_type(chain_type),
            sample_rate,
            nominal_buffer_size,
        )
        .map(|host| Box::new(host) as WorkerHost)
    }
}

pub fn run_carla_worker(options: CarlaWorkerOptions) -> Result<()> {
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
    loop {
        if let Some(host) = host
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
        {
            if host.is_visible() {
                host.idle();
            }
        }
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
        // Once a frame starts, read it without a timeout so a partial TCP frame
        // cannot be mistaken for a fresh header on the next idle tick.
        stream.set_read_timeout(None)?;
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
                let mut host = host.lock().unwrap_or_else(|error| error.into_inner());
                let kind = match request.kind {
                    ControlRequestKind::Instantiate {
                        chain_type,
                        sample_rate,
                        nominal_buffer_size,
                    } => match instantiate_worker_host(
                        &options,
                        chain_type,
                        sample_rate,
                        nominal_buffer_size,
                    ) {
                        Ok(created) => {
                            *host = Some(created);
                            status.lifecycle = LifecycleState::Running;
                            status.ready = true;
                            ControlResponseKind::Ack
                        }
                        Err(error) => {
                            status.lifecycle = LifecycleState::Unavailable;
                            status.exit_kind = WorkerExitKind::StartupFailure;
                            protocol_error(error)
                        }
                    },
                    ControlRequestKind::SetActive(active) => match host.as_mut() {
                        Some(host) => {
                            host.set_active(active);
                            status.active = active;
                            ControlResponseKind::Ack
                        }
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::SetVisible(visible) => match host.as_mut() {
                        Some(host) => match host.set_visible(visible) {
                            Ok(()) => {
                                status.visible = host.is_visible();
                                ControlResponseKind::Ack
                            }
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::SaveState => match host.as_mut() {
                        Some(host) => match host.save_state() {
                            Ok(state) => ControlResponseKind::State(state),
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::RestoreState(state) => match host.as_mut() {
                        Some(host) => match host.restore_state(&state) {
                            Ok(()) => ControlResponseKind::Ack,
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::Status => {
                        if let Some(host) = host.as_mut() {
                            status.active = host.is_active();
                            status.visible = host.is_visible();
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
                let info = host.info();
                if block.audio_inputs.len() != info.audio_inputs {
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
                if info.midi_inputs > 0 {
                    host.set_midi_input_events(0, &midi)?;
                }
                host.process(block.frames as usize)?;
                let mut audio_outputs = Vec::with_capacity(info.audio_outputs);
                for index in 0..info.audio_outputs {
                    audio_outputs.push(
                        host.audio_output(index)
                            .ok_or_else(|| anyhow!("Carla audio output {index} disappeared"))?
                            [..block.frames as usize]
                            .to_vec(),
                    );
                }
                let mut midi_outputs = Vec::new();
                if info.midi_outputs > 0 {
                    midi_outputs.extend(
                        host.midi_output_events(0)?
                            .into_iter()
                            .map(|(frame_offset, data)| MidiEvent { frame_offset, data }),
                    );
                }
                processed_blocks.fetch_add(1, Ordering::Relaxed);
                let result = PrototypeBlockResult {
                    sequence: block.sequence,
                    generation: options.generation,
                    frames: block.frames,
                    audio_outputs,
                    midi_outputs,
                };
                result.validate()?;
                write_frame(&mut stream, &WorkerToParent::Process(result))?;
            }
        }
    }
}

#[derive(Debug, Default)]
struct BoundedLog {
    bytes: VecDeque<u8>,
    dropped: u64,
}

impl BoundedLog {
    fn push(&mut self, data: &[u8]) {
        if data.len() > LOG_CAPACITY {
            let dropped = self.bytes.len().saturating_add(data.len() - LOG_CAPACITY);
            self.dropped = self.dropped.saturating_add(dropped as u64);
            self.bytes.clear();
            self.bytes.extend(&data[data.len() - LOG_CAPACITY..]);
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(data.len())
            .saturating_sub(LOG_CAPACITY);
        for _ in 0..overflow {
            self.bytes.pop_front();
        }
        self.dropped = self.dropped.saturating_add(overflow as u64);
        self.bytes.extend(data);
    }

    fn snapshot(&self) -> LogSnapshot {
        LogSnapshot {
            bytes: self.bytes.iter().copied().collect(),
            dropped_bytes: self.dropped,
        }
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.dropped = 0;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LogSnapshot {
    pub bytes: Vec<u8>,
    pub dropped_bytes: u64,
}

fn drain_pipe(
    mut pipe: impl Read + Send + 'static,
    destination: Arc<Mutex<BoundedLog>>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("carla-worker-log".to_owned())
        .spawn(move || {
            let mut buffer = [0u8; 4096];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => destination
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(&buffer[..read]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
        .expect("worker log thread must start")
}

pub struct SubprocessCarlaProcessor {
    info: CarlaProcessorInfo,
    chain_id: ChainId,
    generation: ProcessGeneration,
    request_id: u64,
    block_sequence: u64,
    stream: TcpStream,
    notification: UdpSocket,
    notification_token: [u8; 16],
    shared_transport: SharedBlockTransport,
    child: Child,
    stdout: Arc<Mutex<BoundedLog>>,
    stderr: Arc<Mutex<BoundedLog>>,
    log_threads: Vec<JoinHandle<()>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_input_counts: Vec<usize>,
    midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_output_counts: Vec<usize>,
    shared_midi_outputs: Vec<MidiEvent>,
    shared_midi_output_count: usize,
    active: bool,
    visible: bool,
    ready: bool,
    checkpoint: String,
    process_timeout: Duration,
    deadline_misses: u64,
    midi_input_overflows: u64,
    midi_output_overflows: u64,
    stale_completions: u64,
    serialized_reference_transport: bool,
    exit_kind: WorkerExitKind,
    shutdown_complete: bool,
}

impl fmt::Debug for SubprocessCarlaProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubprocessCarlaProcessor")
            .field("info", &self.info)
            .field("chain_id", &self.chain_id)
            .field("generation", &self.generation)
            .field("active", &self.active)
            .field("visible", &self.visible)
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}

impl SubprocessCarlaProcessor {
    pub fn spawn(
        executable: impl AsRef<Path>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
    ) -> Result<Self> {
        Self::spawn_with_test_mode(
            executable,
            chain_type,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            generation,
            None,
        )
    }

    pub fn spawn_test_worker(
        executable: impl AsRef<Path>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
        test_mode: CarlaWorkerTestMode,
    ) -> Result<Self> {
        Self::spawn_with_test_mode(
            executable,
            chain_type,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            generation,
            Some(test_mode),
        )
    }

    fn spawn_with_test_mode(
        executable: impl AsRef<Path>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
        test_mode: Option<CarlaWorkerTestMode>,
    ) -> Result<Self> {
        if chain_id.0 == 0 || generation.0 == 0 {
            return Err(anyhow!("chain identity and generation must be nonzero"));
        }
        let protocol_type = protocol_chain_type(chain_type)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let nonce = new_nonce();
        let shared_transport = SharedBlockTransport::create(generation, &nonce)?;
        let mut command = Command::new(executable.as_ref());
        command
            .arg("--carla-worker")
            .arg("--carla-worker-address")
            .arg(address.to_string())
            .arg("--carla-worker-nonce")
            .arg(nonce_hex(&nonce))
            .arg("--carla-worker-chain-id")
            .arg(chain_id.0.to_string())
            .arg("--carla-worker-generation")
            .arg(generation.0.to_string())
            .arg("--carla-worker-shared-memory")
            .arg(shared_transport.path())
            .arg("--no-crash-handling");
        if let Some(test_mode) = test_mode {
            command
                .arg("--carla-worker-test-mode")
                .arg(test_mode.to_string());
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "could not launch Carla worker {}",
                    executable.as_ref().display()
                )
            })?;
        let stdout = Arc::new(Mutex::new(BoundedLog::default()));
        let stderr = Arc::new(Mutex::new(BoundedLog::default()));
        let mut log_threads = Vec::new();
        if let Some(pipe) = child.stdout.take() {
            log_threads.push(drain_pipe(pipe, Arc::clone(&stdout)));
        }
        if let Some(pipe) = child.stderr.take() {
            log_threads.push(drain_pipe(pipe, Arc::clone(&stderr)));
        }

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        let _ = child.kill();
                        return Err(anyhow!("worker connected from non-loopback address {peer}"));
                    }
                    break stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait()? {
                        return Err(anyhow!("Carla worker exited during startup: {status}"));
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(anyhow!("timed out waiting for Carla worker connection"));
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(error.into()),
            }
        };
        // Accepted sockets can inherit the listener's nonblocking mode on
        // Windows. All framed control exchanges below use bounded blocking
        // timeouts, so normalize the accepted stream explicitly.
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
        stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
        let hello: WorkerToParent = read_frame(&mut stream)?;
        let notification_port = match hello {
            WorkerToParent::Control(ControlResponse {
                request_id: RequestId(1),
                chain_id: response_chain,
                generation: response_generation,
                kind: ControlResponseKind::Handshake(hello),
            }) if response_chain == chain_id && response_generation == generation => {
                if let Err(error) = hello.validate(&nonce, generation) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.into());
                }
                hello.notification_port
            }
            other => {
                let _ = child.kill();
                return Err(anyhow!("invalid worker handshake: {other:?}"));
            }
        };
        let notification = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        notification.connect((std::net::Ipv4Addr::LOCALHOST, notification_port))?;
        notification.set_nonblocking(true)?;
        let notification_token = nonce[..16].try_into().expect("nonce prefix has fixed size");

        let channels = protocol_type.audio_channels() as usize;
        let mut processor = Self {
            info: CarlaProcessorInfo {
                chain_type,
                audio_inputs: channels,
                audio_outputs: channels,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            chain_id,
            generation,
            request_id: 1,
            block_sequence: 0,
            stream,
            notification,
            notification_token,
            shared_transport,
            child,
            stdout,
            stderr,
            log_threads,
            audio_inputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            audio_outputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            midi_inputs: vec![(0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| (0, Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES)))
                .collect()],
            midi_input_counts: vec![0],
            midi_outputs: vec![(0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| (0, Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES)))
                .collect()],
            midi_output_counts: vec![0],
            shared_midi_outputs: (0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| MidiEvent {
                    frame_offset: 0,
                    data: Vec::with_capacity(crate::midi_storage::MAX_MSG_BYTES),
                })
                .collect(),
            shared_midi_output_count: 0,
            active: false,
            visible: false,
            ready: false,
            checkpoint: "{}".to_owned(),
            process_timeout: Duration::from_secs_f64(
                (nominal_buffer_size.max(1) as f64 / sample_rate.max(1) as f64).max(0.000_5),
            ),
            deadline_misses: 0,
            midi_input_overflows: 0,
            midi_output_overflows: 0,
            stale_completions: 0,
            serialized_reference_transport: false,
            exit_kind: WorkerExitKind::None,
            shutdown_complete: false,
        };
        processor.control(ControlRequestKind::Instantiate {
            chain_type: protocol_type,
            sample_rate,
            nominal_buffer_size,
        })?;
        processor.ready = true;
        Ok(processor)
    }

    pub fn current_executable() -> Result<PathBuf> {
        std::env::current_exe().context("could not locate current executable for Carla worker")
    }

    fn next_request_id(&mut self) -> RequestId {
        self.request_id = self.request_id.saturating_add(1);
        RequestId(self.request_id)
    }

    fn control(&mut self, kind: ControlRequestKind) -> Result<ControlResponseKind> {
        self.control_with_timeout(kind, CONTROL_TIMEOUT)
    }

    fn control_with_timeout(
        &mut self,
        kind: ControlRequestKind,
        timeout: Duration,
    ) -> Result<ControlResponseKind> {
        self.stream.set_read_timeout(Some(timeout))?;
        let request_id = self.next_request_id();
        let request = ControlRequest {
            request_id,
            chain_id: self.chain_id,
            generation: self.generation,
            kind,
        };
        request.validate()?;
        write_frame(&mut self.stream, &ParentToWorker::Control(request))?;
        let response: WorkerToParent = read_frame(&mut self.stream)?;
        match response {
            WorkerToParent::Control(response)
                if response.request_id == request_id
                    && response.chain_id == self.chain_id
                    && response.generation == self.generation =>
            {
                response.validate()?;
                match response.kind {
                    ControlResponseKind::Error(error) => Err(anyhow!(
                        "Carla worker error {:?}: {}",
                        error.code,
                        error.message
                    )),
                    kind => Ok(kind),
                }
            }
            other => Err(anyhow!("unexpected Carla worker response: {other:?}")),
        }
    }

    pub fn status(&mut self) -> Result<WorkerStatus> {
        match self.control(ControlRequestKind::Status)? {
            ControlResponseKind::Status(mut status) => {
                status.deadline_misses = self.deadline_misses;
                status.midi_input_overflows = self.midi_input_overflows;
                status.midi_output_overflows = self.midi_output_overflows;
                status.stale_completions = self.stale_completions;
                Ok(status)
            }
            other => Err(anyhow!("worker returned {other:?} for status request")),
        }
    }

    pub fn stdout_snapshot(&self) -> LogSnapshot {
        self.stdout
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot()
    }

    pub fn stderr_snapshot(&self) -> LogSnapshot {
        self.stderr
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot()
    }

    pub fn worker_id(&self) -> u32 {
        self.child.id()
    }

    pub fn deadline_misses(&self) -> u64 {
        self.deadline_misses
    }

    pub fn shared_memory_path(&self) -> &Path {
        self.shared_transport.path()
    }

    pub fn use_serialized_reference_transport_for_benchmark(&mut self) {
        self.serialized_reference_transport = true;
    }

    fn process_serialized_reference(&mut self, frames: usize) -> Result<()> {
        if !self.is_active() {
            self.clear_outputs(frames.min(MAX_BLOCK_FRAMES));
            return Ok(());
        }
        if frames == 0 || frames > MAX_BLOCK_FRAMES {
            return Err(anyhow!("invalid serialized reference block size {frames}"));
        }
        self.block_sequence = self.block_sequence.saturating_add(1);
        let block = shoop_plugin_protocol::PrototypeBlock {
            sequence: BlockSequence(self.block_sequence),
            generation: self.generation,
            frames: frames as u32,
            audio_inputs: self
                .audio_inputs
                .iter()
                .map(|input| input[..frames].to_vec())
                .collect(),
            midi_inputs: self
                .midi_inputs
                .iter()
                .enumerate()
                .flat_map(|(channel, events)| {
                    events[..self.midi_input_counts[channel].min(events.len())]
                        .iter()
                        .map(|(frame_offset, data)| MidiEvent {
                            frame_offset: *frame_offset,
                            data: data.clone(),
                        })
                })
                .collect(),
        };
        block.validate()?;
        write_frame(&mut self.stream, &ParentToWorker::Process(block))?;
        let response: WorkerToParent = read_frame(&mut self.stream)?;
        let WorkerToParent::Process(result) = response else {
            return Err(anyhow!(
                "worker returned control traffic for reference block"
            ));
        };
        result.validate()?;
        if result.sequence != BlockSequence(self.block_sequence)
            || result.generation != self.generation
            || result.frames as usize != frames
            || result.audio_outputs.len() != self.audio_outputs.len()
        {
            return Err(anyhow!(
                "worker returned mismatched serialized reference block"
            ));
        }
        for (destination, source) in self.audio_outputs.iter_mut().zip(result.audio_outputs) {
            destination[..frames].copy_from_slice(&source);
        }
        self.midi_output_counts.fill(0);
        for (slot, event) in self.midi_outputs[0]
            .iter_mut()
            .zip(result.midi_outputs.iter())
        {
            if event.data.len() > crate::midi_storage::MAX_MSG_BYTES {
                continue;
            }
            slot.0 = event.frame_offset;
            slot.1.clear();
            slot.1.extend_from_slice(&event.data);
            self.midi_output_counts[0] += 1;
        }
        Ok(())
    }

    pub fn shutdown_requested(&mut self) -> WorkerExitKind {
        if self.shutdown_complete {
            return self.exit_kind;
        }
        self.exit_kind = WorkerExitKind::Requested;
        let _ = self.control(ControlRequestKind::Shutdown);
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                _ => {
                    self.exit_kind = WorkerExitKind::Unresponsive;
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        self.ready = false;
        self.shutdown_complete = true;
        self.exit_kind
    }

    pub fn terminate_worker_for_test(&mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        self.ready = false;
        Ok(())
    }

    pub fn clear_logs(&self) {
        self.stdout
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.stderr
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    fn clear_outputs(&mut self, frames: usize) {
        for output in &mut self.audio_outputs {
            let frames = frames.min(output.len());
            output[..frames].fill(0.0);
        }
        self.midi_output_counts.fill(0);
    }
}

impl CarlaProcessor for SubprocessCarlaProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn is_ready(&mut self) -> bool {
        self.ready && self.child.try_wait().ok().flatten().is_none()
    }

    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        if self.ready {
            CarlaProcessorLifecycle::Running
        } else {
            CarlaProcessorLifecycle::Crashed
        }
    }

    fn generation(&self) -> u64 {
        self.generation.0
    }

    fn exit_kind(&self) -> WorkerExitKind {
        self.exit_kind
    }

    fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
        let stdout = self.stdout_snapshot();
        let stderr = self.stderr_snapshot();
        vec![CarlaGenerationLog {
            generation: self.generation.0,
            stdout: stdout.bytes,
            stderr: stderr.bytes,
            stdout_dropped_bytes: stdout.dropped_bytes,
            stderr_dropped_bytes: stderr.dropped_bytes,
        }]
    }

    fn clear_logs(&mut self) {
        SubprocessCarlaProcessor::clear_logs(self);
    }

    fn set_active(&mut self, active: bool) {
        if self.control(ControlRequestKind::SetActive(active)).is_ok() {
            self.active = active;
        } else {
            self.ready = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active && self.ready
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.control_with_timeout(ControlRequestKind::SetVisible(visible), UI_CONTROL_TIMEOUT)?;
        self.visible = visible;
        Ok(())
    }

    fn is_visible(&mut self) -> bool {
        match self.status() {
            Ok(status) => {
                if self.visible && !status.visible {
                    self.exit_kind = WorkerExitKind::UiClosed;
                }
                self.visible = status.visible;
                self.ready = status.ready;
            }
            Err(_) => {
                self.visible = false;
                self.ready = false;
            }
        }
        self.visible
    }

    fn save_state(&mut self) -> Result<String> {
        match self.control(ControlRequestKind::SaveState) {
            Ok(ControlResponseKind::State(state)) => {
                self.checkpoint.clone_from(&state);
                Ok(state)
            }
            Ok(other) => Err(anyhow!("worker returned {other:?} for state save")),
            Err(error) => Err(error),
        }
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        self.control(ControlRequestKind::RestoreState(state.to_owned()))?;
        self.checkpoint.clear();
        self.checkpoint.push_str(state);
        Ok(())
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.audio_inputs.get_mut(index).map(Vec::as_mut_slice)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.audio_outputs.get(index).map(Vec::as_slice)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        let destination = self
            .midi_inputs
            .get_mut(index)
            .ok_or_else(|| anyhow!("no subprocess MIDI input {index}"))?;
        let mut count = 0;
        for (frame_offset, data) in events {
            if count == destination.len() || data.len() > crate::midi_storage::MAX_MSG_BYTES {
                self.midi_input_overflows = self.midi_input_overflows.saturating_add(1);
                continue;
            }
            destination[count].0 = *frame_offset;
            destination[count].1.clear();
            destination[count].1.extend_from_slice(data);
            count += 1;
        }
        self.midi_input_counts[index] = count;
        Ok(())
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        let output = self
            .midi_outputs
            .get(index)
            .ok_or_else(|| anyhow!("no subprocess MIDI output {index}"))?;
        Ok(output[..self.midi_output_counts[index]].to_vec())
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        let output = self
            .midi_outputs
            .get(index)
            .ok_or_else(|| anyhow!("no subprocess MIDI output {index}"))?;
        for (frame_offset, data) in &output[..self.midi_output_counts[index]] {
            destination.push(*frame_offset, data)?;
        }
        Ok(())
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        if self.serialized_reference_transport {
            return self.process_serialized_reference(frames);
        }
        if !self.is_active() {
            self.clear_outputs(frames);
            return Ok(());
        }
        if frames == 0 || frames > MAX_BLOCK_FRAMES {
            self.clear_outputs(frames.min(MAX_BLOCK_FRAMES));
            return Err(anyhow!("invalid subprocess block size {frames}"));
        }
        self.block_sequence = self.block_sequence.saturating_add(1);
        let token = match {
            let _span = shoop_tracing::realtime_span_detail!(
                "engine.rt.fx.subprocess_submit",
                value = frames
            );
            self.shared_transport.submit(
                BlockSequence(self.block_sequence),
                frames,
                &self.audio_inputs,
                self.audio_outputs.len(),
                &self.midi_inputs,
                &self.midi_input_counts,
            )
        } {
            Ok(token) => {
                if self.notification.send(&self.notification_token)?
                    != self.notification_token.len()
                {
                    self.ready = false;
                    self.clear_outputs(frames);
                    return Err(anyhow!("short Carla worker notification datagram"));
                }
                token
            }
            Err(
                crate::carla_shared_memory::SharedBlockError::NoFreeSlot
                | crate::carla_shared_memory::SharedBlockError::DeadlineMiss,
            ) => {
                self.deadline_misses = self.deadline_misses.saturating_add(1);
                self.clear_outputs(frames);
                return Ok(());
            }
            Err(error) => {
                self.ready = false;
                self.clear_outputs(frames);
                return Err(error.into());
            }
        };
        let result = {
            let _span = shoop_tracing::realtime_span_detail!(
                "engine.rt.fx.subprocess_wait",
                value = frames
            );
            self.shared_midi_output_count = 0;
            self.shared_transport.wait_and_copy_reusing_midi(
                token,
                Instant::now() + self.process_timeout,
                &mut self.audio_outputs,
                &mut self.shared_midi_outputs,
                &mut self.shared_midi_output_count,
            )
        };
        match result {
            Ok(()) => {
                self.midi_output_counts.fill(0);
                for (slot, event) in self.midi_outputs[0]
                    .iter_mut()
                    .zip(self.shared_midi_outputs[..self.shared_midi_output_count].iter())
                {
                    slot.0 = event.frame_offset;
                    slot.1.clear();
                    slot.1.extend_from_slice(&event.data);
                    self.midi_output_counts[0] += 1;
                }
                Ok(())
            }
            Err(
                crate::carla_shared_memory::SharedBlockError::NoFreeSlot
                | crate::carla_shared_memory::SharedBlockError::DeadlineMiss,
            ) => {
                self.deadline_misses = self.deadline_misses.saturating_add(1);
                self.clear_outputs(frames);
                Ok(())
            }
            Err(crate::carla_shared_memory::SharedBlockError::MidiOverflow) => {
                self.midi_output_overflows = self.midi_output_overflows.saturating_add(1);
                self.clear_outputs(frames);
                Ok(())
            }
            Err(crate::carla_shared_memory::SharedBlockError::StaleCompletion) => {
                self.stale_completions = self.stale_completions.saturating_add(1);
                self.clear_outputs(frames);
                Ok(())
            }
            Err(error) => {
                self.ready = false;
                self.clear_outputs(frames);
                Err(error.into())
            }
        }
    }
}

pub struct SupervisedCarlaProcessor {
    executable: PathBuf,
    info: CarlaProcessorInfo,
    sample_rate: u32,
    nominal_buffer_size: u32,
    chain_id: ChainId,
    test_mode: Option<CarlaWorkerTestMode>,
    generation: ProcessGeneration,
    current: Option<SubprocessCarlaProcessor>,
    previous_logs: VecDeque<CarlaGenerationLog>,
    checkpoint: String,
    has_checkpoint: bool,
    desired_active: bool,
    desired_visible: bool,
    lifecycle: CarlaProcessorLifecycle,
    exit_kind: WorkerExitKind,
    crash_summary: Option<String>,
}

impl fmt::Debug for SupervisedCarlaProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupervisedCarlaProcessor")
            .field("info", &self.info)
            .field("chain_id", &self.chain_id)
            .field("generation", &self.generation)
            .field("lifecycle", &self.lifecycle)
            .field("desired_active", &self.desired_active)
            .field("desired_visible", &self.desired_visible)
            .field("crash_summary", &self.crash_summary)
            .finish_non_exhaustive()
    }
}

impl SupervisedCarlaProcessor {
    pub fn launch(
        executable: impl Into<PathBuf>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
    ) -> Result<Self> {
        Self::launch_with_test_mode(
            executable,
            chain_type,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            None,
        )
    }

    pub fn launch_test_worker(
        executable: impl Into<PathBuf>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        test_mode: CarlaWorkerTestMode,
    ) -> Result<Self> {
        Self::launch_with_test_mode(
            executable,
            chain_type,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            Some(test_mode),
        )
    }

    fn launch_with_test_mode(
        executable: impl Into<PathBuf>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        test_mode: Option<CarlaWorkerTestMode>,
    ) -> Result<Self> {
        let channels = protocol_chain_type(chain_type)?.audio_channels() as usize;
        let mut supervisor = Self {
            executable: executable.into(),
            info: CarlaProcessorInfo {
                chain_type,
                audio_inputs: channels,
                audio_outputs: channels,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            sample_rate,
            nominal_buffer_size,
            chain_id,
            test_mode,
            generation: ProcessGeneration(0),
            current: None,
            previous_logs: VecDeque::new(),
            checkpoint: String::new(),
            has_checkpoint: false,
            desired_active: false,
            desired_visible: false,
            lifecycle: CarlaProcessorLifecycle::Stopped,
            exit_kind: WorkerExitKind::None,
            crash_summary: None,
        };
        supervisor.start_generation(false);
        Ok(supervisor)
    }

    fn retain_current_logs(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let mut logs = current.generation_logs();
        if let Some(log) = logs.pop() {
            if self.previous_logs.len() == MAX_LOG_GENERATIONS {
                self.previous_logs.pop_front();
            }
            self.previous_logs.push_back(log);
        }
    }

    fn start_generation(&mut self, show_after_restore: bool) {
        self.exit_kind = WorkerExitKind::None;
        self.lifecycle = if self.generation.0 == 0 {
            CarlaProcessorLifecycle::Starting
        } else {
            CarlaProcessorLifecycle::Restarting
        };
        if self.current.is_some() {
            self.retain_current_logs();
            drop(self.current.take());
        }
        self.generation = ProcessGeneration(self.generation.0.saturating_add(1));
        let started = SubprocessCarlaProcessor::spawn_with_test_mode(
            &self.executable,
            self.info.chain_type,
            self.sample_rate,
            self.nominal_buffer_size,
            self.chain_id,
            self.generation,
            self.test_mode,
        );
        let mut current = match started {
            Ok(current) => current,
            Err(error) => {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.exit_kind = WorkerExitKind::StartupFailure;
                self.crash_summary = Some(format!("Carla worker startup failed: {error}"));
                return;
            }
        };
        if self.has_checkpoint {
            if let Err(error) = current.restore_state(&self.checkpoint) {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.exit_kind = WorkerExitKind::StartupFailure;
                self.crash_summary = Some(format!("state restore failed: {error}"));
                self.current = Some(current);
                return;
            }
        }
        current.set_active(self.desired_active);
        if show_after_restore {
            if let Err(error) = current.set_visible(true) {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.exit_kind = WorkerExitKind::StartupFailure;
                self.crash_summary = Some(format!("external UI start failed: {error}"));
                self.current = Some(current);
                return;
            }
            self.desired_visible = true;
        }
        self.current = Some(current);
        self.lifecycle = CarlaProcessorLifecycle::Running;
        self.exit_kind = WorkerExitKind::None;
        self.crash_summary = None;
    }

    pub fn worker_id(&self) -> Option<u32> {
        self.current
            .as_ref()
            .map(SubprocessCarlaProcessor::worker_id)
    }

    pub fn terminate_worker_for_test(&mut self) -> Result<()> {
        self.current
            .as_mut()
            .ok_or_else(|| anyhow!("no current worker"))?
            .terminate_worker_for_test()
    }

    pub fn restart_without_ui_for_test(&mut self) -> Result<()> {
        self.start_generation(false);
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            Ok(())
        } else {
            Err(anyhow!(
                "Carla worker restart failed: {}",
                self.crash_summary.as_deref().unwrap_or("unknown error")
            ))
        }
    }
}

impl CarlaProcessor for SupervisedCarlaProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn is_ready(&mut self) -> bool {
        if let Some(current) = self.current.as_mut() {
            if !current.is_ready() && self.lifecycle == CarlaProcessorLifecycle::Running {
                self.lifecycle = CarlaProcessorLifecycle::Crashed;
                self.exit_kind = WorkerExitKind::UnexpectedExit;
                let status = current.child.try_wait().ok().flatten();
                self.crash_summary = Some(match status {
                    Some(status) => format!("Carla worker exited unexpectedly: {status}"),
                    None => "Carla worker disconnected unexpectedly".to_owned(),
                });
            }
        }
        self.lifecycle == CarlaProcessorLifecycle::Running
    }

    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        self.lifecycle
    }

    fn generation(&self) -> u64 {
        self.generation.0
    }

    fn exit_kind(&self) -> WorkerExitKind {
        self.exit_kind
    }

    fn crash_summary(&self) -> Option<String> {
        self.crash_summary.clone()
    }

    fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
        let mut logs: Vec<_> = self.previous_logs.iter().cloned().collect();
        if let Some(current) = self.current.as_ref() {
            logs.extend(current.generation_logs());
        }
        logs
    }

    fn clear_logs(&mut self) {
        self.previous_logs.clear();
        if let Some(current) = self.current.as_mut() {
            current.clear_logs();
        }
    }

    fn toggle_or_recover(&mut self) -> Result<()> {
        if matches!(
            self.lifecycle,
            CarlaProcessorLifecycle::Crashed | CarlaProcessorLifecycle::Unavailable
        ) {
            self.start_generation(true);
            if self.lifecycle == CarlaProcessorLifecycle::Running {
                Ok(())
            } else {
                Err(anyhow!(
                    "Carla worker recovery failed: {}",
                    self.crash_summary.as_deref().unwrap_or("unknown error")
                ))
            }
        } else {
            let visible = self.is_visible();
            self.set_visible(!visible)
        }
    }

    fn set_active(&mut self, active: bool) {
        self.desired_active = active;
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            if let Some(current) = self.current.as_mut() {
                current.set_active(active);
                if !current.is_ready() {
                    self.lifecycle = CarlaProcessorLifecycle::Crashed;
                    self.exit_kind = WorkerExitKind::ProtocolFailure;
                    self.crash_summary = Some("Carla worker rejected active state".to_owned());
                }
            }
        }
    }

    fn is_active(&self) -> bool {
        self.desired_active && self.lifecycle == CarlaProcessorLifecycle::Running
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.desired_visible = visible;
        if visible
            && matches!(
                self.lifecycle,
                CarlaProcessorLifecycle::Crashed | CarlaProcessorLifecycle::Unavailable
            )
        {
            return self.toggle_or_recover();
        }
        let current = self
            .current
            .as_mut()
            .ok_or_else(|| anyhow!("Carla worker is unavailable"))?;
        current.set_visible(visible)
    }

    fn is_visible(&mut self) -> bool {
        if self.lifecycle != CarlaProcessorLifecycle::Running {
            return false;
        }
        self.current
            .as_mut()
            .is_some_and(CarlaProcessor::is_visible)
    }

    fn save_state(&mut self) -> Result<String> {
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            if let Some(current) = self.current.as_mut() {
                if let Ok(state) = current.save_state() {
                    self.checkpoint.clone_from(&state);
                    self.has_checkpoint = true;
                }
            }
        }
        Ok(self.checkpoint.clone())
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            let current = self
                .current
                .as_mut()
                .ok_or_else(|| anyhow!("Carla worker is unavailable"))?;
            current.restore_state(state)?;
        }
        self.checkpoint.clear();
        self.checkpoint.push_str(state);
        self.has_checkpoint = true;
        Ok(())
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.current.as_mut()?.audio_input_mut(index)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.current.as_ref()?.audio_output(index)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        match self.current.as_mut() {
            Some(current) => current.set_midi_input_events(index, events),
            None => Ok(()),
        }
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        match self.current.as_mut() {
            Some(current) => current.midi_output_events(index),
            None => Ok(Vec::new()),
        }
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        match self.current.as_mut() {
            Some(current) => current.fill_midi_output_events(index, destination),
            None => {
                destination.clear();
                Ok(())
            }
        }
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        if self.lifecycle != CarlaProcessorLifecycle::Running {
            return Err(anyhow!("Carla worker is not running"));
        }
        let result = self
            .current
            .as_mut()
            .ok_or_else(|| anyhow!("Carla worker is unavailable"))?
            .process(frames);
        if let Err(error) = &result {
            self.lifecycle = CarlaProcessorLifecycle::Crashed;
            self.exit_kind = if self
                .current
                .as_mut()
                .is_some_and(|current| !current.is_ready())
            {
                WorkerExitKind::UnexpectedExit
            } else {
                WorkerExitKind::ProtocolFailure
            };
            self.crash_summary = Some(error.to_string());
        }
        result
    }
}

impl Drop for SubprocessCarlaProcessor {
    fn drop(&mut self) {
        self.shutdown_requested();
        for thread in self.log_threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_round_trip_and_validation() {
        let nonce = new_nonce();
        assert_eq!(parse_nonce(&nonce_hex(&nonce)).unwrap(), nonce);
        assert!(parse_nonce("bad").is_err());
    }

    #[test]
    fn bounded_logs_disclose_and_clear_truncation() {
        let mut log = BoundedLog::default();
        log.push(&[0xff, 0x00, 0xfe]);
        log.push(&vec![1; LOG_CAPACITY + 100]);
        let snapshot = log.snapshot();
        assert_eq!(snapshot.bytes.len(), LOG_CAPACITY);
        assert_eq!(snapshot.dropped_bytes, 103);
        log.clear();
        assert_eq!(log.snapshot(), LogSnapshot::default());
    }

    #[test]
    fn pipe_drain_handles_binary_flood_without_blocking() {
        let destination = Arc::new(Mutex::new(BoundedLog::default()));
        let mut flood = Vec::with_capacity(LOG_CAPACITY * 4 + 3);
        flood.extend_from_slice(&[0xff, 0x00, 0xfe]);
        flood.resize(LOG_CAPACITY * 4 + 3, 0x5a);
        let thread = drain_pipe(std::io::Cursor::new(flood), Arc::clone(&destination));
        thread.join().unwrap();
        let snapshot = destination
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot();
        assert_eq!(snapshot.bytes.len(), LOG_CAPACITY);
        assert_eq!(snapshot.dropped_bytes, (LOG_CAPACITY * 3 + 3) as u64);
    }
}
