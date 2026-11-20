//! Parent-side control and supervision for a dedicated Carla child.

use crate::carla_shared_memory::{BlockTime, EditorVisibility, SharedBlockTransport};
use crate::child::CarlaWorkerTestMode;
use crate::processor::{
    CarlaExternalUi, CarlaGenerationLog, CarlaProcessor, CarlaProcessorInfo,
    CarlaProcessorLifecycle,
};
use anyhow::{anyhow, Context, Result};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, ChainId, ControlRequest,
    ControlRequestKind, ControlResponse, ControlResponseKind, MidiEvent, ParentToWorker,
    PreparedMidiBuffer as CarlaMidiBuffer, ProcessGeneration, RequestId, WorkerExitKind,
    WorkerStatus, WorkerToParent, MAX_BLOCK_FRAMES, MAX_MIDI_EVENTS_PER_BLOCK,
    MAX_NATIVE_MIDI_PACKET_BYTES,
};
use std::{
    collections::VecDeque,
    fmt,
    io::Read,
    net::{TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const LOG_CAPACITY: usize = 64 * 1024;
const MAX_LOG_GENERATIONS: usize = 8;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const UI_CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub fn nonce_hex(nonce: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in nonce {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

pub fn new_nonce() -> Result<[u8; 32]> {
    let mut bytes = [std::mem::MaybeUninit::uninit(); 32];
    let bytes = getrandom::fill_uninit(&mut bytes)
        .map_err(|error| anyhow!("could not generate Carla worker nonce: {error}"))?;
    Ok(bytes.try_into().expect("the nonce buffer has 32 bytes"))
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

#[derive(Debug)]
struct SubprocessControlLink {
    chain_id: ChainId,
    generation: ProcessGeneration,
    request_id: AtomicU64,
    stream: Mutex<TcpStream>,
    visible: AtomicBool,
    editor_visibility: EditorVisibility,
    ui_closed: AtomicBool,
    ready: AtomicBool,
}

#[derive(Debug)]
struct SubprocessExternalUi {
    control: Arc<SubprocessControlLink>,
}

impl CarlaExternalUi for SubprocessExternalUi {
    fn set_visible(&self, visible: bool) -> Result<()> {
        control_exchange(
            &self.control,
            ControlRequestKind::SetVisible(visible),
            UI_CONTROL_TIMEOUT,
        )?;
        self.control.visible.store(visible, Ordering::Release);
        if visible {
            self.control.ui_closed.store(false, Ordering::Release);
        }
        Ok(())
    }

    fn refresh(&self) {
        match control_exchange(&self.control, ControlRequestKind::Status, CONTROL_TIMEOUT) {
            Ok(ControlResponseKind::Status(status)) => {
                let was_visible = self.control.visible.swap(status.visible, Ordering::AcqRel);
                if was_visible && !status.visible {
                    self.control.ui_closed.store(true, Ordering::Release);
                }
                self.control.ready.store(status.ready, Ordering::Release);
            }
            Ok(_) => {}
            Err(_) => {
                self.control.ready.store(false, Ordering::Release);
            }
        }
    }

    fn is_visible(&self) -> bool {
        self.control.ready.load(Ordering::Acquire)
            && self
                .control
                .editor_visibility
                .get()
                .unwrap_or_else(|| self.control.visible.load(Ordering::Acquire))
    }

    fn ui_was_closed(&self) -> bool {
        self.control.ui_closed.load(Ordering::Acquire)
    }
}

fn control_exchange(
    control: &SubprocessControlLink,
    kind: ControlRequestKind,
    timeout: Duration,
) -> Result<ControlResponseKind> {
    let mut stream = control
        .stream
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    stream.set_read_timeout(Some(timeout))?;
    let request_id = RequestId(control.request_id.fetch_add(1, Ordering::Relaxed) + 1);
    let request = ControlRequest {
        request_id,
        chain_id: control.chain_id,
        generation: control.generation,
        kind,
    };
    request.validate()?;
    write_frame(&mut *stream, &ParentToWorker::Control(request))?;
    let response: WorkerToParent = read_frame(&mut *stream)?;
    match response {
        WorkerToParent::Control(response)
            if response.request_id == request_id
                && response.chain_id == control.chain_id
                && response.generation == control.generation =>
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

pub struct SubprocessCarlaProcessor {
    info: CarlaProcessorInfo,
    chain_id: ChainId,
    generation: ProcessGeneration,
    control_link: Arc<SubprocessControlLink>,
    block_sequence: u64,
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
    checkpoint: String,
    process_timeout: Duration,
    deadline_misses: u64,
    midi_input_overflows: u64,
    midi_output_overflows: u64,
    stale_completions: u64,
    recoveries: u64,
    degraded: bool,
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
            .field(
                "visible",
                &self.control_link.visible.load(Ordering::Acquire),
            )
            .field("ready", &self.control_link.ready.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl SubprocessCarlaProcessor {
    pub fn spawn(
        executable: impl AsRef<Path>,
        chain_type: CarlaChainType,
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
        chain_type: CarlaChainType,
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
        chain_type: CarlaChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
        test_mode: Option<CarlaWorkerTestMode>,
    ) -> Result<Self> {
        if chain_id.0 == 0 || generation.0 == 0 {
            return Err(anyhow!("chain identity and generation must be nonzero"));
        }
        let protocol_type = chain_type;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let nonce = new_nonce()?;
        let shared_transport = SharedBlockTransport::create(generation, &nonce)?;
        let editor_visibility = shared_transport.editor_visibility()?;
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
        let processor = Self {
            info: CarlaProcessorInfo {
                chain_type,
                audio_inputs: channels,
                audio_outputs: channels,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            chain_id,
            generation,
            control_link: Arc::new(SubprocessControlLink {
                chain_id,
                generation,
                request_id: AtomicU64::new(1),
                stream: Mutex::new(stream),
                visible: AtomicBool::new(false),
                editor_visibility,
                ui_closed: AtomicBool::new(false),
                ready: AtomicBool::new(false),
            }),
            block_sequence: 0,
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
                .map(|_| (0, Vec::with_capacity(MAX_NATIVE_MIDI_PACKET_BYTES)))
                .collect()],
            midi_input_counts: vec![0],
            midi_outputs: vec![(0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| (0, Vec::with_capacity(MAX_NATIVE_MIDI_PACKET_BYTES)))
                .collect()],
            midi_output_counts: vec![0],
            shared_midi_outputs: (0..MAX_MIDI_EVENTS_PER_BLOCK)
                .map(|_| MidiEvent {
                    frame_offset: 0,
                    data: Vec::with_capacity(MAX_NATIVE_MIDI_PACKET_BYTES),
                })
                .collect(),
            shared_midi_output_count: 0,
            active: false,
            checkpoint: "{}".to_owned(),
            process_timeout: Duration::from_secs_f64(
                (nominal_buffer_size.max(1) as f64 / sample_rate.max(1) as f64).max(0.000_5),
            ),
            deadline_misses: 0,
            midi_input_overflows: 0,
            midi_output_overflows: 0,
            stale_completions: 0,
            recoveries: 0,
            degraded: false,
            serialized_reference_transport: false,
            exit_kind: WorkerExitKind::None,
            shutdown_complete: false,
        };
        processor.control(ControlRequestKind::Instantiate {
            chain_type: protocol_type,
            sample_rate,
            nominal_buffer_size,
        })?;
        processor.control_link.ready.store(true, Ordering::Release);
        Ok(processor)
    }

    pub fn current_executable() -> Result<PathBuf> {
        std::env::current_exe().context("could not locate current executable for Carla worker")
    }

    fn control(&self, kind: ControlRequestKind) -> Result<ControlResponseKind> {
        control_exchange(&self.control_link, kind, CONTROL_TIMEOUT)
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
        let mut stream = self
            .control_link
            .stream
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        write_frame(&mut *stream, &ParentToWorker::Process(block))?;
        let response: WorkerToParent = read_frame(&mut *stream)?;
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
            if event.data.len() > MAX_NATIVE_MIDI_PACKET_BYTES {
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
        self.control_link.ready.store(false, Ordering::Release);
        self.shutdown_complete = true;
        self.exit_kind
    }

    pub fn terminate_worker_for_test(&mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        self.control_link.ready.store(false, Ordering::Release);
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

    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        Some(Arc::new(SubprocessExternalUi {
            control: Arc::clone(&self.control_link),
        }))
    }

    fn is_ready(&mut self) -> bool {
        let ready = self.control_link.ready.load(Ordering::Acquire)
            && self.child.try_wait().ok().flatten().is_none();
        self.control_link.ready.store(ready, Ordering::Release);
        ready
    }

    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        if self.control_link.ready.load(Ordering::Acquire) {
            if self.degraded {
                CarlaProcessorLifecycle::Degraded
            } else {
                CarlaProcessorLifecycle::Running
            }
        } else {
            CarlaProcessorLifecycle::Crashed
        }
    }

    fn generation(&self) -> u64 {
        self.generation.0
    }

    fn exit_kind(&self) -> WorkerExitKind {
        if self.control_link.ui_closed.load(Ordering::Acquire) {
            WorkerExitKind::UiClosed
        } else {
            self.exit_kind
        }
    }

    fn deadline_misses(&self) -> u64 {
        self.deadline_misses
    }

    fn stale_completions(&self) -> u64 {
        self.stale_completions
    }

    fn recoveries(&self) -> u64 {
        self.recoveries
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
            self.control_link.ready.store(false, Ordering::Release);
        }
    }

    fn is_active(&self) -> bool {
        self.active && self.control_link.ready.load(Ordering::Acquire)
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        let ui = SubprocessExternalUi {
            control: Arc::clone(&self.control_link),
        };
        ui.set_visible(visible)
    }

    fn is_visible(&mut self) -> bool {
        match self.status() {
            Ok(status) => {
                let was_visible = self.control_link.visible.load(Ordering::Acquire);
                if was_visible && !status.visible {
                    self.exit_kind = WorkerExitKind::UiClosed;
                }
                self.control_link
                    .visible
                    .store(status.visible, Ordering::Release);
                self.control_link
                    .ready
                    .store(status.ready, Ordering::Release);
            }
            Err(_) => {
                self.control_link.visible.store(false, Ordering::Release);
                self.control_link.ready.store(false, Ordering::Release);
            }
        }
        self.control_link.visible.load(Ordering::Acquire)
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
            if count == destination.len() || data.len() > MAX_NATIVE_MIDI_PACKET_BYTES {
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

    fn process_at(&mut self, frames: usize, time: Option<BlockTime>) -> Result<()> {
        self.shared_transport.set_block_time(time);
        let result = self.process(frames);
        self.shared_transport.set_block_time(None);
        result
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
                    self.control_link.ready.store(false, Ordering::Release);
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
                self.degraded = true;
                self.clear_outputs(frames);
                return Ok(());
            }
            Err(error) => {
                self.control_link.ready.store(false, Ordering::Release);
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
                if std::mem::replace(&mut self.degraded, false) {
                    self.recoveries = self.recoveries.saturating_add(1);
                }
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
                self.degraded = true;
                self.clear_outputs(frames);
                Ok(())
            }
            Err(crate::carla_shared_memory::SharedBlockError::MidiOverflow) => {
                self.midi_output_overflows = self.midi_output_overflows.saturating_add(1);
                self.degraded = true;
                self.clear_outputs(frames);
                Ok(())
            }
            Err(crate::carla_shared_memory::SharedBlockError::StaleCompletion) => {
                self.stale_completions = self.stale_completions.saturating_add(1);
                self.degraded = true;
                self.clear_outputs(frames);
                Ok(())
            }
            Err(error) => {
                self.control_link.ready.store(false, Ordering::Release);
                self.clear_outputs(frames);
                Err(error.into())
            }
        }
    }
}

#[derive(Debug, Default)]
struct SupervisedExternalUi {
    current: Mutex<Option<Arc<dyn CarlaExternalUi>>>,
    desired_visible: AtomicBool,
}

impl CarlaExternalUi for SupervisedExternalUi {
    fn set_visible(&self, visible: bool) -> Result<()> {
        self.desired_visible.store(visible, Ordering::Release);
        let current = self
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or_else(|| anyhow!("Carla worker UI is unavailable"))?;
        current.set_visible(visible)
    }

    fn refresh(&self) {
        if let Some(current) = self
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        {
            current.refresh();
        }
    }

    fn is_visible(&self) -> bool {
        self.current
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_some_and(|current| current.is_visible())
    }

    fn ui_was_closed(&self) -> bool {
        self.current
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_some_and(|current| current.ui_was_closed())
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
    previous_deadline_misses: u64,
    previous_stale_completions: u64,
    previous_recoveries: u64,
    checkpoint: String,
    has_checkpoint: bool,
    desired_active: bool,
    external_ui: Arc<SupervisedExternalUi>,
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
            .field(
                "desired_visible",
                &self.external_ui.desired_visible.load(Ordering::Acquire),
            )
            .field("crash_summary", &self.crash_summary)
            .finish_non_exhaustive()
    }
}

impl SupervisedCarlaProcessor {
    pub fn launch(
        executable: impl Into<PathBuf>,
        chain_type: CarlaChainType,
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
        chain_type: CarlaChainType,
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
        chain_type: CarlaChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        test_mode: Option<CarlaWorkerTestMode>,
    ) -> Result<Self> {
        let channels = usize::from(chain_type.audio_channels());
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
            previous_deadline_misses: 0,
            previous_stale_completions: 0,
            previous_recoveries: 0,
            checkpoint: String::new(),
            has_checkpoint: false,
            desired_active: false,
            external_ui: Arc::new(SupervisedExternalUi::default()),
            lifecycle: CarlaProcessorLifecycle::Stopped,
            exit_kind: WorkerExitKind::None,
            crash_summary: None,
        };
        supervisor.start_generation(false);
        Ok(supervisor)
    }

    fn retain_current_logs_and_health(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        self.previous_deadline_misses = self
            .previous_deadline_misses
            .saturating_add(current.deadline_misses());
        self.previous_stale_completions = self
            .previous_stale_completions
            .saturating_add(current.stale_completions());
        self.previous_recoveries = self
            .previous_recoveries
            .saturating_add(current.recoveries());
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
            self.retain_current_logs_and_health();
            *self
                .external_ui
                .current
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
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
            let restored = if self.test_mode.is_some() {
                current.restore_state(&self.checkpoint)
            } else {
                crate::state::restore_candidate(&mut current, &self.checkpoint)
            };
            if let Err(error) = restored {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.exit_kind = WorkerExitKind::StartupFailure;
                self.crash_summary = Some(format!("state restore failed: {error}"));
                self.current = Some(current);
                self.retain_current_logs_and_health();
                drop(self.current.take());
                return;
            }
        }
        current.set_active(self.desired_active);
        if !current.is_ready() {
            self.lifecycle = CarlaProcessorLifecycle::Unavailable;
            self.exit_kind = WorkerExitKind::StartupFailure;
            self.crash_summary = Some("Carla worker rejected the initial active state".to_owned());
            self.current = Some(current);
            return;
        }
        if show_after_restore {
            if let Err(error) = current.set_visible(true) {
                self.lifecycle = CarlaProcessorLifecycle::Unavailable;
                self.exit_kind = WorkerExitKind::StartupFailure;
                self.crash_summary = Some(format!("external UI start failed: {error}"));
                self.current = Some(current);
                return;
            }
            self.external_ui
                .desired_visible
                .store(true, Ordering::Release);
        }
        *self
            .external_ui
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = current.external_ui();
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
        self.restart(false)
    }

    pub fn recover(&mut self, show_editor: bool) -> Result<()> {
        if !matches!(
            self.lifecycle,
            CarlaProcessorLifecycle::Crashed | CarlaProcessorLifecycle::Unavailable
        ) {
            return Err(anyhow!("Carla worker is not awaiting recovery"));
        }
        self.restart(show_editor)
    }

    fn restart(&mut self, show_editor: bool) -> Result<()> {
        self.start_generation(show_editor);
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            Ok(())
        } else {
            Err(anyhow!(
                "Carla worker recovery failed: {}",
                self.crash_summary.as_deref().unwrap_or("unknown error")
            ))
        }
    }
}

impl CarlaProcessor for SupervisedCarlaProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        Some(Arc::clone(&self.external_ui) as Arc<dyn CarlaExternalUi>)
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
        self.lifecycle.is_operational()
    }

    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        if self.lifecycle == CarlaProcessorLifecycle::Running {
            self.current
                .as_ref()
                .map(CarlaProcessor::lifecycle)
                .unwrap_or(self.lifecycle)
        } else {
            self.lifecycle
        }
    }

    fn generation(&self) -> u64 {
        self.generation.0
    }

    fn exit_kind(&self) -> WorkerExitKind {
        if self.exit_kind == WorkerExitKind::None && self.external_ui.ui_was_closed() {
            WorkerExitKind::UiClosed
        } else {
            self.exit_kind
        }
    }

    fn deadline_misses(&self) -> u64 {
        self.previous_deadline_misses.saturating_add(
            self.current
                .as_ref()
                .map(CarlaProcessor::deadline_misses)
                .unwrap_or(0),
        )
    }

    fn stale_completions(&self) -> u64 {
        self.previous_stale_completions.saturating_add(
            self.current
                .as_ref()
                .map(CarlaProcessor::stale_completions)
                .unwrap_or(0),
        )
    }

    fn recoveries(&self) -> u64 {
        self.previous_recoveries.saturating_add(
            self.current
                .as_ref()
                .map(CarlaProcessor::recoveries)
                .unwrap_or(0),
        )
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
            self.recover(true)
        } else {
            let visible = self.is_visible();
            self.set_visible(!visible)
        }
    }

    fn recover(&mut self, show_editor: bool) -> Result<()> {
        SupervisedCarlaProcessor::recover(self, show_editor)
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
        self.desired_active && self.lifecycle().is_operational()
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.external_ui
            .desired_visible
            .store(visible, Ordering::Release);
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
        if !self.lifecycle().is_operational() {
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

    fn capture_state(&mut self) -> Result<String> {
        if !self.is_ready() {
            anyhow::bail!("Carla worker is unavailable for state capture");
        }
        let current = self
            .current
            .as_mut()
            .context("Carla worker is unavailable")?;
        let state = current.capture_state()?;
        self.checkpoint.clone_from(&state);
        self.has_checkpoint = true;
        Ok(state)
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
        self.process_at(frames, None)
    }

    fn process_at(&mut self, frames: usize, time: Option<BlockTime>) -> Result<()> {
        if self.lifecycle != CarlaProcessorLifecycle::Running {
            return Err(anyhow!("Carla worker is not running"));
        }
        let result = self
            .current
            .as_mut()
            .ok_or_else(|| anyhow!("Carla worker is unavailable"))?
            .process_at(frames, time);
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

    #[shoop_wasm_test_support::shoop_test]
    fn nonce_round_trip_and_validation() {
        let nonce = new_nonce().unwrap();
        assert_eq!(
            crate::child::parse_nonce(&nonce_hex(&nonce)).unwrap(),
            nonce
        );
        assert!(crate::child::parse_nonce("bad").is_err());
        assert!(crate::child::parse_nonce(&"é".repeat(32)).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
