use crate::{
    control::{
        connect, prove_client, read_frame, write_frame, Action, Intent, Kind, Request, Response,
        Status, Welcome, CONTROL_TIMEOUT, HEARTBEAT, STATE_LIMIT,
    },
    media::POLL,
    wire::VERSION,
    ClientMedia, Config, Counters, Lifecycle,
};
use rt_buffered::{AsyncBatch, AsyncCompletion, AsyncTransport, Backend, Batch, LocalTransport};
use rt_runtime::{graph::PortCounts, Error};
use std::{
    io,
    net::{SocketAddr, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
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
    fn open(address: SocketAddr, kind: Kind, config: Config) -> io::Result<(Self, MediaSession)> {
        let (tcp, reply) = connect(address, Intent::Open { kind, config })?;
        let Welcome::Ready {
            version: VERSION,
            config: accepted,
            udp_port,
            stream,
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
        Ok((Self { tcp, next: 0 }, MediaSession { socket, token }))
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
    reset_requested: AtomicU64,
    reset_done: AtomicU64,
    media_changed: AtomicBool,
    media_failed: AtomicBool,
    media: Mutex<Option<MediaSession>>,
}

impl Shared {
    fn replace_media(&self, session: Option<MediaSession>) {
        let mut pending = self.media.lock().unwrap();
        *pending = session;
        self.media_changed.store(true, Ordering::Release);
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
        let (transport, media_worker) =
            ClientMedia::prepare(config).map_err(|error| io::Error::other(format!("{error:?}")))?;
        let (mut connection, media) = Connection::open(address, kind, config)?;
        let inactive = connection.call(Action::Active(false))?;
        inactive.result.map_err(io::Error::other)?;
        let status = inactive.status;
        let shared = Arc::new(Shared {
            observation: Mutex::new(Observation {
                owner: 1,
                generation: 1,
                status,
                connected: true,
                ..Observation::default()
            }),
            closing: AtomicBool::new(false),
            desired_active: AtomicBool::new(false),
            reset_requested: AtomicU64::new(0),
            reset_done: AtomicU64::new(0),
            media_changed: AtomicBool::new(true),
            media_failed: AtomicBool::new(false),
            media: Mutex::new(Some(media)),
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
            .spawn(move || control_loop(shared, requested, connection, address, kind, config))?;
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
    let mut session: Option<MediaSession> = None;
    let mut published = Instant::now();
    while !shared.closing.load(Ordering::Acquire) {
        if let Some(next) = shared.take_media_update() {
            if session.is_some() {
                worker.disconnect();
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
        thread::sleep(if shared.desired_active.load(Ordering::Relaxed) {
            POLL
        } else {
            Duration::from_millis(1)
        });
    }
}

fn control_loop(
    shared: Arc<Shared>,
    commands: Receiver<Command>,
    initial: Connection,
    address: SocketAddr,
    kind: Kind,
    config: Config,
) {
    let mut connection = Some(initial);
    let mut checkpoint = None;
    let mut notified_active = false;
    let mut owner_reset = 0;
    let mut heartbeat = Instant::now();
    let mut retry = Instant::now();
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
        if shared.media_failed.swap(false, Ordering::AcqRel) || reset || recover {
            disconnect(&shared, &mut connection);
            retry = Instant::now() - RETRY_INTERVAL;
        }
        if reset {
            checkpoint = None;
            owner_reset = requested;
            let mut observation = shared.observation.lock().unwrap();
            observation.owner = observation.owner.wrapping_add(1);
        }
        let desired = shared.desired_active.load(Ordering::Acquire);
        if connection.is_none() && retry.elapsed() >= RETRY_INTERVAL {
            retry = Instant::now();
            let opened = Connection::open(address, kind, config).and_then(|(mut link, media)| {
                if let Some(project) = &checkpoint {
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
    disconnect(&shared, &mut connection);
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
