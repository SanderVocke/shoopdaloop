use crate::{convert, Client, Port, Transport};
use anyhow::{bail, Context, Result};
use shoop_rt_host_native::NativeConfig;
use shoop_rt_protocol::{codec, host::*, MAX_RECORD_BYTES};
use shoop_rt_transport::{
    carrier::{CarrierError, RecordPort},
    client::ProtocolClient,
    records::RecordClient,
};
use shoop_rt_transport_shared_memory::{Descriptor, Port as SharedPort, Side};
use shoop_rt_transport_socket::{setup, Config as SocketConfig};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(10);
type Input = Arc<Mutex<ChildStdin>>;
struct Monitor {
    thread: Option<JoinHandle<()>>,
    #[cfg(feature = "testing")]
    frame: Arc<AtomicU64>,
    stop: Arc<AtomicU8>,
    #[cfg(feature = "testing")]
    observations: Arc<AtomicU64>,
    #[cfg(feature = "testing")]
    expected: Arc<AtomicU64>,
}
impl Monitor {
    fn start(child: &mut Child) -> Self {
        let output = child.stdout.take().expect("piped stdout");
        let frame = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicU8::new(0));
        let frames = frame.clone();
        let stopped = stop.clone();
        let expected = Arc::new(AtomicU64::new(0));
        let egress = expected.clone();
        let observations = Arc::new(AtomicU64::new(0));
        let observed = observations.clone();
        let thread = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Some(value) = line.strip_prefix("@FRAME ") {
                    let mut values = value.split_whitespace();
                    if let Some(frame) = values.next().and_then(|s| s.parse().ok()) {
                        if let Some(count) = values.next().and_then(|s| s.parse().ok()) {
                            egress.store(count, Ordering::Release);
                        }
                        frames.store(frame, Ordering::Release);
                        observed.fetch_add(1, Ordering::Release);
                    }
                } else if let Some(value) = line.strip_prefix("@STOP ").and_then(|s| s.parse().ok())
                {
                    stopped.store(value, Ordering::Release);
                } else {
                    tracing::debug!(message=%line,"engine process output");
                }
            }
        });
        Self {
            thread: Some(thread),
            #[cfg(feature = "testing")]
            frame,
            stop,
            #[cfg(feature = "testing")]
            expected,
            #[cfg(feature = "testing")]
            observations,
        }
    }
    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub struct Pending {
    child: Option<Child>,
    input: Option<Input>,
    control: Option<shoop_rt_transport_socket::Port>,
    transport: Transport,
    monitor: Option<Monitor>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(monitor) = self.monitor.as_mut() {
            monitor.join();
        }
    }
}
pub struct Started {
    pub client: Client,
    pub supervisor: Supervisor,
    pub warnings: Vec<String>,
}
pub struct Supervisor {
    child: Option<Child>,
    input: Input,
    monitor: Monitor,
    #[cfg(feature = "testing")]
    sent: Arc<AtomicU64>,
    #[cfg(feature = "testing")]
    received: Arc<AtomicU64>,
}
#[derive(Clone, Copy, Debug)]
pub struct StopReport {
    pub success: bool,
    pub midi_reset: bool,
    pub cleaned: bool,
}
impl Supervisor {
    #[cfg(feature = "testing")]
    pub fn disconnect_for_test(&self) -> std::io::Result<()> {
        self.input.lock().unwrap().write_all(b"D")
    }

