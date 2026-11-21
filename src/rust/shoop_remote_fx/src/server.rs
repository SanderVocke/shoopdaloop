use crate::{
    control::{
        read_frame, write_frame, Action, ChainDescription, Credential, Description, Greeting,
        Intent, Kind, Lifecycle, Request, Response, Status, Welcome, CONTROL_TIMEOUT,
        CONTROL_VERSION, DESCRIPTION_LIMIT, INITIAL_LIMIT, LEASE, STATE_LIMIT,
    },
    Config, Counters, Dsp, ServerMedia,
};
use std::{
    collections::BTreeMap,
    io,
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_CONNECTIONS: usize = 64;
const MEDIA_BUDGET: usize = 512 * 1024 * 1024;
const WIRE_BUDGET: usize = 256 * 1024 * 1024;
const RETAIN_CHAIN: Duration = Duration::from_secs(30);

pub trait ChainControls: Send {
    fn status(&self) -> Status;
    fn active(&mut self, active: bool) -> Result<(), String>;
    fn editor(&mut self, visible: bool) -> Result<(), String>;
    fn save(&mut self) -> Result<String, String>;
    fn restore(&mut self, state: String) -> Result<(), String>;
}

pub struct Chain {
    pub dsp: Box<dyn Dsp>,
    pub controls: Box<dyn ChainControls>,
}

struct Resources {
    slots: usize,
    media: usize,
    wire: usize,
}

struct Server {
    instance: String,
    slots: usize,
    resources: Arc<Mutex<Resources>>,
    pending: AtomicUsize,
    chains: Mutex<BTreeMap<String, RegisteredChain>>,
    stopping: Arc<AtomicBool>,
    factory: Box<dyn Fn(Kind, Config) -> Result<Chain, String> + Send + Sync>,
}

struct Connection(Arc<Server>);

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.pending.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Reservation {
    resources: Arc<Mutex<Resources>>,
    media: usize,
    wire: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut resources = self.resources.lock().unwrap();
        resources.slots -= 1;
        resources.media -= self.media;
        resources.wire -= self.wire;
    }
}

struct RegisteredChain {
    chain: ChainInfo,
    parked: Option<ParkedChain>,
}

#[derive(Clone)]
struct ChainInfo {
    kind: Kind,
    config: Config,
    secret: Option<String>,
    controls: Arc<Mutex<Box<dyn ChainControls>>>,
    status: Arc<Mutex<Status>>,
}

struct ParkedChain {
    dsp: Box<dyn Dsp>,
    reservation: Reservation,
    expires: Instant,
}

impl Server {
    fn reserve(&self, kind: Kind, config: Config) -> io::Result<Reservation> {
        if config.input.channels > kind.channels()
            || config.output.channels > kind.channels()
            || !config.input.midi
            || !config.output.midi
        {
            return Err(io::Error::other(
                "Requested ports do not match the Carla chain",
            ));
        }
        let timing = config
            .prepare()
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let media = crate::transport_bytes(config)
            .map_err(|error| io::Error::other(format!("{error:?}")))?
            + timing.batch_frames() * usize::from(kind.channels()) * 8
            + 2 * 8192 * usize::from(kind.channels()) * 4
            + 2 * STATE_LIMIT;
        let rate = config.sample_rate as usize;
        let quantum = usize::from(config.quantum);
        let payload = config.input.payload(quantum) + config.output.payload(quantum);
        let fragments = payload.div_ceil(config.fragment_bytes()) + 1;
        let wire = (payload + fragments * (crate::wire::HEADER + 48))
            .saturating_mul(rate.div_ceil(quantum))
            .saturating_mul(usize::from(config.copies));
        let mut resources = self.resources.lock().unwrap();
        if resources.slots == self.slots {
            return Err(io::Error::other(
                "FX service prepared capacity is exhausted",
            ));
        }
        if resources.media.saturating_add(media) > MEDIA_BUDGET {
            return Err(io::Error::other(
                "FX service prepared memory budget is exhausted",
            ));
        }
        if resources.wire.saturating_add(wire) > WIRE_BUDGET {
            return Err(io::Error::other(
                "FX service traffic budget is exhausted for the requested audio and MIDI limits",
            ));
        }
        resources.slots += 1;
        resources.media += media;
        resources.wire += wire;
        Ok(Reservation {
            resources: Arc::clone(&self.resources),
            media,
            wire,
        })
    }

