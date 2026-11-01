use crate::{
    config::HostConfig,
    connection::Port,
    engine, private,
    registry::{identity, AppTransport, HostEntry, Registry},
};
use anyhow::{anyhow, ensure, Context, Result};
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_protocol::{codec, stream::StreamEncoder, *};
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_host_native::{DriverConfig, NativeConfig, NativeHost};
use shoop_transport::{CarrierError, RecordPort, MAX_RECORD_BYTES};
use shoop_transport_socket::setup::Listener;
use std::{
    collections::VecDeque,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct StartOptions {
    pub registry: PathBuf,
    pub endpoint: SocketAddr,
    pub credential: Option<PathBuf>,
    pub host: HostId,
    pub session: SessionId,
    pub name: String,
}
impl StartOptions {
    pub fn new(registry: PathBuf) -> Result<Self> {
        Ok(Self {
            registry,
            endpoint: "127.0.0.1:0".parse()?,
            credential: None,
            host: HostId(identity()?),
            session: SessionId(identity()?),
            name: "ShoopDaLoop".into(),
        })
    }
}

struct Acceptor {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    ports: Receiver<shoop_transport_socket::Port>,
}
impl Acceptor {
    fn new(listener: Listener) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let (sender, ports) = mpsc::sync_channel(4);
        let join = thread::Builder::new()
            .name("shoop-auth".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    match listener.accept(Default::default(), Duration::from_millis(100)) {
                        Ok(port) => {
                            let _ = sender.try_send(port);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            join: Some(join),
            ports,
        })
    }
}
impl Drop for Acceptor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Registration {
    registry: Registry,
    entry: HostEntry,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Err(error) = self.registry.remove(&self.entry) {
            tracing::warn!(%error, "host.registry.cleanup_failed");
        }
    }
}

