use crate::{
    control::{
        connect, prove_client, read_frame, write_frame, Action, Credential, Intent, Kind, Request,
        Response, Status, Welcome, CONTROL_TIMEOUT, CONTROL_VERSION, HEARTBEAT, STATE_LIMIT,
    },
    media::POLL,
    ClientMedia, Config, Counters, Lifecycle,
};
use rt_buffered::{AsyncBatch, AsyncCompletion, AsyncTransport, Backend, Batch, LocalTransport};
use rt_runtime::{graph::PortCounts, Error};
pub use shoop_processor_protocol::RemoteFxReconnect as Reconnect;
use shoop_processor_protocol::RemoteFxTarget;
use std::{
    io,
    net::{SocketAddr, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

const RETRY_INTERVAL: Duration = Duration::from_millis(500);
const COMMANDS: usize = 8;

struct Connection {
    tcp: TcpStream,
    next: u64,
}

impl Connection {
    fn open(
        address: SocketAddr,
        kind: Kind,
        config: Config,
        policy: Reconnect,
        credential: Option<&Credential>,
    ) -> io::Result<(Self, MediaSession, Option<Credential>, bool)> {
        let (tcp, reply, resumed) = if let Some(credential) = credential {
            match connect(
                address,
                Intent::Resume {
                    credential: credential.clone(),
                },
            )? {
                (_, Welcome::Pending {}) => return Err(io::ErrorKind::WouldBlock.into()),
                (_, Welcome::Missing {}) => {
                    let (tcp, reply) = connect(
                        address,
                        Intent::Open {
                            kind,
                            config,
                            retain: true,
                        },
                    )?;
                    (tcp, reply, false)
                }
                (tcp, reply) => (tcp, reply, true),
            }
        } else {
            let (tcp, reply) = connect(
                address,
                Intent::Open {
                    kind,
                    config,
                    retain: policy != Reconnect::Recreate,
                },
            )?;
            (tcp, reply, false)
        };
        let Welcome::Ready {
            version: CONTROL_VERSION,
            config: accepted,
            udp_port,
            stream,
            credential,
        } = reply
        else {
            return Err(io::Error::other("Incompatible FX stream reply"));
        };
        if accepted != config || udp_port == 0 {
            return Err(io::Error::other(
                "FX service accepted a different stream configuration",
            ));
        }
        let token = u128::from_str_radix(&stream, 16).map_err(io::Error::other)?;
        let mut udp = address;
        udp.set_port(udp_port);
        let socket = prove_client(udp, token)?;
        Ok((
            Self { tcp, next: 0 },
            MediaSession {
                socket,
                token,
                config,
            },
            credential,
            resumed,
        ))
    }

    fn call(&mut self, action: Action) -> io::Result<Response> {
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
        write_frame(&mut self.tcp, &Request { id, action }, STATE_LIMIT)?;
        let reply: Response = read_frame(&mut self.tcp, STATE_LIMIT, CONTROL_TIMEOUT)?;
        if reply.id != id {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(reply)
    }
}

struct MediaSession {
    socket: UdpSocket,
    token: u128,
    config: Config,
}

enum Operation {
    Remote(Action),
    Recover,
}

struct Command {
    operation: Operation,
    owner: (u64, u64),
    reply: SyncSender<Result<Option<String>, String>>,
}

pub struct Ticket(Receiver<Result<Option<String>, String>>);

impl Ticket {
    pub fn poll(&mut self) -> Option<Result<Option<String>, String>> {
        match self.0.try_recv() {
            Ok(reply) => Some(reply),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("FX control worker stopped".into())),
        }
    }

    pub fn wait(self) -> Result<Option<String>, String> {
        self.0
            .recv_timeout(CONTROL_TIMEOUT + RETRY_INTERVAL)
            .map_err(|error| error.to_string())?
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Observation {
    pub owner: u64,
    pub generation: u64,
    pub status: Status,
    pub media: Counters,
    pub connected: bool,
}

struct Shared {
    observation: Mutex<Observation>,
    closing: AtomicBool,
    desired_active: AtomicBool,
    dry_on_loss: AtomicBool,
    target: Mutex<Option<(u32, RemoteFxTarget)>>,
    claim: AtomicU64,
    claim_result: AtomicU64,
    reset_requested: AtomicU64,
    reset_done: AtomicU64,
    media_changed: AtomicBool,
    media_failed: AtomicBool,
    media_thread: OnceLock<thread::Thread>,
    media: Mutex<Option<MediaSession>>,
}

impl Shared {
    fn replace_media(&self, session: Option<MediaSession>) {
        let mut pending = self.media.lock().unwrap();
        *pending = session;
        self.media_changed.store(true, Ordering::Release);
        if let Some(worker) = self.media_thread.get() {
            worker.unpark();
        }
    }

    fn take_media_update(&self) -> Option<Option<MediaSession>> {
        if !self.media_changed.load(Ordering::Acquire) {
            return None;
        }
        let mut pending = self.media.lock().unwrap();
        // Consume the flag with its socket so a reconnect cannot leave a second,
        // empty update behind after its replacement socket has been taken.
        self.media_changed.store(false, Ordering::Relaxed);
        Some(pending.take())
    }
}

struct Lifetime {
    shared: Arc<Shared>,
}

impl Drop for Lifetime {
    fn drop(&mut self) {
        self.shared.closing.store(true, Ordering::Release);
    }
}

pub struct Client {
    lifetime: Arc<Lifetime>,
    commands: SyncSender<Command>,
}

pub struct RenderEndpoint {
    lifetime: Arc<Lifetime>,
    transport: LocalTransport,
    ports: PortCounts,
}

impl Client {
    pub fn prepare(
        address: SocketAddr,
        kind: Kind,
        config: Config,
    ) -> io::Result<(RenderEndpoint, Self)> {
        let (mut connection, media, _, _) =
            Connection::open(address, kind, config, Reconnect::Recreate, None)?;
        let inactive = connection.call(Action::Active(false))?;
        inactive.result.map_err(io::Error::other)?;
        Self::spawn(
            kind,
            config,
            Some(RemoteFxTarget {
                address,
                reconnect: Reconnect::Recreate,
                dry_on_loss: false,
            }),
            Some((connection, media, inactive.status)),
        )
    }

    pub fn prepare_slot(
        kind: Kind,
        config: Config,
        default_target: Option<RemoteFxTarget>,
    ) -> io::Result<(RenderEndpoint, Self)> {
        Self::spawn(kind, config, default_target, None)
    }

    fn spawn(
        kind: Kind,
        config: Config,
        default_target: Option<RemoteFxTarget>,
        initial: Option<(Connection, MediaSession, Status)>,
    ) -> io::Result<(RenderEndpoint, Self)> {
        let (transport, media_worker) =
            ClientMedia::prepare(config).map_err(|error| io::Error::other(format!("{error:?}")))?;
        let (connection, media, status) = match initial {
            Some((connection, media, status)) => (Some(connection), Some(media), status),
            None => (None, None, Status::default()),
        };
        let eager = connection.is_some();
        let shared = Arc::new(Shared {
            observation: Mutex::new(Observation {
                owner: 1,
                generation: u64::from(eager),
                status,
                connected: eager,
                ..Observation::default()
            }),
            closing: AtomicBool::new(false),
            desired_active: AtomicBool::new(false),
            dry_on_loss: AtomicBool::new(default_target.is_some_and(|target| target.dry_on_loss)),
            target: Mutex::new(None),
            claim: AtomicU64::new(0),
            claim_result: AtomicU64::new(0),
            reset_requested: AtomicU64::new(0),
            reset_done: AtomicU64::new(0),
            media_changed: AtomicBool::new(true),
            media_failed: AtomicBool::new(false),
            media_thread: OnceLock::new(),
            media: Mutex::new(media),
        });
        let lifetime = Arc::new(Lifetime {
            shared: Arc::clone(&shared),
        });
        let (commands, requested) = mpsc::sync_channel(COMMANDS);
        let media_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("remote-fx-audio".into())
            .spawn(move || media_loop(&media_shared, media_worker))?;
        thread::Builder::new()
            .name("remote-fx-client".into())
            .spawn(move || {
                control_loop(shared, requested, connection, kind, config, default_target)
            })?;
        let endpoint = RenderEndpoint {
            lifetime: Arc::clone(&lifetime),
            transport,
            ports: config.ports(),
        };
        Ok((endpoint, Self { lifetime, commands }))
    }

    pub fn observation(&self) -> Observation {
        *self.lifetime.shared.observation.lock().unwrap()
    }

    pub fn claimed(&self, generation: u32) -> Option<bool> {
        let result = self.lifetime.shared.claim_result.load(Ordering::Acquire);
        (result >> 1 == u64::from(generation)).then_some(result & 1 != 0)
    }

    pub fn configure_claim(&self, generation: u32, target: Option<RemoteFxTarget>) {
        *self.lifetime.shared.target.lock().unwrap() = target.map(|target| (generation, target));
    }

    pub fn request(&self, owner: u64, generation: u64, action: Action) -> Result<Ticket, String> {
        self.submit((owner, generation), Operation::Remote(action))
    }

    pub fn recover(&self, owner: u64, generation: u64) -> Result<Ticket, String> {
        self.submit((owner, generation), Operation::Recover)
    }

    fn submit(&self, owner: (u64, u64), operation: Operation) -> Result<Ticket, String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command {
                operation,
                owner,
                reply,
            })
            .map_err(|_| "FX control queue is unavailable".to_owned())?;
        Ok(Ticket(result))
    }
}

impl AsyncTransport for RenderEndpoint {
    fn try_submit(&mut self, batch: AsyncBatch<'_>) -> Result<(), Error> {
        self.lifetime
            .shared
            .desired_active
            .store(true, Ordering::Release);
        self.transport.try_submit(batch)
    }

    fn try_completion(&mut self) -> Option<AsyncCompletion<'_>> {
        self.transport.try_completion()
    }
}

impl Backend for RenderEndpoint {
    fn dry_on_missing(&self) -> bool {
        self.lifetime.shared.dry_on_loss.load(Ordering::Relaxed)
    }
    fn claim(&mut self, generation: u32, inputs: u8, outputs: u8) {
        let shared = &self.lifetime.shared;
        let claim = (u64::from(generation) << 16) | (u64::from(outputs) << 8) | u64::from(inputs);
        shared.claim.store(claim, Ordering::Release);
    }
    fn ports(&self) -> PortCounts {
        self.ports
    }

    fn async_transport(&mut self) -> Option<&mut dyn AsyncTransport> {
        Some(self)
    }

    fn process(&mut self, _batch: &mut Batch<'_, '_>) -> Result<(), Error> {
        Err(Error::InvalidState)
    }

    fn suspend(&mut self) -> Result<(), Error> {
        self.lifetime
            .shared
            .desired_active
            .store(false, Ordering::Release);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.suspend()
    }

    fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
        let shared = &self.lifetime.shared;
        if step == 0 {
            shared.desired_active.store(false, Ordering::Release);
            shared.claim.store(0, Ordering::Release);
            shared.reset_requested.fetch_add(1, Ordering::Release);
            return Ok(false);
        }
        if shared.reset_done.load(Ordering::Acquire)
            != shared.reset_requested.load(Ordering::Acquire)
        {
            return Ok(false);
        }
        Ok(true)
    }
}