    fn describe(&self) -> Description {
        let reserved = self.resources.lock().unwrap().slots;
        let chains: Vec<_> = self
            .chains
            .lock()
            .unwrap()
            .iter()
            .map(|(id, entry)| ChainDescription {
                id: id.clone(),
                kind: entry.chain.kind,
                config: entry.chain.config,
                attached: entry.parked.is_none(),
                status: *entry.chain.status.lock().unwrap(),
            })
            .collect();
        Description {
            instance: self.instance.clone(),
            pid: std::process::id(),
            slots: self.slots,
            reserved,
            active: chains
                .iter()
                .filter(|chain| chain.attached && chain.status.active)
                .count(),
            kinds: vec![Kind::Rack, Kind::Patchbay, Kind::Patchbay16x],
            streams: chains
                .iter()
                .filter(|chain| chain.attached)
                .map(|chain| chain.status.counters)
                .collect(),
            chains,
        }
    }

    fn local_management(&self, tcp: &TcpStream, instance: &str) -> io::Result<()> {
        let peer = tcp.peer_addr()?.ip();
        if instance == self.instance && (peer.is_loopback() || peer == tcp.local_addr()?.ip()) {
            Ok(())
        } else {
            Err(io::Error::other(
                "FX management requires the service's local identity",
            ))
        }
    }

    fn open(&self, mut tcp: TcpStream) -> io::Result<()> {
        tcp.set_nodelay(true)?;
        tcp.set_write_timeout(Some(CONTROL_TIMEOUT))?;
        let Greeting { version, intent } =
            read_frame(&mut tcp, INITIAL_LIMIT, Duration::from_secs(2))?;
        if version != CONTROL_VERSION {
            return self.refuse(&mut tcp, "Incompatible FX service version".into());
        }
        match intent {
            Intent::Describe => write_frame(
                &mut tcp,
                &Welcome::Description {
                    version: CONTROL_VERSION,
                    description: self.describe(),
                },
                DESCRIPTION_LIMIT,
            ),
            Intent::Shutdown { instance } => {
                if let Err(error) = self.local_management(&tcp, &instance) {
                    return self.refuse(&mut tcp, error.to_string());
                }
                write_frame(&mut tcp, &Welcome::Shutdown, INITIAL_LIMIT)?;
                self.stopping.store(true, Ordering::Release);
                Ok(())
            }
            Intent::Editor {
                instance,
                chain,
                visible,
            } => {
                let result = self.local_management(&tcp, &instance).and_then(|()| {
                    let chain = self
                        .chains
                        .lock()
                        .unwrap()
                        .get(&chain)
                        .map(|entry| entry.chain.clone())
                        .ok_or_else(|| io::Error::other("FX chain has closed"))?;
                    let mut controls = chain.controls.lock().unwrap();
                    controls.editor(visible).map_err(io::Error::other)?;
                    let visible = controls.status().editor_visible;
                    chain.status.lock().unwrap().editor_visible = visible;
                    Ok(())
                });
                match result {
                    Ok(()) => write_frame(&mut tcp, &Welcome::Confirmed {}, INITIAL_LIMIT),
                    Err(error) => self.refuse(&mut tcp, error.to_string()),
                }
            }
            Intent::Open {
                kind,
                config,
                retain,
            } => self.open_chain(tcp, kind, config, retain),
            Intent::Resume { credential } => self.resume_chain(tcp, credential),
            Intent::Release { credential } => {
                let removed = {
                    let mut chains = self.chains.lock().unwrap();
                    if chains.get(&credential.chain).is_some_and(|entry| {
                        entry.parked.is_some()
                            && entry.chain.secret.as_ref() == Some(&credential.secret)
                    }) {
                        chains.remove(&credential.chain)
                    } else {
                        None
                    }
                };
                let reply = if removed.is_some() {
                    Welcome::Confirmed {}
                } else {
                    Welcome::Missing {}
                };
                drop(removed);
                write_frame(&mut tcp, &reply, INITIAL_LIMIT)
            }
        }
    }

    fn refuse(&self, tcp: &mut TcpStream, message: String) -> io::Result<()> {
        write_frame(tcp, &Welcome::Refused(message), INITIAL_LIMIT)
    }

