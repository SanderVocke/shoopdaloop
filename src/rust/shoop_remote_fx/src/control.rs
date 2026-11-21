use crate::{Config, Counters};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpStream, UdpSocket},
    time::{Duration, Instant},
};

pub const DEFAULT_COPIES: u8 = 2;
pub const CONTROL_VERSION: u8 = 2;
pub const INITIAL_LIMIT: usize = 4096;
pub(crate) const DESCRIPTION_LIMIT: usize = 64 * 1024;
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Rack => "Rack",
            Self::Patchbay => "Patchbay",
            Self::Patchbay16x => "Patchbay 16x",
        }
    }
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
#[serde(deny_unknown_fields)]
pub struct Status {
    pub active: bool,
    pub lifecycle: Lifecycle,
    pub editor_visible: Option<bool>,
    pub counters: Counters,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub instance: String,
    pub pid: u32,
    pub slots: usize,
    pub reserved: usize,
    pub active: usize,
    pub kinds: Vec<Kind>,
    pub streams: Vec<Counters>,
    pub chains: Vec<ChainDescription>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChainDescription {
    pub id: String,
    pub kind: Kind,
    pub config: Config,
    pub attached: bool,
    pub status: Status,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credential {
    pub chain: String,
    pub secret: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Greeting {
    pub version: u8,
    pub intent: Intent,
}

impl Greeting {
    pub(crate) fn new(intent: Intent) -> Self {
        Self {
            version: CONTROL_VERSION,
            intent,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Intent {
    Describe,
    Shutdown {
        instance: String,
    },
    Open {
        kind: Kind,
        config: Config,
        retain: bool,
    },
    Resume {
        credential: Credential,
    },
    Release {
        credential: Credential,
    },
    Editor {
        instance: String,
        chain: String,
        visible: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Welcome {
    Description {
        version: u8,
        description: Description,
    },
    Shutdown,
    Confirmed {},
    Pending {},
    Missing {},
    Ready {
        version: u8,
        config: Config,
        udp_port: u16,
        stream: String,
        credential: Option<Credential>,
    },
    Refused(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub id: u64,
    pub action: Action,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Status,
    Close,
    Active(bool),
    Editor(bool),
    Save,
    Restore(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
    decode_frame(&bytes)
}

fn decode_frame<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    // The framing schema allows a named JSON object or the unit shutdown reply.
    // In particular, reject positional arrays that Serde otherwise accepts for structs.
    let bytes = bytes.trim_ascii();
    if !bytes.starts_with(b"{") && bytes != br#""shutdown""# {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut decoder).map_err(io::Error::other)?;
    decoder.end().map_err(io::Error::other)?;
    Ok(value)
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
    let reply_limit = if matches!(intent, Intent::Describe) {
        DESCRIPTION_LIMIT
    } else {
        INITIAL_LIMIT
    };
    let mut stream = TcpStream::connect_timeout(&address, QUERY_TIMEOUT)?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
    write_frame(&mut stream, &Greeting::new(intent), INITIAL_LIMIT)?;
    let welcome = read_frame(&mut stream, reply_limit, CONTROL_TIMEOUT)?;
    match welcome {
        Welcome::Refused(reason) => Err(io::Error::other(reason)),
        welcome => Ok((stream, welcome)),
    }
}

pub fn describe(address: SocketAddr) -> io::Result<Description> {
    match connect(address, Intent::Describe)?.1 {
        Welcome::Description {
            version: CONTROL_VERSION,
            description,
        } => Ok(description),
        _ => Err(io::Error::other("Incompatible FX service description")),
    }
}

pub fn set_editor(
    address: SocketAddr,
    instance: String,
    chain: String,
    visible: bool,
) -> io::Result<()> {
    match connect(
        address,
        Intent::Editor {
            instance,
            chain,
            visible,
        },
    )?
    .1
    {
        Welcome::Confirmed {} => Ok(()),
        _ => Err(io::Error::other("Incompatible FX service reply")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};

    #[shoop_wasm_test_support::shoop_test]
    fn control_frames_require_named_records_and_known_fields() {
        assert!(decode_frame::<Greeting>(br#" {"version":1,"intent":"describe"} "#).is_ok());
        assert!(decode_frame::<Greeting>(br#"[1,"describe"]"#).is_err());
        assert!(
            decode_frame::<Greeting>(br#"{"version":1,"intent":"describe","extra":0}"#).is_err()
        );
        assert!(decode_frame::<Greeting>(br#"{"version":1,"intent":"describe"}{}"#).is_err());
        assert!(decode_frame::<Greeting>(
            br#"{"version":1,"intent":{"shutdown":{"instance":"fx","extra":0}}}"#
        )
        .is_err());
        assert!(matches!(
            decode_frame::<Welcome>(br#""shutdown""#),
            Ok(Welcome::Shutdown)
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn inspection_can_return_counters_for_all_slots() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let report = Description {
            instance: "desktop".into(),
            pid: 1,
            slots: 32,
            reserved: 32,
            active: 32,
            kinds: vec![Kind::Rack],
            streams: vec![Counters::default(); 32],
            chains: vec![],
        };
        thread::scope(|scope| {
            let server = scope.spawn(|| {
                let (mut peer, _) = listener.accept().unwrap();
                let greeting: Greeting =
                    read_frame(&mut peer, INITIAL_LIMIT, CONTROL_TIMEOUT).unwrap();
                assert!(matches!(greeting.intent, Intent::Describe));
                let response = Welcome::Description {
                    version: CONTROL_VERSION,
                    description: report.clone(),
                };
                write_frame(&mut peer, &response, DESCRIPTION_LIMIT).unwrap();
            });
            assert_eq!(describe(address).unwrap(), report);
            server.join().unwrap();
        });
    }
}