    #[cfg(feature = "testing")]
    pub fn kill_for_test(&mut self) -> std::io::Result<()> {
        self.child.as_mut().unwrap().kill()
    }
    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
    pub fn stop(mut self) -> StopReport {
        self.stop_inner()
    }
    fn stop_inner(&mut self) -> StopReport {
        let Some(mut child) = self.child.take() else {
            return StopReport {
                success: true,
                midi_reset: true,
                cleaned: true,
            };
        };
        if let Ok(mut input) = self.input.lock() {
            let _ = input.write_all(b"Q");
            let _ = input.flush();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let success = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
            }
        };
        self.monitor.join();
        let stop = self.monitor.stop.load(Ordering::Acquire);
        tracing::info!(
            success,
            midi_reset = stop & 1 != 0,
            cleaned = stop & 2 != 0,
            "engine_process.stopped"
        );
        StopReport {
            success,
            midi_reset: stop & 1 != 0,
            cleaned: stop & 2 != 0,
        }
    }
    #[cfg(feature = "testing")]
    pub fn testing_clock(&self, sample_rate: u32, block: u64) -> TestingClock {
        TestingClock {
            input: self.input.clone(),
            frame: self.monitor.frame.clone(),
            sample_rate,
            block,
            remainder: 0,
            sent: self.sent.clone(),
            received: self.received.clone(),
            expected: self.monitor.expected.clone(),
            observations: self.monitor.observations.clone(),
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}
#[cfg(feature = "testing")]
pub struct TestingClock {
    input: Input,
    frame: Arc<AtomicU64>,
    sample_rate: u32,
    block: u64,
    remainder: u128,
    sent: Arc<AtomicU64>,
    received: Arc<AtomicU64>,
    expected: Arc<AtomicU64>,
    observations: Arc<AtomicU64>,
}
#[cfg(feature = "testing")]
impl TestingClock {
    pub fn observations_ready(&self) -> bool {
        self.received.load(Ordering::Acquire) >= self.expected.load(Ordering::Acquire)
    }
    pub fn frame(&self) -> u64 {
        self.frame.load(Ordering::Acquire)
    }
    pub fn step(&mut self, frames: u64) -> bool {
        let observed = self.observations.load(Ordering::Acquire);
        let target = self.frame() + frames;
        let Ok(mut input) = self.input.lock() else {
            return false;
        };
        if input
            .write_all(b"A")
            .and_then(|_| input.write_all(&frames.to_le_bytes()))
            .and_then(|_| {
                input.write_all(
                    &if frames == 0 {
                        0
                    } else {
                        self.sent.load(Ordering::Acquire)
                    }
                    .to_le_bytes(),
                )
            })
            .and_then(|_| input.flush())
            .is_err()
        {
            return false;
        }
        drop(input);
        let deadline = Instant::now() + TIMEOUT;
        while self.frame() < target || self.observations.load(Ordering::Acquire) == observed {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::yield_now();
        }
        true
    }
    pub fn advance(&mut self, elapsed: Duration) {
        let total = self.remainder + elapsed.as_nanos() * u128::from(self.sample_rate);
        self.remainder = total % 1_000_000_000;
        assert!(self.step((total / 1_000_000_000) as u64));
    }
    pub fn wait(&mut self) -> bool {
        self.step(self.block)
    }
}
impl Pending {
    pub fn spawn(executable: &Path, transport: Transport) -> Result<Self> {
        if !executable.is_absolute() {
            bail!("engine executable path must be absolute")
        }
        Self::spawn_command(Command::new(executable), transport)
    }
    pub fn spawn_command(mut command: Command, transport: Transport) -> Result<Self> {
        let listener = setup::Listener::bind()?;
        command
            .env("SHOOP_RT_PROCESS_ADDRESS", listener.address()?.to_string())
            .env("SHOOP_RT_PROCESS_TRANSPORT", transport.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .context("could not launch the companion engine executable")?;
        tracing::info!(
            pid = child.id(),
            transport = transport.as_str(),
            "engine_process.spawned"
        );
        let input = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
        let monitor = Monitor::start(&mut child);
        let mut pending = Self {
            child: Some(child),
            input: Some(input),
            control: None,
            transport,
            monitor: Some(monitor),
        };
        pending.control = Some(listener.accept(SocketConfig::default(), TIMEOUT)?);
        match pending.exchange(HostRequest::Hello {
            version: HOST_VERSION,
        })? {
            HostReply::Hello { version } if version == HOST_VERSION => {}
            _ => bail!("incompatible engine host protocol"),
        }
        Ok(pending)
    }
    fn exchange(&mut self, request: HostRequest) -> Result<HostReply> {
        let mut buf = [0; MAX_RECORD_BYTES];
        let len = codec::encode_host_request(&request, &mut buf)
            .map_err(|e| anyhow::anyhow!("host request codec: {e:?}"))?;
        let control = self.control.as_mut().unwrap();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match control.post(&buf[..len]) {
                Ok(()) => break,
                Err(CarrierError::Full) => {}
                Err(error) => bail!("engine setup transport failed: {error:?}"),
            };
            if Instant::now() > deadline {
                bail!("engine setup send timed out")
            }
            std::thread::yield_now();
        }
        loop {
            if let Some(len) = control.take(&mut buf) {
                let reply = codec::decode_host_reply(&buf[..len])
                    .map_err(|e| anyhow::anyhow!("host reply codec: {e:?}"))?;
                if let HostReply::Error(text) = reply {
                    bail!("engine host: {}", text.as_str())
                }
                return Ok(reply);
            }
            if control.failure().is_some() {
                bail!("engine disconnected during startup")
            }
            if Instant::now() > deadline {
                bail!("engine setup response timed out")
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn ok(&mut self, request: HostRequest) -> Result<()> {
        if self.exchange(request)? != HostReply::Ok {
            bail!("unexpected host setup response")
        }
        Ok(())
    }
    #[cfg(feature = "cpal")]
    pub fn discover(&mut self, host: Option<&str>) -> Result<shoop_rt_host_native::Discovery> {
        let host = Text::new(host.unwrap_or(""))
            .ok_or_else(|| anyhow::anyhow!("host name exceeds capacity"))?;
        let mut result = shoop_rt_host_native::Discovery::default();
        for kind in [
            DiscoverKind::Hosts,
            DiscoverKind::AudioInputs,
            DiscoverKind::AudioOutputs,
            DiscoverKind::MidiInputs,
            DiscoverKind::MidiOutputs,
        ] {
            for index in 0..MAX_SETUP_ITEMS {
                match self.exchange(HostRequest::Discover {
                    kind,
                    index: index as u16,
                    host,
                })? {
                    HostReply::End => break,
                    HostReply::Device {
                        kind: returned,
                        index: returned_index,
                        name,
                        default,
                        channels,
                        sample_rate,
                        min_buffer,
                        max_buffer,
                    } if returned == kind && returned_index as usize == index => {
                        let name = name.as_str().to_string();
                        match kind {
                            DiscoverKind::Hosts => result.hosts.push(name),
                            DiscoverKind::MidiInputs => result.midi_inputs.push(name),
                            DiscoverKind::MidiOutputs => result.midi_outputs.push(name),
                            DiscoverKind::AudioInputs | DiscoverKind::AudioOutputs => {
                                let device = shoop_rt_host_native::AudioDevice {
                                    name,
                                    default,
                                    channels,
                                    sample_rate,
                                    buffer_frames: (max_buffer != 0)
                                        .then_some((min_buffer, max_buffer)),
                                };
                                if kind == DiscoverKind::AudioInputs {
                                    result.inputs.push(device)
                                } else {
                                    result.outputs.push(device)
                                }
                            }
                        }
                    }
                    _ => bail!("unexpected discovery page"),
                }
                if index + 1 == MAX_SETUP_ITEMS {
                    bail!("device discovery exceeds setup capacity")
                }
            }
        }
        result.host = host.as_str().to_string();
        Ok(result)
    }
    pub fn start(mut self, config: &NativeConfig) -> Result<Started> {
        self.ok(HostRequest::ConfigureEngine(convert::engine(
            &config.engine,
        )?))?;
        if config.engine.segments.len() > MAX_SETUP_ITEMS {
            bail!("too many pool segments")
        }
        for segment in &config.engine.segments {
            self.ok(HostRequest::AddSegment {
                strips: segment.strips,
                loops: segment.loops,
            })?;
        }
        self.ok(HostRequest::ConfigureDriver(convert::driver(config)?))?;
        #[cfg(feature = "cpal")]
        if let shoop_rt_host_native::DriverConfig::Cpal(driver) = &config.driver {
            if driver.midi_inputs.len() > MAX_SETUP_ITEMS
                || driver.midi_outputs.len() > MAX_SETUP_ITEMS
            {
                bail!("too many MIDI ports")
            }
            for (names, input) in [(&driver.midi_inputs, true), (&driver.midi_outputs, false)] {
                for name in names {
                    let name = Text::new(name)
                        .ok_or_else(|| anyhow::anyhow!("MIDI device name exceeds capacity"))?;
                    self.ok(if input {
                        HostRequest::AddMidiInput(name)
                    } else {
                        HostRequest::AddMidiOutput(name)
                    })?;
                }
            }
        }
        let shared = if self.transport == Transport::SharedMemory {
            let descriptor = Descriptor::new([512, 256, 32])?;
            let port = SharedPort::create(descriptor, Side::Client)?;
            self.input
                .as_ref()
                .unwrap()
                .lock()
                .map_err(|_| anyhow::anyhow!("engine launch pipe failed"))?
                .write_all(&descriptor.encode())?;
            Some(port)
        } else {
            None
        };
        let mut warnings = Vec::new();
        let mut reply = self.exchange(HostRequest::Start)?;
        loop {
            match reply {
                HostReply::Warning(text) => {
                    warnings.push(text.as_str().to_string());
                    reply = self.receive_reply()?;
                }
                HostReply::Ready(_) => break,
                _ => bail!("unexpected engine readiness response"),
            }
        }
        let control = self.control.take().unwrap();
        let port = if let Some(shared) = shared {
            Port::shared(shared.with_liveness(control.stream().try_clone()?)?)
        } else {
            Port::socket(control)
        };
        tracing::info!(
            transport = self.transport.as_str(),
            warnings = warnings.len(),
            "engine_process.ready"
        );
        let supervisor = Supervisor {
            child: self.child.take(),
            input: self.input.take().unwrap(),
            monitor: self.monitor.take().unwrap(),
            #[cfg(feature = "testing")]
            sent: port.sent.clone(),
            #[cfg(feature = "testing")]
            received: port.received.clone(),
        };
        Ok(Started {
            client: ProtocolClient::new(RecordClient::new(port)),
            supervisor,
            warnings,
        })
    }
    fn receive_reply(&mut self) -> Result<HostReply> {
        let deadline = Instant::now() + TIMEOUT;
        let mut buf = [0; MAX_RECORD_BYTES];
        let control = self.control.as_mut().unwrap();
        loop {
            if let Some(len) = control.take(&mut buf) {
                return codec::decode_host_reply(&buf[..len])
                    .map_err(|e| anyhow::anyhow!("host reply: {e:?}"));
            }
            if control.failure().is_some() {
                bail!("engine disconnected during startup")
            }
            if Instant::now() > deadline {
                bail!("engine readiness timed out")
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