    fn open_chain(
        &self,
        mut tcp: TcpStream,
        kind: Kind,
        config: Config,
        retain: bool,
    ) -> io::Result<()> {
        let prepared = self.reserve(kind, config).and_then(|reservation| {
            let chain = (self.factory)(kind, config).map_err(io::Error::other)?;
            Ok((reservation, chain))
        });
        let (reservation, Chain { dsp, controls }) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return self.refuse(&mut tcp, error.to_string()),
        };
        let id = uuid::Uuid::new_v4().to_string();
        let chain = ChainInfo {
            kind,
            config,
            secret: retain.then(|| uuid::Uuid::new_v4().to_string()),
            controls: Arc::new(Mutex::new(controls)),
            status: Arc::new(Mutex::new(Status::default())),
        };
        self.chains.lock().unwrap().insert(
            id.clone(),
            RegisteredChain {
                chain: chain.clone(),
                parked: None,
            },
        );
        self.attach(tcp, id, chain, reservation, dsp, false)
    }

    fn resume_chain(&self, mut tcp: TcpStream, credential: Credential) -> io::Result<()> {
        let mut chains = self.chains.lock().unwrap();
        let Some(entry) = chains
            .get_mut(&credential.chain)
            .filter(|entry| entry.chain.secret.as_ref() == Some(&credential.secret))
        else {
            drop(chains);
            return write_frame(&mut tcp, &Welcome::Missing {}, INITIAL_LIMIT);
        };
        let Some(parked) = entry.parked.take() else {
            drop(chains);
            return write_frame(&mut tcp, &Welcome::Pending {}, INITIAL_LIMIT);
        };
        if Instant::now() >= parked.expires {
            chains.remove(&credential.chain);
            drop(chains);
            drop(parked);
            return write_frame(&mut tcp, &Welcome::Missing {}, INITIAL_LIMIT);
        }
        let chain = entry.chain.clone();
        drop(chains);
        self.attach(
            tcp,
            credential.chain,
            chain,
            parked.reservation,
            parked.dsp,
            true,
        )
    }

    fn reap_expired(&self) {
        let mut chains = self.chains.lock().unwrap();
        let expired = chains.iter().find_map(|(id, entry)| {
            entry
                .parked
                .as_ref()
                .filter(|parked| Instant::now() >= parked.expires)
                .map(|_| id.clone())
        });
        let removed = expired.and_then(|id| chains.remove(&id));
        drop(chains);
        drop(removed);
    }

    fn attach(
        &self,
        mut tcp: TcpStream,
        id: String,
        chain: ChainInfo,
        reservation: Reservation,
        mut dsp: Box<dyn Dsp>,
        mut retainable: bool,
    ) -> io::Result<()> {
        let result = self.run_attachment(&mut tcp, &id, &chain, dsp.as_mut(), &mut retainable);
        let mut chains = self.chains.lock().unwrap();
        if let Ok(Some(reply)) = result {
            chains.remove(&id);
            drop(chains);
            drop(chain);
            drop(dsp);
            drop(reservation);
            return write_frame(&mut tcp, &reply, STATE_LIMIT);
        }
        if chain.secret.is_some() && retainable && !self.stopping.load(Ordering::Acquire) {
            let entry = chains.get_mut(&id).unwrap();
            chain.status.lock().unwrap().lifecycle = Lifecycle::Unavailable;
            entry.parked = Some(ParkedChain {
                dsp,
                reservation,
                expires: Instant::now() + RETAIN_CHAIN,
            });
        } else {
            chains.remove(&id);
        }
        drop(chains);
        result.map(|_| ())
    }

    fn run_attachment(
        &self,
        tcp: &mut TcpStream,
        id: &str,
        chain: &ChainInfo,
        dsp: &mut dyn Dsp,
        retainable: &mut bool,
    ) -> io::Result<Option<Response>> {
        let config = chain.config;
        let mut server =
            ServerMedia::prepare(config).map_err(|error| io::Error::other(format!("{error:?}")))?;
        let socket = UdpSocket::bind((tcp.local_addr()?.ip(), 0))?;
        let stream = uuid::Uuid::new_v4().as_u128();
        write_frame(
            tcp,
            &Welcome::Ready {
                version: CONTROL_VERSION,
                config,
                udp_port: socket.local_addr()?.port(),
                stream: format!("{stream:032x}"),
                credential: chain.secret.as_ref().map(|secret| Credential {
                    chain: id.to_owned(),
                    secret: secret.clone(),
                }),
            },
            INITIAL_LIMIT,
        )?;
        crate::control::prove_server(&socket, tcp.peer_addr()?.ip(), stream)?;
        *retainable = true;
        socket.set_read_timeout(Some(Duration::from_millis(1)))?;
        let closed = AtomicBool::new(false);
        let counters = Mutex::new(Counters::default());
        thread::scope(|scope| {
            let media = scope.spawn(|| {
                let mut published = Instant::now();
                while !closed.load(Ordering::Acquire) && !self.stopping.load(Ordering::Acquire) {
                    if wait_for_media(&socket)
                        .and_then(|()| server.poll(&socket, stream, dsp))
                        .is_err()
                    {
                        closed.store(true, Ordering::Release);
                        break;
                    }
                    if published.elapsed() >= crate::control::HEARTBEAT {
                        *counters.lock().unwrap() = server.counters();
                        published = Instant::now();
                    }
                }
            });
            let result = self.control(tcp, chain, &closed, &counters);
            closed.store(true, Ordering::Release);
            let _ = media.join();
            result
        })
    }

    fn control(
        &self,
        tcp: &mut TcpStream,
        chain: &ChainInfo,
        closed: &AtomicBool,
        counters: &Mutex<Counters>,
    ) -> io::Result<Option<Response>> {
        let mut previous: Option<Response> = None;
        while !closed.load(Ordering::Acquire) && !self.stopping.load(Ordering::Acquire) {
            let Request { id, action } = read_frame(tcp, STATE_LIMIT, LEASE)?;
            if let Some(previous) = &previous {
                if id == previous.id {
                    write_frame(tcp, previous, STATE_LIMIT)?;
                    continue;
                }
                if id < previous.id {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
            let closing = matches!(action, Action::Close);
            let mut controls = chain.controls.lock().unwrap();
            let result = match action {
                Action::Status | Action::Close => Ok(None),
                Action::Active(active) => controls.active(active).map(|()| None),
                Action::Editor(visible) => controls.editor(visible).map(|()| None),
                Action::Save => controls.save().map(Some),
                Action::Restore(state) => controls.restore(state).map(|()| None),
            };
            let mut status = controls.status();
            drop(controls);
            status.counters = *counters.lock().unwrap();
            if status.lifecycle == Lifecycle::Running
                && previous.as_ref().is_some_and(|previous| {
                    status.counters.missed_batches > previous.status.counters.missed_batches
                })
            {
                status.lifecycle = Lifecycle::Degraded;
            }
            *chain.status.lock().unwrap() = status;
            let reply = Response { id, status, result };
            if closing {
                return Ok(Some(reply));
            }
            write_frame(tcp, &reply, STATE_LIMIT)?;
            previous = Some(reply);
        }
        Ok(None)
    }
}

fn wait_for_media(socket: &UdpSocket) -> io::Result<()> {
    socket.set_nonblocking(false)?;
    let result = socket.peek(&mut [0; crate::wire::DATAGRAM + 1]);
    socket.set_nonblocking(true)?;
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
            ) || error.raw_os_error() == Some(10040) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub fn serve(
    listener: TcpListener,
    slots: usize,
    stopping: Arc<AtomicBool>,
    factory: impl Fn(Kind, Config) -> Result<Chain, String> + Send + Sync + 'static,
) -> io::Result<()> {
    if !(1..=32).contains(&slots) {
        return Err(io::Error::other("FX service needs 1–32 prepared slots"));
    }
    listener.set_nonblocking(true)?;
    let server = Arc::new(Server {
        instance: uuid::Uuid::new_v4().to_string(),
        slots,
        resources: Arc::new(Mutex::new(Resources {
            slots: 0,
            media: 0,
            wire: 0,
        })),
        pending: AtomicUsize::new(0),
        chains: Mutex::new(BTreeMap::new()),
        stopping,
        factory: Box::new(factory),
    });
    let mut connections = Vec::with_capacity(MAX_CONNECTIONS);
    while !server.stopping.load(Ordering::Acquire) {
        server.reap_expired();
        connections.retain(|thread: &thread::JoinHandle<_>| !thread.is_finished());
        match listener.accept() {
            Ok((socket, _)) if server.pending.load(Ordering::Relaxed) < MAX_CONNECTIONS => {
                server.pending.fetch_add(1, Ordering::Relaxed);
                let connection = Connection(Arc::clone(&server));
                let handle = thread::Builder::new()
                    .name("remote-fx-control".into())
                    .spawn(move || {
                        let _ = connection.0.open(socket);
                        drop(connection);
                    })?;
                connections.push(handle);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    for connection in connections {
        let _ = connection.join();
    }
    Ok(())
}
