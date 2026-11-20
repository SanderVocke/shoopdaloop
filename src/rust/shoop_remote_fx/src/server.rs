use crate::{
    control::{
        read_frame, write_frame, Action, Description, Greeting, Intent, Kind, Lifecycle, Request,
        Response, Status, Welcome, CONTROL_TIMEOUT, INITIAL_LIMIT, LEASE, STATE_LIMIT,
    },
    wire::VERSION,
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
    resources: Mutex<Resources>,
    pending: AtomicUsize,
    active: AtomicUsize,
    streams: Mutex<BTreeMap<u128, Arc<Mutex<Counters>>>>,
    stopping: Arc<AtomicBool>,
    factory: Box<dyn Fn(Kind, Config) -> Result<Chain, String> + Send + Sync>,
}

struct Connection(Arc<Server>);

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.pending.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Reservation<'a> {
    server: &'a Server,
    media: usize,
    wire: usize,
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        let mut resources = self.server.resources.lock().unwrap();
        resources.slots -= 1;
        resources.media -= self.media;
        resources.wire -= self.wire;
    }
}

impl Server {
    fn reserve(&self, kind: Kind, config: Config) -> io::Result<Reservation<'_>> {
        if config.input.channels != kind.channels()
            || config.output.channels != kind.channels()
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
            server: self,
            media,
            wire,
        })
    }

    fn describe(&self) -> Description {
        Description {
            instance: self.instance.clone(),
            pid: std::process::id(),
            slots: self.slots,
            reserved: self.resources.lock().unwrap().slots,
            active: self.active.load(Ordering::Relaxed),
            kinds: vec![Kind::Rack, Kind::Patchbay, Kind::Patchbay16x],
            streams: self
                .streams
                .lock()
                .unwrap()
                .values()
                .map(|counters| *counters.lock().unwrap())
                .collect(),
        }
    }

    fn open(&self, mut tcp: TcpStream) -> io::Result<()> {
        tcp.set_nodelay(true)?;
        tcp.set_write_timeout(Some(CONTROL_TIMEOUT))?;
        let Greeting { version, intent } =
            read_frame(&mut tcp, INITIAL_LIMIT, Duration::from_secs(2))?;
        if version != VERSION {
            return write_frame(
                &mut tcp,
                &Welcome::Refused("Incompatible FX service version".into()),
                INITIAL_LIMIT,
            );
        }
        match intent {
            Intent::Describe => write_frame(
                &mut tcp,
                &Welcome::Description {
                    version: VERSION,
                    description: self.describe(),
                },
                INITIAL_LIMIT,
            ),
            Intent::Shutdown { instance } => {
                let peer = tcp.peer_addr()?.ip();
                if instance != self.instance
                    || !(peer.is_loopback() || peer == tcp.local_addr()?.ip())
                {
                    return write_frame(
                        &mut tcp,
                        &Welcome::Refused("FX service shutdown requires its local identity".into()),
                        INITIAL_LIMIT,
                    );
                }
                write_frame(&mut tcp, &Welcome::Shutdown, INITIAL_LIMIT)?;
                self.stopping.store(true, Ordering::Release);
                Ok(())
            }
            Intent::Open { kind, config } => self.open_chain(tcp, kind, config),
        }
    }

    fn open_chain(&self, mut tcp: TcpStream, kind: Kind, config: Config) -> io::Result<()> {
        let prepared = self.reserve(kind, config).and_then(|reservation| {
            let chain = (self.factory)(kind, config).map_err(io::Error::other)?;
            let server = ServerMedia::prepare(config)
                .map_err(|error| io::Error::other(format!("{error:?}")))?;
            Ok((reservation, chain, server))
        });
        let (
            _reservation,
            Chain {
                mut dsp,
                mut controls,
            },
            mut server,
        ) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                return write_frame(
                    &mut tcp,
                    &Welcome::Refused(error.to_string()),
                    INITIAL_LIMIT,
                )
            }
        };
        let socket = UdpSocket::bind((tcp.local_addr()?.ip(), 0))?;
        let stream = uuid::Uuid::new_v4().as_u128();
        write_frame(
            &mut tcp,
            &Welcome::Ready {
                version: VERSION,
                config,
                udp_port: socket.local_addr()?.port(),
                stream: format!("{stream:032x}"),
            },
            INITIAL_LIMIT,
        )?;
        crate::control::prove_server(&socket, tcp.peer_addr()?.ip(), stream)?;
        socket.set_read_timeout(Some(Duration::from_millis(1)))?;

        let closed = AtomicBool::new(false);
        let counters = Arc::new(Mutex::new(Counters::default()));
        self.streams
            .lock()
            .unwrap()
            .insert(stream, Arc::clone(&counters));
        let result = thread::scope(|scope| {
            let media = scope.spawn(|| {
                let mut published = Instant::now();
                while !closed.load(Ordering::Acquire) && !self.stopping.load(Ordering::Acquire) {
                    if wait_for_media(&socket)
                        .and_then(|()| server.poll(&socket, stream, dsp.as_mut()))
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
            let mut active = false;
            let result = self.control(&mut tcp, controls.as_mut(), &closed, &mut active, &counters);
            if active {
                self.active.fetch_sub(1, Ordering::Relaxed);
            }
            closed.store(true, Ordering::Release);
            let _ = media.join();
            result
        });
        self.streams.lock().unwrap().remove(&stream);
        result
    }

    fn control(
        &self,
        tcp: &mut TcpStream,
        chain: &mut dyn ChainControls,
        closed: &AtomicBool,
        active: &mut bool,
        counters: &Mutex<Counters>,
    ) -> io::Result<()> {
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
            let result = match action {
                Action::Status => Ok(None),
                Action::Active(active) => chain.active(active).map(|()| None),
                Action::Editor(visible) => chain.editor(visible).map(|()| None),
                Action::Save => chain.save().map(Some),
                Action::Restore(state) => chain.restore(state).map(|()| None),
            };
            let mut status = chain.status();
            if status.active != *active {
                if status.active {
                    self.active.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.active.fetch_sub(1, Ordering::Relaxed);
                }
                *active = status.active;
            }
            status.counters = *counters.lock().unwrap();
            if status.lifecycle == Lifecycle::Running
                && previous.as_ref().is_some_and(|previous| {
                    status.counters.missed_batches > previous.status.counters.missed_batches
                })
            {
                status.lifecycle = Lifecycle::Degraded;
            }
            let reply = Response { id, status, result };
            write_frame(tcp, &reply, STATE_LIMIT)?;
            previous = Some(reply);
        }
        Ok(())
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
        resources: Mutex::new(Resources {
            slots: 0,
            media: 0,
            wire: 0,
        }),
        pending: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
        streams: Mutex::new(BTreeMap::new()),
        stopping,
        factory: Box::new(factory),
    });
    let mut connections = Vec::with_capacity(MAX_CONNECTIONS);
    while !server.stopping.load(Ordering::Acquire) {
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