pub fn run(config: HostConfig, options: StartOptions) -> Result<()> {
    let _span =
        tracing::info_span!("host.session", host = %options.host.0, session = %options.session.0)
            .entered();
    config.validate()?;
    ensure!(
        options.host.0 != 0 && options.session.0 != 0 && options.name.len() <= 128,
        "invalid host identity or name"
    );
    let registry = Registry::open(&options.registry)?;
    let (mut native, mut warnings) = engine::native_config(
        &config.driver,
        &config.pools,
        &engine::inventory(&config.driver),
    );
    #[cfg(feature = "testing")]
    if config.test_signal {
        match &mut native.driver {
            DriverConfig::Dummy(dummy) => dummy.input = Some(|_, _| 0.25),
            #[cfg(feature = "cpal")]
            DriverConfig::Cpal(_) => {}
        }
    }
    #[cfg(not(feature = "testing"))]
    let _ = &mut native;
    let started = match shoop_rt_host_native::start(&native) {
        Ok(started) => started,
        Err(error) if !matches!(native.driver, DriverConfig::Dummy(_)) => {
            warnings.push(format!(
                "Could not start the configured audio driver: {error}; using dummy"
            ));
            let dummy = shoop_app_api::DummyAudioDriverConfig::default();
            shoop_rt_host_native::start(&NativeConfig {
                driver: DriverConfig::Dummy(engine::dummy_config(&dummy)),
                engine: engine::engine_description(&config.pools, dummy.buffer_size),
            })?
        }
        Err(error) => return Err(error.into()),
    };
    warnings.extend(started.host.warnings().iter().cloned());
    let mut host = Some(started.host);
    let backend = RtBackend::connect(started.client, Sleep::default())?;
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            host: options.host,
            session: options.session,
            master_auto_connect: config.master_auto_connect,
            loop_smoothing_ms: config.loop_smoothing_ms,
        },
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !service.is_ready() {
        service.tick(Duration::from_millis(2));
        ensure!(
            service.failure().is_none(),
            "session initialization failed: {:?}",
            service.failure()
        );
        ensure!(
            Instant::now() < deadline,
            "session initialization timed out"
        );
        thread::sleep(Duration::from_millis(2));
    }
    let token = match &options.credential {
        Some(path) => private::read(path, 32)?
            .try_into()
            .map_err(|_| anyhow!("invalid credential length"))?,
        None => {
            let mut token = [0; 32];
            getrandom::fill(&mut token)
                .map_err(|_| anyhow!("session credential entropy unavailable"))?;
            token
        }
    };
    let data = Listener::bind_with(options.endpoint, token)?;
    let management = Listener::bind_with("127.0.0.1:0".parse()?, token)?;
    let descriptor = if config.transport == AppTransport::SharedMemory {
        Some(shoop_transport_shared_memory::Descriptor::new([
            512, 32, 32,
        ])?)
    } else {
        None
    };
    let mut shared = descriptor
        .map(|descriptor| {
            shoop_transport_shared_memory::Port::create(
                descriptor,
                shoop_transport_shared_memory::Side::Server,
            )
        })
        .transpose()?;
    let credential = options
        .credential
        .map(Ok)
        .unwrap_or_else(|| registry.create_credential(options.host, &token))?;
    let registration = Registration {
        entry: HostEntry {
            version: VERSION,
            host: options.host,
            session: options.session,
            name: options.name,
            process: std::process::id(),
            endpoint: data.address()?,
            management: management.address()?,
            credential,
            transport: config.transport,
            shared_memory: descriptor.map(|descriptor| descriptor.encode().to_vec()),
            warnings,
        },
        registry,
    };
    registration.registry.publish(&registration.entry)?;
    let data = Acceptor::new(data)?;
    let management = Acceptor::new(management)?;
    let mut clients = Vec::new();
    let mut managers = Vec::new();
    let mut next_connection = 1u64;
    let mut last = Instant::now();
    let mut finished = None;
    let mut cleanup_error = None;
    loop {
        for _ in 0..4 {
            let Ok(mut port) = data.ports.try_recv() else {
                break;
            };
            if clients.len() >= 4 {
                continue;
            }
            let shared_port = if config.transport == AppTransport::SharedMemory {
                match shared.take() {
                    Some(shared) => Some(shared.with_liveness(port.stream().try_clone()?)?),
                    None => {
                        let _ = send(&mut port, &WireMessage::Rejected(Refusal::Unsupported));
                        continue;
                    }
                }
            } else {
                None
            };
            if send(
                &mut port,
                &WireMessage::CarrierReady {
                    host: registration.entry.host,
                    session: registration.entry.session,
                },
            )
            .is_err()
            {
                continue;
            }
            clients.push(Data::new(ConnectionId(next_connection), port, shared_port));
            next_connection = next_connection
                .checked_add(1)
                .context("connection generation exhausted")?;
        }
        for _ in 0..4 {
            match management.ports.try_recv() {
                Ok(port) if managers.len() < 4 => managers.push(Manager {
                    port,
                    started: Instant::now(),
                    sent: false,
                }),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        for client in &mut clients {
            client.receive(&mut service);
        }
        let now = Instant::now();
        service.tick(now.duration_since(last));
        last = now;
        if service.shutdown_ready() {
            service
                .begin_host_cleanup()
                .map_err(|reason| anyhow!("host cleanup refused: {reason:?}"))?;
            let result = match host.take() {
                Some(host) => stop(host),
                None => Err(cleanup_error
                    .clone()
                    .unwrap_or_else(|| "host cleanup could not be completed".into())),
            };
            if let Err(error) = &result {
                cleanup_error = Some(error.clone());
            }
            if result.is_ok() {
                finished = Some(Instant::now() + Duration::from_secs(2));
            }
            service.complete_shutdown(result);
        }
        for manager in &mut managers {
            manager.pump(&mut service);
        }
        managers.retain_mut(|manager| {
            !(manager.sent && manager.port.outgoing_empty())
                && manager.started.elapsed() < Duration::from_secs(2)
                && manager.port.failure().is_none()
        });
        for client in &mut clients {
            client.write(&mut service);
        }
        clients.retain_mut(|client| {
            let alive = client.alive(&service);
            if !alive {
                service.detach(client.id);
            }
            alive
        });
        if finished.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    drop(service);
    drop(host);
    drop(registration);
    Ok(())
}

fn stop(host: NativeHost) -> Result<(), String> {
    let report = host.stop();
    let mut errors = Vec::new();
    if !report.midi_reset {
        errors.push("MIDI reset was not confirmed".to_owned());
    }
    if report.phase == shoop_rt_host_native::RenderPhase::Faulted {
        errors.push("rendering faulted".to_owned());
    }
    if let Err(error) = report.stopped {
        errors.push(format!("engine cleanup failed: {error}"));
    }
    if report.stats.midi_out_failed > 0 {
        errors.push("MIDI output cleanup failed".to_owned());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
fn send(port: &mut impl RecordPort, message: &WireMessage) -> Result<()> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let length =
        codec::encode(message, &mut bytes).map_err(|_| anyhow!("invalid host response"))?;
    port.post(&bytes[..length])
        .map_err(|error| anyhow!("host response delivery failed: {error:?}"))
}
struct Manager {
    port: shoop_transport_socket::Port,
    started: Instant,
    sent: bool,
}
impl Manager {
    fn pump(&mut self, service: &mut Service) {
        let mut bytes = [0; MAX_RECORD_BYTES];
        if !self.sent {
            if let Some(length) = self.port.take(&mut bytes) {
                let reply = match codec::decode(&bytes[..length]) {
                    Ok(WireMessage::ManagementRequest(request)) => service.management(request),
                    _ => ManagementReply::Refused(Refusal::Invalid),
                };
                self.sent = send(&mut self.port, &WireMessage::ManagementReply(reply)).is_ok();
            }
        } else {
            let _ = self.port.flush();
        }
    }
}
struct Data {
    id: ConnectionId,
    port: Port,
    shared: Option<shoop_transport_shared_memory::Port>,
    started: Instant,
    attached: bool,
    closing: bool,
    closing_since: Option<Instant>,
    semantic: Option<StreamEncoder>,
    snapshot: bool,
    media: Option<StreamEncoder>,
    pending: Option<Vec<u8>>,
    transfer: u64,
    replies: VecDeque<ServiceMessage>,
}
impl Data {
    fn new(
        id: ConnectionId,
        port: shoop_transport_socket::Port,
        shared: Option<shoop_transport_shared_memory::Port>,
    ) -> Self {
        Self {
            id,
            port: Port::Socket(port),
            shared,
            started: Instant::now(),
            attached: false,
            closing: false,
            closing_since: None,
            semantic: None,
            snapshot: false,
            media: None,
            pending: None,
            transfer: 1,
            replies: VecDeque::new(),
        }
    }
    fn retire(&mut self) {
        self.closing = true;
        self.closing_since.get_or_insert_with(Instant::now);
        self.media = None;
        self.semantic = None;
        self.pending = None;
    }
    fn receive(&mut self, service: &mut Service) {
        if self.shared.is_some() {
            if let Port::Socket(port) = &mut self.port {
                let _ = port.flush();
                if !port.outgoing_empty() {
                    return;
                }
            }
            self.port = Port::Shared(self.shared.take().unwrap());
        }
        if self.closing {
            return;
        }
        let mut bytes = [0; MAX_RECORD_BYTES];
        for _ in 0..MAX_COMMANDS_PER_TURN {
            let Some(length) = self.port.take(&mut bytes) else {
                break;
            };
            let result = match codec::decode(&bytes[..length]) {
                Ok(WireMessage::Client(message)) => {
                    if matches!(
                        message,
                        ClientMessage::CancelMedia { .. } | ClientMessage::ReadMedia { .. }
                    ) {
                        self.media = None;
                    }
                    let detached = matches!(message, ClientMessage::Detach { .. });
                    let result = service.handle(self.id, message);
                    if result.is_ok() && detached {
                        self.retire();
                    }
                    result
                }
                _ => Err(Refusal::Invalid),
            };
            if let Err(reason) = result {
                self.replies.push_back(ServiceMessage::Rejected(reason));
                self.retire();
                break;
            }
            self.attached |= service.has_connection(self.id);
        }
    }
    fn write(&mut self, service: &mut Service) {
        for index in 0..MAX_MESSAGES_PER_TURN {
            if self.pending.is_none() {
                if self.semantic.is_none() {
                    let update = self
                        .replies
                        .pop_front()
                        .or_else(|| service.take_update(self.id));
                    if let Some(update) = update {
                        let media = matches!(update, ServiceMessage::Media { .. });
                        self.snapshot = matches!(update, ServiceMessage::Snapshot { .. });
                        let stream = StreamEncoder::new(update, TransferId(self.transfer));
                        self.transfer = self.transfer.saturating_add(1);
                        match stream {
                            Ok(stream) if media => self.media = Some(stream),
                            Ok(stream) => self.semantic = Some(stream),
                            Err(_) => {
                                self.retire();
                                return;
                            }
                        }
                    }
                }
                let from_media = self.media.is_some()
                    && (self.semantic.is_none() || (self.snapshot && index % 4 == 3));
                let stream = if from_media {
                    &mut self.media
                } else {
                    &mut self.semantic
                };
                let Some(encoder) = stream.as_mut() else {
                    break;
                };
                let Some(message) = encoder.next_message() else {
                    *stream = None;
                    continue;
                };
                let mut bytes = [0; MAX_RECORD_BYTES];
                match codec::encode(&message, &mut bytes) {
                    Ok(length) => self.pending = Some(bytes[..length].to_vec()),
                    Err(_) => {
                        self.retire();
                        return;
                    }
                }
            }
            match self.port.post(self.pending.as_ref().unwrap()) {
                Ok(()) => {
                    self.pending = None;
                }
                Err(CarrierError::Full) => break,
                Err(_) => {
                    self.retire();
                    break;
                }
            }
        }
    }
    fn alive(&mut self, service: &Service) -> bool {
        if self
            .closing_since
            .is_some_and(|started| started.elapsed() > Duration::from_secs(2))
        {
            return false;
        }
        if self.port.failure().is_some() {
            return false;
        }
        if self.attached && !service.has_connection(self.id) && !self.closing {
            return false;
        }
        if !self.attached && self.started.elapsed() > Duration::from_secs(3) {
            return false;
        }
        let drained = match &mut self.port {
            Port::Socket(port) => {
                let _ = port.flush();
                port.outgoing_empty()
            }
            Port::Shared(_) => true,
        };
        !(self.closing
            && self.replies.is_empty()
            && self.semantic.is_none()
            && self.pending.is_none()
            && drained)
    }
}
