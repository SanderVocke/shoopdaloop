use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use shoop_remote_dsp::Config;
use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    #[default]
    Udp,
    Tcp,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum InputPolicy {
    #[default]
    Gap,
    Complete,
}
#[derive(Default, Serialize)]
struct Counters {
    queue_high_water_bytes: usize,
    write_would_block: u64,
    error: Option<String>,
}

pub(crate) struct Framing {
    receive: Vec<u8>,
    received: usize,
    body_length: Option<usize>,
    transmit: Vec<u8>,
    head: usize,
    queued: usize,
    maximum: usize,
    counters: Counters,
}
impl Framing {
    fn new(maximum: usize, capacity: usize) -> Self {
        Self {
            receive: vec![0; maximum + 4],
            received: 0,
            body_length: None,
            transmit: vec![0; capacity.max(maximum + 4)],
            head: 0,
            queued: 0,
            maximum,
            counters: Counters::default(),
        }
    }
    fn receive(&mut self, reader: &mut impl Read, output: &mut [u8]) -> io::Result<usize> {
        for _ in 0..32 {
            if self.received == 4 && self.body_length.is_none() {
                let length = u32::from_le_bytes(self.receive[..4].try_into().unwrap()) as usize;
                if length == 0 || length > self.maximum || length > output.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid audio frame length",
                    ));
                }
                self.body_length = Some(length);
            }
            let target = self.body_length.map_or(4, |length| length + 4);
            if self.received == target {
                let length = self.body_length.unwrap();
                output[..length].copy_from_slice(&self.receive[4..target]);
                self.received = 0;
                self.body_length = None;
                return Ok(length);
            }
            match reader.read(&mut self.receive[self.received..target]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "audio connection closed",
                    ))
                }
                Ok(n) => self.received += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Err(io::ErrorKind::WouldBlock.into())
    }
    fn enqueue(&mut self, packet: &[u8]) -> io::Result<()> {
        if packet.is_empty() || packet.len() > self.maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid outgoing audio length",
            ));
        }
        if packet.len() + 4 > self.transmit.len() - self.queued {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "bounded TCP audio queue exhausted; stream cannot skip bytes",
            ));
        }
        for bytes in [&(packet.len() as u32).to_le_bytes()[..], packet] {
            let tail = (self.head + self.queued) % self.transmit.len();
            let first = bytes.len().min(self.transmit.len() - tail);
            self.transmit[tail..tail + first].copy_from_slice(&bytes[..first]);
            self.transmit[..bytes.len() - first].copy_from_slice(&bytes[first..]);
            self.queued += bytes.len();
        }
        self.counters.queue_high_water_bytes =
            self.counters.queue_high_water_bytes.max(self.queued);
        Ok(())
    }
    fn flush(&mut self, writer: &mut impl Write) -> io::Result<()> {
        for _ in 0..32 {
            if self.queued == 0 {
                break;
            }
            let count = self.queued.min(self.transmit.len() - self.head);
            match writer.write(&self.transmit[self.head..self.head + count]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    self.head = (self.head + n) % self.transmit.len();
                    self.queued -= n;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.counters.write_would_block += 1;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

pub enum Audio {
    Udp {
        socket: UdpSocket,
        peer: SocketAddr,
        connected: bool,
    },
    Listening {
        listener: TcpListener,
        token: [u8; 32],
        peer: IpAddr,
        config: Config,
        buffer_size: usize,
    },
    Tcp {
        stream: TcpStream,
        frames: Framing,
    },
}
fn buffers(socket: &socket2::SockRef<'_>, size: usize) -> io::Result<()> {
    socket.set_recv_buffer_size(size)?;
    socket.set_send_buffer_size(size)
}
impl Audio {
    pub fn udp_client(address: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        buffers(&socket2::SockRef::from(&socket), 1_048_576)?;
        socket.set_nonblocking(true)?;
        Ok(Self::Udp {
            socket,
            peer: address,
            connected: false,
        })
    }
    pub fn server(
        transport: Transport,
        address: SocketAddr,
        peer: SocketAddr,
        config: Config,
        buffer_size: usize,
    ) -> io::Result<Self> {
        match transport {
            Transport::Udp => {
                let socket = UdpSocket::bind(address)?;
                buffers(&socket2::SockRef::from(&socket), buffer_size)?;
                socket.set_nonblocking(true)?;
                Ok(Self::Udp {
                    socket,
                    peer,
                    connected: false,
                })
            }
            Transport::Tcp => {
                let socket = socket2::Socket::new(
                    socket2::Domain::for_address(address),
                    socket2::Type::STREAM,
                    Some(socket2::Protocol::TCP),
                )?;
                socket.set_reuse_address(true)?;
                socket.bind(&address.into())?;
                socket.listen(8)?;
                let listener: TcpListener = socket.into();
                buffers(&socket2::SockRef::from(&listener), buffer_size)?;
                listener.set_nonblocking(true)?;
                let mut token = [0; 32];
                getrandom::fill(&mut token).map_err(io::Error::other)?;
                Ok(Self::Listening {
                    listener,
                    token,
                    peer: peer.ip(),
                    config,
                    buffer_size,
                })
            }
        }
    }
    pub fn tcp(stream: TcpStream, config: Config, buffer_size: usize) -> io::Result<Self> {
        buffers(&socket2::SockRef::from(&stream), buffer_size)?;
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        let maximum = config.max_packet_bytes();
        let packets = config.allowance as usize
            + config.reorder_packets as usize
            + config.multiplier as usize
            + 1;
        Ok(Self::Tcp {
            stream,
            frames: Framing::new(maximum, ((maximum + 4) * packets).min(8 * 1024 * 1024)),
        })
    }
    pub fn activate(self) -> io::Result<Self> {
        let Self::Listening {
            listener,
            token,
            peer,
            config,
            buffer_size,
        } = self
        else {
            return Ok(self);
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "TCP audio attach timed out",
                ));
            }
            match listener.accept() {
                Ok((stream, address)) if address.ip() == peer => {
                    let stream = shoop_transport_socket::setup::authenticate(
                        stream,
                        token,
                        true,
                        deadline.saturating_duration_since(Instant::now()),
                    )?;
                    return Self::tcp(stream, config, buffer_size);
                }
                Ok(_) => {}
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted =>
                {
                    std::thread::sleep(Duration::from_micros(100))
                }
                Err(e) => return Err(e),
            }
        }
    }
    pub fn connect_udp(&mut self, address: SocketAddr) -> io::Result<()> {
        let Self::Udp {
            socket,
            peer,
            connected,
        } = self
        else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        socket.connect(address)?;
        *peer = address;
        *connected = true;
        Ok(())
    }
    pub fn address(&self) -> io::Result<SocketAddr> {
        match self {
            Self::Udp { socket, .. } => socket.local_addr(),
            Self::Listening { listener, .. } => listener.local_addr(),
            Self::Tcp { stream, .. } => stream.local_addr(),
        }
    }
    pub fn token(&self) -> Option<[u8; 32]> {
        match self {
            Self::Listening { token, .. } => Some(*token),
            _ => None,
        }
    }
    pub fn buffer_sizes(&self) -> io::Result<(usize, usize)> {
        let socket = match self {
            Self::Udp { socket, .. } => socket2::SockRef::from(socket),
            Self::Listening { listener, .. } => socket2::SockRef::from(listener),
            Self::Tcp { stream, .. } => socket2::SockRef::from(stream),
        };
        Ok((socket.recv_buffer_size()?, socket.send_buffer_size()?))
    }
    pub fn receive(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Udp { socket, peer, .. } => match socket.recv_from(output) {
                Ok((n, address)) if address == *peer => Ok(n),
                Ok(_) => Err(io::ErrorKind::WouldBlock.into()),
                Err(e) => Err(e),
            },
            Self::Tcp { stream, frames } => {
                let result = frames.receive(stream, output);
                if let Err(e) = &result {
                    if e.kind() != io::ErrorKind::WouldBlock
                        && e.kind() != io::ErrorKind::Interrupted
                    {
                        frames.counters.error.get_or_insert_with(|| e.to_string());
                    }
                }
                result
            }
            Self::Listening { .. } => Err(io::ErrorKind::NotConnected.into()),
        }
    }
    pub fn send(&mut self, packet: &[u8]) -> io::Result<()> {
        match self {
            Self::Udp {
                socket,
                peer,
                connected,
            } => {
                let n = if *connected {
                    socket.send(packet)?
                } else {
                    socket.send_to(packet, *peer)?
                };
                if n != packet.len() {
                    return Err(io::ErrorKind::WriteZero.into());
                }
                Ok(())
            }
            Self::Tcp { stream, frames } => {
                if frames.counters.error.is_some() {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                let result = frames
                    .flush(stream)
                    .and_then(|_| frames.enqueue(packet))
                    .and_then(|_| frames.flush(stream));
                if let Err(e) = &result {
                    frames.counters.error.get_or_insert_with(|| e.to_string());
                }
                result
            }
            Self::Listening { .. } => Err(io::ErrorKind::NotConnected.into()),
        }
    }
    pub fn flush(&mut self) -> io::Result<()> {
        if let Self::Tcp { stream, frames } = self {
            if frames.counters.error.is_some() {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let result = frames.flush(stream);
            if let Err(e) = &result {
                frames.counters.error.get_or_insert_with(|| e.to_string());
            }
            result?;
        }
        Ok(())
    }
    pub fn failed(&self) -> bool {
        matches!(self, Self::Tcp { frames, .. } if frames.counters.error.is_some())
    }
    pub fn report(&self) -> serde_json::Value {
        let (receive, send) = self.buffer_sizes().unwrap_or_default();
        match self {
            Self::Tcp { stream, frames } => {
                serde_json::json!({"transport":"tcp", "nodelay":stream.nodelay().ok(),
                "local_endpoint":stream.local_addr().ok().map(|v|v.to_string()),"peer_endpoint":stream.peer_addr().ok().map(|v|v.to_string()),
                "receive_buffer":receive,"send_buffer":send,"queue_capacity_bytes":frames.transmit.len(),
                "queued_bytes":frames.queued,"counters":frames.counters})
            }
            Self::Udp { socket, peer, .. } => {
                serde_json::json!({"transport":"udp","receive_buffer":receive,"send_buffer":send,
                "local_endpoint":socket.local_addr().ok().map(|v|v.to_string()),"peer_endpoint":peer.to_string()})
            }
            Self::Listening { .. } => serde_json::Value::Null,
        }
    }
}

pub fn runtime_info() -> serde_json::Value {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap_or_default();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .map_or("", |(_, rest)| rest)
        .split_whitespace()
        .collect();
    let number = |index| {
        fields
            .get(index)
            .and_then(|value: &&str| value.parse::<i64>().ok())
    };
    let status = std::fs::read_to_string("/proc/thread-self/status").unwrap_or_default();
    let affinity = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .map(str::trim);
    serde_json::json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "kernel":std::fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|v|v.trim().to_string()),
        "scheduler":number(38),"realtime_priority":number(37),"nice":number(16),"affinity":affinity})
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fragmented {
        bytes: std::collections::VecDeque<u8>,
        block: bool,
    }
    impl Read for Fragmented {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.block = !self.block;
            if self.block {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            match self.bytes.pop_front() {
                Some(byte) => {
                    out[0] = byte;
                    Ok(1)
                }
                None => Ok(0),
            }
        }
    }
    impl Write for Fragmented {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            self.block = !self.block;
            if self.block {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.bytes.push_back(input[0]);
            Ok(1)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[shoop_wasm_test_support::shoop_test(no_wasm = "native TCP audio framing")]
    fn fragmented_reads_writes_and_wrapped_queue_preserve_every_byte() {
        let mut frames = Framing::new(12, 23);
        let mut wire = Fragmented {
            bytes: Default::default(),
            block: false,
        };
        for _ in 0..3 {
            frames.enqueue(b"first").unwrap();
            frames.enqueue(b"second").unwrap();
            while frames.queued != 0 {
                frames.flush(&mut wire).unwrap();
            }
            let mut output = [0; 12];
            for expected in [b"first".as_slice(), b"second".as_slice()] {
                loop {
                    match frames.receive(&mut wire, &mut output) {
                        Ok(n) => {
                            assert_eq!(&output[..n], expected);
                            break;
                        }
                        Err(e) => assert_eq!(e.kind(), io::ErrorKind::WouldBlock),
                    }
                }
            }
        }
        assert!(frames.counters.write_would_block > 0);
    }
    #[shoop_wasm_test_support::shoop_test(no_wasm = "native TCP audio framing")]
    fn malformed_lengths_truncation_and_backpressure_are_explicit() {
        for length in [0u32, 13, u32::MAX] {
            let mut frames = Framing::new(12, 16);
            assert_eq!(
                frames
                    .receive(&mut io::Cursor::new(length.to_le_bytes()), &mut [0; 12])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut frames = Framing::new(12, 16);
        frames.enqueue(&[0; 12]).unwrap();
        assert_eq!(
            frames.enqueue(&[1]).unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
        let partial = [3, 0, 0, 0, 1, 2];
        assert_eq!(
            frames
                .receive(&mut io::Cursor::new(partial), &mut [0; 12])
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
