use crate::{wire::VERSION, Config, Counters};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpStream, UdpSocket},
    time::{Duration, Instant},
};

pub const DEFAULT_COPIES: u8 = 2;
pub const INITIAL_LIMIT: usize = 4096;
pub const STATE_LIMIT: usize = 23 * 1024 * 1024;
const QUERY_TIMEOUT: Duration = Duration::from_secs(1);
pub(crate) const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const LEASE: Duration = Duration::from_secs(10);
pub(crate) const HEARTBEAT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Rack,
    Patchbay,
    Patchbay16x,
}

impl Kind {
    pub fn channels(self) -> u8 {
        match self {
            Self::Rack | Self::Patchbay => 2,
            Self::Patchbay16x => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum Lifecycle {
    Stopped,
    #[default]
    Starting,
    Running,
    Degraded,
    Crashed,
    Restarting,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Status {
    pub active: bool,
    pub lifecycle: Lifecycle,
    pub editor_visible: Option<bool>,
    pub counters: Counters,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Description {
    pub instance: String,
    pub pid: u32,
    pub slots: usize,
    pub reserved: usize,
    pub active: usize,
    pub kinds: Vec<Kind>,
    pub streams: Vec<Counters>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Greeting {
    pub version: u8,
    pub intent: Intent,
}

impl Greeting {
    pub(crate) fn new(intent: Intent) -> Self {
        Self {
            version: VERSION,
            intent,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Intent {
    Describe,
    Shutdown { instance: String },
    Open { kind: Kind, config: Config },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Welcome {
    Description {
        version: u8,
        description: Description,
    },
    Shutdown,
    Ready {
        version: u8,
        config: Config,
        udp_port: u16,
        stream: String,
    },
    Refused(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Request {
    pub id: u64,
    pub action: Action,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Status,
    Active(bool),
    Editor(bool),
    Save,
    Restore(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Response {
    pub id: u64,
    pub status: Status,
    pub result: Result<Option<String>, String>,
}

pub(crate) fn read_frame<T: DeserializeOwned>(
    stream: &mut TcpStream,
    maximum: usize,
    timeout: Duration,
) -> io::Result<T> {
    let end = Instant::now() + timeout;
    let mut header = [0; 4];
    read_until(stream, &mut header, end)?;
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > maximum {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; length];
    read_until(stream, &mut bytes, end)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn read_until(stream: &mut TcpStream, mut data: &mut [u8], end: Instant) -> io::Result<()> {
    while !data.is_empty() {
        let remaining = end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(data) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(length) => data = &mut data[length..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(crate) fn write_frame(
    stream: &mut TcpStream,
    value: &impl Serialize,
    maximum: usize,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > maximum {
        return Err(io::ErrorKind::InvalidData.into());
    }
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)
}

pub(crate) fn connect(address: SocketAddr, intent: Intent) -> io::Result<(TcpStream, Welcome)> {
    let mut stream = TcpStream::connect_timeout(&address, QUERY_TIMEOUT)?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
    write_frame(&mut stream, &Greeting::new(intent), INITIAL_LIMIT)?;
    let welcome = read_frame(&mut stream, INITIAL_LIMIT, CONTROL_TIMEOUT)?;
    match welcome {
        Welcome::Refused(reason) => Err(io::Error::other(reason)),
        welcome => Ok((stream, welcome)),
    }
}

pub fn describe(address: SocketAddr) -> io::Result<Description> {
    match connect(address, Intent::Describe)?.1 {
        Welcome::Description {
            version: VERSION,
            description,
        } => Ok(description),
        _ => Err(io::Error::other("Incompatible FX service description")),
    }
}

pub fn shutdown(address: SocketAddr, instance: String) -> io::Result<()> {
    match connect(address, Intent::Shutdown { instance })?.1 {
        Welcome::Shutdown => Ok(()),
        _ => Err(io::Error::other("Incompatible FX service reply")),
    }
}

pub(crate) fn prove_client(address: SocketAddr, stream: u128) -> io::Result<UdpSocket> {
    let local = match address.ip() {
        IpAddr::V4(_) => "0.0.0.0:0",
        IpAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(local)?;
    socket.connect(address)?;
    socket.set_read_timeout(Some(Duration::from_millis(120)))?;
    let proof = stream.to_le_bytes();
    let mut echo = [0; 17];
    for _ in 0..8 {
        socket.send(&proof)?;
        if socket
            .recv(&mut echo)
            .is_ok_and(|size| size == 16 && echo[..size] == proof)
        {
            socket.set_nonblocking(true)?;
            return Ok(socket);
        }
    }
    Err(io::Error::other(
        "FX UDP return address verification timed out",
    ))
}

pub(crate) fn prove_server(socket: &UdpSocket, peer: IpAddr, stream: u128) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    let end = Instant::now() + CONTROL_TIMEOUT;
    let proof = stream.to_le_bytes();
    let mut candidate = [0; 17];
    while Instant::now() < end {
        if let Ok((size, address)) = socket.recv_from(&mut candidate) {
            if address.ip() == peer && size == 16 && candidate[..size] == proof {
                socket.connect(address)?;
                socket.send(&proof)?;
                socket.set_nonblocking(true)?;
                return Ok(());
            }
        }
    }
    Err(io::Error::other(
        "FX UDP return address verification timed out",
    ))
}
