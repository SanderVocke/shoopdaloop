use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::BTreeSet,
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};

const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 74, 80), 24509);
const MAGIC: &[u8; 8] = b"SHOOPFX?";
const QUERY_BYTES: usize = 8 + 1 + 16;
const REPLY_BYTES: usize = QUERY_BYTES + 2;
const DISCOVERY_TIME: Duration = Duration::from_millis(300);
const MAX_SERVICES: usize = 32;

pub fn discover_lan() -> io::Result<Vec<SocketAddr>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_multicast_ttl_v4(1)?;
    let mut query = [0u8; QUERY_BYTES];
    query[..8].copy_from_slice(MAGIC);
    query[8] = crate::CONTROL_VERSION;
    query[9..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    socket.send_to(&query, GROUP)?;
    let deadline = Instant::now() + DISCOVERY_TIME;
    let mut found = BTreeSet::new();
    let mut response = [0u8; REPLY_BYTES + 1];
    while found.len() < MAX_SERVICES {
        let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
        else {
            break;
        };
        socket.set_read_timeout(Some(remaining))?;
        match socket.recv_from(&mut response) {
            Ok((REPLY_BYTES, mut source)) if response[..QUERY_BYTES] == query => {
                let port = u16::from_be_bytes([response[QUERY_BYTES], response[QUERY_BYTES + 1]]);
                if port != 0 {
                    source.set_port(port);
                    found.insert(source);
                }
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                break
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(found.into_iter().collect())
}

pub(crate) struct Advertiser {
    receiver: UdpSocket,
    reply: UdpSocket,
    port: u16,
}

impl Advertiser {
    pub(crate) fn bind(address: SocketAddr) -> io::Result<Option<Self>> {
        let SocketAddr::V4(address) = address else {
            return Ok(None);
        };
        if address.ip().is_loopback() {
            return Ok(None);
        }
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, GROUP.port()).into())?;
        socket.join_multicast_v4(GROUP.ip(), address.ip())?;
        socket.set_nonblocking(true)?;
        let reply = UdpSocket::bind((*address.ip(), 0))?;
        reply.set_nonblocking(true)?;
        Ok(Some(Self {
            receiver: socket.into(),
            reply,
            port: address.port(),
        }))
    }

    pub(crate) fn answer(&self) {
        let mut packet = [0u8; REPLY_BYTES];
        if let Ok((QUERY_BYTES, source)) = self.receiver.recv_from(&mut packet) {
            if &packet[..8] == MAGIC && packet[8] == crate::CONTROL_VERSION {
                packet[QUERY_BYTES..].copy_from_slice(&self.port.to_be_bytes());
                let _ = self.reply.send_to(&packet, source);
            }
        }
    }
}