fn media_loop(shared: &Shared, mut worker: ClientMedia) {
    let _ = shared.media_thread.set(thread::current());
    let mut session: Option<MediaSession> = None;
    let mut published = Instant::now();
    while !shared.closing.load(Ordering::Acquire) {
        if let Some(next) = shared.take_media_update() {
            if session.is_some() {
                worker.disconnect();
            }
            if let Some(link) = &next {
                worker.set_config(link.config);
            }
            session = next;
        }
        if let Some(link) = &session {
            if worker.poll(&link.socket, link.token).is_err() {
                shared.media_failed.store(true, Ordering::Release);
                session = None;
                worker.disconnect();
            }
        } else {
            worker.disconnect();
        }
        if published.elapsed() >= HEARTBEAT {
            shared.observation.lock().unwrap().media = worker.counters();
            published = Instant::now();
        }
        if session.is_none() {
            thread::park_timeout(Duration::from_millis(10));
        } else {
            thread::sleep(if shared.desired_active.load(Ordering::Relaxed) {
                POLL
            } else {
                Duration::from_millis(1)
            });
        }
    }
}

fn control_loop(
    shared: Arc<Shared>,
    commands: Receiver<Command>,
    mut connection: Option<Connection>,
    kind: Kind,
    config: Config,
    default_target: Option<RemoteFxTarget>,
) {
    let eager = connection.is_some();
    let mut checkpoint = None;
    let mut notified_active = false;
    let mut owner_reset = 0;
    let mut heartbeat = Instant::now();
    let mut retry = Instant::now();
    let mut claim = 0;
    let mut credential = None;
    let mut target = default_target;
    while !shared.closing.load(Ordering::Acquire) {
        let command = match commands.recv_timeout(Duration::from_millis(10)) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let requested = shared.reset_requested.load(Ordering::Acquire);
        let reset = requested != owner_reset;
        let valid = command.as_ref().is_none_or(|command| {
            let observation = shared.observation.lock().unwrap();
            command.owner == (observation.owner, observation.generation) && !reset
        });
        let recover = valid
            && command
                .as_ref()
                .is_some_and(|command| matches!(command.operation, Operation::Recover));
        if reset {
            if let Some(target) = target {
                close_chain(target.address, &mut connection, credential.take());
            }
        }
        if shared.media_failed.swap(false, Ordering::AcqRel) || reset {
            disconnect(&shared, &mut connection);
            retry = Instant::now() - RETRY_INTERVAL;
        }
        if recover {
            retry = Instant::now() - RETRY_INTERVAL;
        }
        if reset {
            checkpoint = None;
            owner_reset = requested;
            let mut observation = shared.observation.lock().unwrap();
            observation.owner = observation.owner.wrapping_add(1);
            shared.claim_result.store(0, Ordering::Release);
            if !eager {
                shared.reset_done.store(owner_reset, Ordering::Release);
            }
        }
        let desired = shared.desired_active.load(Ordering::Acquire);
        let requested_claim = shared.claim.load(Ordering::Acquire);
        if requested_claim != claim {
            claim = requested_claim;
            retry = Instant::now() - RETRY_INTERVAL;
            let generation = (claim >> 16) as u32;
            target = shared
                .target
                .lock()
                .unwrap()
                .filter(|(configured, _)| generation == *configured)
                .map(|(_, target)| target)
                .or(default_target);
            shared.dry_on_loss.store(
                target.is_some_and(|target| target.dry_on_loss),
                Ordering::Relaxed,
            );
            if claim != 0 && target.is_none() {
                shared
                    .claim_result
                    .store(u64::from(generation) << 1, Ordering::Release);
            }
        }
        let claimed_generation = claim >> 16;
        let first_open = shared.claim_result.load(Ordering::Relaxed) == 0;
        let open_target = target.filter(|target| {
            connection.is_none()
                && (eager || claim != 0)
                && (target.reconnect != Reconnect::Manual || first_open || recover)
                && retry.elapsed() >= RETRY_INTERVAL
        });
        if let Some(RemoteFxTarget {
            address,
            reconnect: policy,
            ..
        }) = open_target
        {
            retry = Instant::now();
            let mut config = config;
            if !eager {
                config.input.channels = claim as u8;
                config.output.channels = (claim >> 8) as u8;
            }
            let opened = Connection::open(address, kind, config, policy, credential.as_ref())
                .and_then(|(mut link, media, next, resumed)| {
                    credential = next;
                    if let Some(project) = checkpoint.as_ref().filter(|_| !resumed) {
                        link.call(Action::Restore(String::clone(project)))?
                            .result
                            .map_err(io::Error::other)?;
                    }
                    let ready = link.call(Action::Active(desired))?;
                    ready.result.map_err(io::Error::other)?;
                    Ok((link, media, ready.status))
                });
            if let Ok((link, media, status)) = opened {
                connection = Some(link);
                notified_active = desired;
                let mut observation = shared.observation.lock().unwrap();
                observation.connected = true;
                observation.status = status;
                observation.generation = observation.generation.wrapping_add(1);
                shared.replace_media(Some(media));
                shared
                    .claim_result
                    .store((claimed_generation << 1) | 1, Ordering::Release);
            } else if claimed_generation != 0 && shared.claim_result.load(Ordering::Relaxed) == 0 {
                if let Err(error) = opened {
                    eprintln!("Remote Carla at {address}: {error}");
                }
                shared
                    .claim_result
                    .store(claimed_generation << 1, Ordering::Release);
            }
        }
        if connection.is_some() && owner_reset != shared.reset_done.load(Ordering::Relaxed) {
            shared.reset_done.store(owner_reset, Ordering::Release);
        }
        let action = if valid {
            command.as_ref().map(|command| match &command.operation {
                Operation::Remote(Action::Restore(state)) => Action::Restore(state.clone()),
                Operation::Remote(Action::Active(active)) => Action::Active(*active),
                Operation::Remote(Action::Editor(visible)) => Action::Editor(*visible),
                Operation::Remote(Action::Save) => Action::Save,
                Operation::Remote(Action::Status) => Action::Status,
                Operation::Remote(Action::Close) => Action::Close,
                Operation::Recover => Action::Editor(true),
            })
        } else {
            None
        };
        let mut result = None;
        if let Some(link) = &mut connection {
            let next = action
                .or_else(|| (desired != notified_active).then_some(Action::Active(desired)))
                .or_else(|| (heartbeat.elapsed() >= HEARTBEAT).then_some(Action::Status));
            if let Some(action) = next {
                let restored = match &action {
                    Action::Restore(project) => Some(project.clone()),
                    _ => None,
                };
                let saving = matches!(action, Action::Save);
                let active = match action {
                    Action::Active(value) => Some(value),
                    _ => None,
                };
                match link.call(action) {
                    Ok(response) => {
                        heartbeat = Instant::now();
                        shared.observation.lock().unwrap().status = response.status;
                        if let Ok(state) = &response.result {
                            if saving {
                                checkpoint = state.clone();
                            }
                            if let Some(restored) = restored {
                                checkpoint = Some(restored);
                            }
                            if let Some(active) = active {
                                notified_active = active;
                            }
                        }
                        result = Some(response.result);
                    }
                    Err(_) => disconnect(&shared, &mut connection),
                }
            }
        }
        if let Some(command) = command {
            let result = if valid {
                result.unwrap_or_else(|| Err("FX service is disconnected".into()))
            } else {
                Err("FX processor owner has changed".into())
            };
            let _ = command.reply.send(result);
        }
    }
    if let Some(target) = target {
        close_chain(target.address, &mut connection, credential);
    }
    disconnect(&shared, &mut connection);
}

