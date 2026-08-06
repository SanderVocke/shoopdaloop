use crate::carla_processor::{CarlaProcessor, CarlaProcessorInfo};
use crate::lv2_carla::CarlaLv2Host;
use crate::FXChainType;
use anyhow::{anyhow, Context, Result};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, ChainId, ControlRequest,
    ControlRequestKind, ControlResponse, ControlResponseKind, LifecycleState, MidiEvent,
    ParentToWorker, ProcessGeneration, ProtocolError, ProtocolErrorCode, PrototypeBlock,
    PrototypeBlockResult, RequestId, WorkerExitKind, WorkerHello, WorkerStatus, WorkerToParent,
    MAX_BLOCK_FRAMES,
};
use std::collections::VecDeque;
use std::fmt;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const LOG_CAPACITY: usize = 64 * 1024;
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

    let mut host: Option<CarlaLv2Host> = None;
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
                            host = Some(created);
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
                status.processed_blocks = status.processed_blocks.saturating_add(1);
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
    child: Child,
    stdout: Arc<Mutex<BoundedLog>>,
    stderr: Arc<Mutex<BoundedLog>>,
    log_threads: Vec<JoinHandle<()>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
    active: bool,
    visible: bool,
    ready: bool,
    checkpoint: String,
    process_timeout: Duration,
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
            child,
            stdout,
            stderr,
            log_threads,
            audio_inputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            audio_outputs: vec![vec![0.0; MAX_BLOCK_FRAMES]; channels],
            midi_inputs: vec![Vec::new()],
            midi_outputs: vec![Vec::new()],
            active: false,
            visible: false,
            ready: false,
            checkpoint: "{}".to_owned(),
            process_timeout: Duration::from_secs_f64(
                (nominal_buffer_size.max(1) as f64 / sample_rate.max(1) as f64).max(0.000_5),
            ),
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
            ControlResponseKind::Status(status) => Ok(status),
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
            Err(_) => Ok(self.checkpoint.clone()),
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
        let block = PrototypeBlock {
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
                .flatten()
                .map(|(frame_offset, data)| MidiEvent {
                    frame_offset: *frame_offset,
                    data: data.clone(),
                })
                .collect(),
        };
        block.validate()?;
        self.stream.set_read_timeout(Some(self.process_timeout))?;
        let result = (|| -> Result<PrototypeBlockResult> {
            write_frame(&mut self.stream, &ParentToWorker::Process(block))?;
            let response: WorkerToParent = read_frame(&mut self.stream)?;
            match response {
                WorkerToParent::Process(result)
                    if result.sequence == BlockSequence(self.block_sequence)
                        && result.generation == self.generation =>
                {
                    result.validate()?;
                    Ok(result)
                }
                other => Err(anyhow!("unexpected process response: {other:?}")),
            }
        })();
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.ready = false;
                self.clear_outputs(frames);
                return Err(error);
            }
        };
        if result.audio_outputs.len() != self.audio_outputs.len() {
            self.ready = false;
            self.clear_outputs(frames);
            return Err(anyhow!("worker returned wrong audio output channel count"));
        }
        for (destination, source) in self.audio_outputs.iter_mut().zip(result.audio_outputs) {
            destination[..frames].copy_from_slice(&source);
        }
        self.midi_outputs[0] = result
            .midi_outputs
            .into_iter()
            .map(|event| (event.frame_offset, event.data))
            .collect();
        Ok(())
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
