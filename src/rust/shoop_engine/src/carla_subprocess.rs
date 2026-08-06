use crate::carla_processor::{
    CarlaGenerationLog, CarlaProcessor, CarlaProcessorInfo, CarlaProcessorLifecycle,
};
use crate::carla_shared_memory::SharedBlockTransport;
use crate::lv2_carla::CarlaLv2Host;
use crate::realtime_lock_guard::Mutex;
use crate::FXChainType;
use anyhow::{anyhow, Context, Result};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, ChainId, ControlRequest,
    ControlRequestKind, ControlResponse, ControlResponseKind, LifecycleState, MidiEvent,
    ParentToWorker, ProcessGeneration, ProtocolError, ProtocolErrorCode, PrototypeBlockResult,
    RequestId, WorkerExitKind, WorkerHello, WorkerStatus, WorkerToParent, MAX_BLOCK_FRAMES,
};
use std::collections::VecDeque;
use std::fmt;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
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
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

fn protocol_chain_type(chain_type: FXChainType) -> Result<CarlaChainType> {
    match chain_type {
        FXChainType::CarlaRack => Ok(CarlaChainType::Rack),
        FXChainType::CarlaPatchbay => Ok(CarlaChainType::Patchbay),
        FXChainType::CarlaPatchbay16x => Ok(CarlaChainType::Patchbay16x),
        FXChainType::Test2x2x1 => Err(anyhow!("test FX chain is not a Carla worker type")),
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

#[derive(Debug, Clone)]
pub struct CarlaWorkerOptions {
    pub address: SocketAddr,
    pub nonce: [u8; 32],
    pub chain_id: ChainId,
    pub generation: ProcessGeneration,
    pub shared_memory_path: PathBuf,
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

fn run_shared_worker(
    mut transport: SharedBlockTransport,
    host: Arc<Mutex<Option<CarlaLv2Host>>>,
    stop: Arc<AtomicBool>,
    processed_blocks: Arc<AtomicU64>,
) {
    let mut midi_inputs = Vec::with_capacity(shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK);
    while !stop.load(Ordering::Acquire) {
        let Some(token) = transport.worker_take() else {
            std::hint::spin_loop();
            thread::yield_now();
            continue;
        };
        let result = (|| -> Result<()> {
            let mut host = host.lock().unwrap_or_else(|error| error.into_inner());
            let host = host
                .as_mut()
                .ok_or_else(|| anyhow!("shared block arrived before Carla instantiation"))?;
            if transport.worker_audio_input_channels(token) != host.info.ports.audio_inputs.len()
                || transport.worker_audio_output_channels(token)
                    != host.info.ports.audio_outputs.len()
            {
                return Err(anyhow!(
                    "shared block channel layout does not match Carla host"
                ));
            }
            for channel in 0..host.info.ports.audio_inputs.len() {
                let destination = host
                    .audio_input_mut(channel)
                    .ok_or_else(|| anyhow!("Carla audio input {channel} disappeared"))?;
                transport.worker_copy_audio_input(token, channel, destination)?;
            }
            transport.worker_read_midi(token, &mut midi_inputs)?;
            if !host.info.ports.midi_inputs.is_empty() {
                host.set_midi_input_events(
                    0,
                    midi_inputs
                        .iter()
                        .map(|event| (event.frame_offset, event.data.as_slice())),
                )?;
            }
            host.process(token.frames)?;
            let mut midi_outputs = Vec::new();
            if !host.info.ports.midi_outputs.is_empty() {
                midi_outputs.extend(
                    host.midi_output_events(0)?
                        .into_iter()
                        .map(|(frame_offset, data)| MidiEvent { frame_offset, data }),
                );
            }
            let audio_outputs: Vec<_> = (0..host.info.ports.audio_outputs.len())
                .map(|channel| {
                    host.audio_output(channel)
                        .ok_or_else(|| anyhow!("Carla audio output {channel} disappeared"))
                })
                .collect::<Result<_>>()?;
            transport.worker_complete(token, &audio_outputs, &midi_outputs)?;
            processed_blocks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Carla shared-memory worker failed: {error:#}");
            stop.store(true, Ordering::Release);
        }
    }
}

pub fn run_carla_worker(options: CarlaWorkerOptions) -> Result<()> {
    let mut stream = TcpStream::connect_timeout(&options.address, STARTUP_TIMEOUT)
        .context("worker could not connect to parent")?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
    let hello = WorkerHello::current(options.nonce, options.generation);
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
    let host: Arc<Mutex<Option<CarlaLv2Host>>> = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let processed_blocks = Arc::new(AtomicU64::new(0));
    let shared_thread = thread::Builder::new()
        .name("carla-worker-realtime".to_owned())
        .spawn({
            let host = Arc::clone(&host);
            let stop = Arc::clone(&stop);
            let processed_blocks = Arc::clone(&processed_blocks);
            move || run_shared_worker(shared_transport, host, stop, processed_blocks)
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
        let message: ParentToWorker = match read_frame(&mut stream) {
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
                    } => match CarlaLv2Host::instantiate(
                        engine_chain_type(chain_type),
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
                        Some(host) => match host.save_state_string() {
                            Ok(state) => ControlResponseKind::State(state),
                            Err(error) => protocol_error(error),
                        },
                        None => protocol_error("Carla host is not instantiated"),
                    },
                    ControlRequestKind::RestoreState(state) => match host.as_mut() {
                        Some(host) => match host.restore_state_string(&state) {
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
                if block.audio_inputs.len() != host.info.ports.audio_inputs.len() {
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
                if !host.info.ports.midi_inputs.is_empty() {
                    host.set_midi_input_events(0, midi)?;
                }
                host.process(block.frames as usize)?;
                let mut audio_outputs = Vec::with_capacity(host.info.ports.audio_outputs.len());
                for index in 0..host.info.ports.audio_outputs.len() {
                    audio_outputs.push(
                        host.audio_output(index)
                            .ok_or_else(|| anyhow!("Carla audio output {index} disappeared"))?
                            [..block.frames as usize]
                            .to_vec(),
                    );
                }
                let mut midi_outputs = Vec::new();
                if !host.info.ports.midi_outputs.is_empty() {
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
        let overflow = self
            .bytes
            .len()
            .saturating_add(data.len())
            .saturating_sub(LOG_CAPACITY);
        for _ in 0..overflow.min(self.bytes.len()) {
            self.bytes.pop_front();
        }
        if data.len() > LOG_CAPACITY {
            let dropped_from_data = data.len() - LOG_CAPACITY;
            self.dropped = self.dropped.saturating_add(dropped_from_data as u64);
            self.bytes.extend(&data[dropped_from_data..]);
        } else {
            self.dropped = self.dropped.saturating_add(overflow as u64);
            self.bytes.extend(data);
        }
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
    shared_transport: SharedBlockTransport,
    child: Child,
    stdout: Arc<Mutex<BoundedLog>>,
    stderr: Arc<Mutex<BoundedLog>>,
    log_threads: Vec<JoinHandle<()>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
    shared_midi_outputs: Vec<MidiEvent>,
    active: bool,
    visible: bool,
    ready: bool,
    checkpoint: String,
    process_timeout: Duration,
    deadline_misses: u64,
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
        if chain_id.0 == 0 || generation.0 == 0 {
            return Err(anyhow!("chain identity and generation must be nonzero"));
        }
        let protocol_type = protocol_chain_type(chain_type)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let nonce = new_nonce();
        let shared_transport = SharedBlockTransport::create(generation, &nonce)?;
        let shared_memory_path = shared_transport.path().to_string_lossy().to_string();
        let mut child = Command::new(executable.as_ref())
            .args([
                "--carla-worker",
                "--carla-worker-address",
                &address.to_string(),
                "--carla-worker-nonce",
                &nonce_hex(&nonce),
                "--carla-worker-chain-id",
                &chain_id.0.to_string(),
                "--carla-worker-generation",
                &generation.0.to_string(),
                "--carla-worker-shared-memory",
                &shared_memory_path,
                "--no-crash-handling",
            ])
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
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
        stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
        let hello: WorkerToParent = read_frame(&mut stream)?;
        match hello {
            WorkerToParent::Control(ControlResponse {
                request_id: RequestId(1),
                chain_id: response_chain,
                generation: response_generation,
                kind: ControlResponseKind::Handshake(hello),
            }) if response_chain == chain_id && response_generation == generation => {
                hello.validate(&nonce, generation)?;
            }
            other => {
                let _ = child.kill();
                return Err(anyhow!("invalid worker handshake: {other:?}"));
            }
        }

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
            shared_transport,
            child,
            stdout,
            stderr,
            log_threads,
            audio_inputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            audio_outputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            midi_inputs: vec![Vec::new()],
            midi_outputs: vec![Vec::with_capacity(
                shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK,
            )],
            shared_midi_outputs: Vec::with_capacity(
                shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK,
            ),
            active: false,
            visible: false,
            ready: false,
            checkpoint: "{}".to_owned(),
            process_timeout: Duration::from_secs_f64(
                (nominal_buffer_size.max(1) as f64 / sample_rate.max(1) as f64).max(0.000_5),
            ),
            deadline_misses: 0,
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
        self.stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
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
        for output in &mut self.midi_outputs {
            output.clear();
        }
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
        self.control(ControlRequestKind::SetVisible(visible))?;
        self.visible = visible;
        Ok(())
    }

    fn is_visible(&mut self) -> bool {
        match self.status() {
            Ok(status) => {
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
        destination.clear();
        destination.extend(events.iter().map(|(offset, data)| (*offset, data.to_vec())));
        Ok(())
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        self.midi_outputs
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("no subprocess MIDI output {index}"))
    }

    fn process(&mut self, frames: usize) -> Result<()> {
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
            )
        } {
            Ok(token) => token,
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
            self.shared_transport.wait_and_copy(
                token,
                Instant::now() + self.process_timeout,
                &mut self.audio_outputs,
                &mut self.shared_midi_outputs,
            )
        };
        match result {
            Ok(()) => {
                self.midi_outputs[0].clear();
                self.midi_outputs[0].extend(
                    self.shared_midi_outputs
                        .drain(..)
                        .map(|event| (event.frame_offset, event.data)),
                );
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
    generation: ProcessGeneration,
    current: Option<SubprocessCarlaProcessor>,
    previous_logs: VecDeque<CarlaGenerationLog>,
    checkpoint: String,
    has_checkpoint: bool,
    desired_active: bool,
    desired_visible: bool,
    lifecycle: CarlaProcessorLifecycle,
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
            generation: ProcessGeneration(0),
            current: None,
            previous_logs: VecDeque::new(),
            checkpoint: String::new(),
            has_checkpoint: false,
            desired_active: false,
            desired_visible: false,
            lifecycle: CarlaProcessorLifecycle::Stopped,
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
        let started = SubprocessCarlaProcessor::spawn(
            &self.executable,
            self.info.chain_type,
            self.sample_rate,
            self.nominal_buffer_size,
            self.chain_id,
            self.generation,
        );
        let mut current = match started {
            Ok(current) => current,
            Err(error) => {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.crash_summary = Some(error.to_string());
                return;
            }
        };
        if self.has_checkpoint {
            if let Err(error) = current.restore_state(&self.checkpoint) {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.crash_summary = Some(format!("state restore failed: {error}"));
                self.current = Some(current);
                return;
            }
        }
        current.set_active(self.desired_active);
        if show_after_restore {
            if let Err(error) = current.set_visible(true) {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.crash_summary = Some(format!("external UI start failed: {error}"));
                self.current = Some(current);
                return;
            }
            self.desired_visible = true;
        }
        self.current = Some(current);
        self.lifecycle = CarlaProcessorLifecycle::Running;
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
                self.crash_summary = Some("Carla worker exited unexpectedly".to_owned());
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

    fn crash_summary(&self) -> Option<&str> {
        self.crash_summary.as_deref()
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
            self.crash_summary = Some(error.to_string());
        }
        result
    }
}

impl Drop for SubprocessCarlaProcessor {
    fn drop(&mut self) {
        let _ = self.control(ControlRequestKind::Shutdown);
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
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
        log.push(&vec![1; LOG_CAPACITY + 100]);
        let snapshot = log.snapshot();
        assert_eq!(snapshot.bytes.len(), LOG_CAPACITY);
        assert_eq!(snapshot.dropped_bytes, 100);
        log.clear();
        assert_eq!(log.snapshot(), LogSnapshot::default());
    }
}