fn close_chain(
    address: SocketAddr,
    connection: &mut Option<Connection>,
    credential: Option<Credential>,
) {
    if connection
        .take()
        .is_some_and(|mut link| link.call(Action::Close).is_ok())
    {
        return;
    }
    if let Some(credential) = credential {
        let _ = connect(address, Intent::Release { credential });
    }
}

fn disconnect(shared: &Shared, connection: &mut Option<Connection>) {
    *connection = None;
    let mut observation = shared.observation.lock().unwrap();
    observation.connected = false;
    observation.status = Status {
        lifecycle: Lifecycle::Unavailable,
        ..Status::default()
    };
    shared.replace_media(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{describe, serve, set_editor, shutdown, Chain, ChainControls, Dsp, MediaLimits};
    use rt_runtime::buffer::MidiBuffer;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    use std::net::TcpListener;

    #[global_allocator]
    static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
        shoop_wasm_test_support::heap::TrackingAllocator;

    #[derive(Default)]
    struct TestChain(Status);

    impl ChainControls for TestChain {
        fn status(&self) -> Status {
            self.0
        }
        fn active(&mut self, active: bool) -> Result<(), String> {
            self.0.active = active;
            self.0.lifecycle = Lifecycle::Running;
            Ok(())
        }
        fn editor(&mut self, visible: bool) -> Result<(), String> {
            self.0.editor_visible = Some(visible);
            Ok(())
        }
        fn save(&mut self) -> Result<String, String> {
            Ok(String::new())
        }
        fn restore(&mut self, _: String) -> Result<(), String> {
            Ok(())
        }
    }

    impl Dsp for TestChain {
        fn process(
            &mut self,
            batch: AsyncBatch<'_>,
            output: &mut [f32],
            _: &mut [MidiBuffer],
        ) -> Result<(), Error> {
            assert_eq!(batch.audio.len(), 3 * batch.frames);
            assert_eq!(output.len(), 5 * batch.frames);
            output[..batch.audio.len()].copy_from_slice(batch.audio);
            Ok(())
        }
    }

    fn wait_for(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(
                Instant::now() < deadline,
                "FX chain did not reach the expected state"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[shoop_test]
    fn owner_claims_asymmetric_chain_and_resumes_the_live_editor_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            serve(listener, Arc::new(AtomicBool::new(false)), |_, _| {
                Ok(Chain {
                    dsp: Box::new(TestChain::default()),
                    controls: Box::new(TestChain::default()),
                })
            })
            .unwrap();
        });
        let ports = MediaLimits {
            channels: 16,
            midi: true,
            midi_events: 4,
            midi_bytes: 16,
        };
        let config = Config {
            sample_rate: 8_000,
            quantum: 32,
            batch_quanta: 1,
            slack_quanta: 12,
            input: ports,
            output: ports,
            copies: 2,
            datagram_bytes: 1200,
        };
        for policy in [Reconnect::Resume, Reconnect::Manual] {
            let (mut endpoint, client) =
                Client::prepare_slot(Kind::Patchbay16x, config, None).unwrap();
            let empty = describe(address).unwrap();
            assert!(empty.chains.is_empty());
            client.configure_claim(
                1,
                Some(RemoteFxTarget {
                    address,
                    reconnect: policy,
                    dry_on_loss: true,
                }),
            );
            assert_no_heap(|| endpoint.claim(1, 3, 5));
            wait_for(|| client.claimed(1) == Some(true));
            assert!(endpoint.dry_on_missing());
            let started = describe(address).unwrap();
            let chain = &started.chains[0];
            assert_eq!(
                (chain.config.input.channels, chain.config.output.channels),
                (3, 5)
            );
            assert!(chain.attached);
            set_editor(address, started.instance.clone(), chain.id.clone(), true).unwrap();
            assert_eq!(
                describe(address).unwrap().chains[0].status.editor_visible,
                Some(true)
            );

            let observed = client.observation();
            if policy == Reconnect::Manual {
                client
                    .lifetime
                    .shared
                    .media_failed
                    .store(true, Ordering::Release);
                wait_for(|| !describe(address).unwrap().chains[0].attached);
                assert!(!client.observation().connected);
                client
                    .recover(observed.owner, observed.generation)
                    .unwrap()
                    .wait()
                    .unwrap();
            } else {
                client
                    .lifetime
                    .shared
                    .media_failed
                    .store(true, Ordering::Release);
            }
            wait_for(|| client.observation().generation > observed.generation);
            let resumed = describe(address).unwrap();
            assert_eq!(resumed.chains[0].id, chain.id);
            assert_eq!(resumed.chains[0].status.editor_visible, Some(true));

            let midi = [MidiBuffer::new(4, 16).unwrap()];
            let input = [0.5; 32 * 16];
            assert_no_heap(|| {
                endpoint
                    .try_submit(AsyncBatch {
                        tag: rt_buffered::BatchTag {
                            generation: 1,
                            sequence: 0,
                            source: rt_runtime::Frame(0),
                        },
                        frames: 32,
                        audio: &input,
                        midi: &midi,
                        parameters: &[],
                    })
                    .unwrap()
            });
            wait_for(|| {
                let Some(completion) = endpoint.try_completion() else {
                    return false;
                };
                let output = completion.output.unwrap();
                assert!(output.audio[..3 * 32].iter().all(|value| *value == 0.5));
                assert!(output.audio[3 * 32..].iter().all(|value| *value == 0.0));
                true
            });
            assert_eq!(endpoint.reset_step(0), Ok(false));
            wait_for(|| endpoint.reset_step(1) == Ok(true));
            assert!(describe(address).unwrap().chains.is_empty());
        }
        let instance = describe(address).unwrap().instance;
        shutdown(address, instance).unwrap();
        server.join().unwrap();
    }
}
