//! Compatible socket setup; endpoint metadata belongs to native composition.
use crate::{Config, NativeStream, Port};
pub use shoop_transport::handshake::MAGIC;
use shoop_transport::handshake::{Handshake, Intent};
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

pub struct Listener {
    listener: TcpListener,
    lan: bool,
}
impl Listener {
    pub fn bind() -> io::Result<Self> {
        Self::bind_with("127.0.0.1:0".parse().expect("loopback address"))
    }
    pub fn bind_with(address: SocketAddr) -> io::Result<Self> {
        if !address.ip().is_loopback() {
            return Err(io::Error::other("only loopback endpoints are supported"));
        }
        Self::bind_policy(address, false)
    }
    pub fn bind_lan(address: SocketAddr) -> io::Result<Self> {
        Self::bind_policy(address, true)
    }
    fn bind_policy(address: SocketAddr, lan: bool) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, lan })
    }
    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub fn accept_stream(&self) -> io::Result<Option<TcpStream>> {
        match self.listener.accept() {
            Ok((stream, peer)) if self.lan || peer.ip().is_loopback() => {
                stream.set_nonblocking(true)?;
                stream.set_nodelay(true)?;
                Ok(Some(stream))
            }
            Ok(_) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn accept(&self, config: Config, timeout: Duration) -> io::Result<Port> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(stream) = self.accept_stream()? {
                let mut handshake =
                    Handshake::new(true, deadline.saturating_duration_since(Instant::now()));
                if let Ok(stream) = progress(stream, &mut handshake) {
                    if handshake.intent() == Intent::Connect {
                        return Port::from_stream(stream, config);
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "local connection timed out",
        ))
    }
}
pub fn connect(address: SocketAddr, config: Config, timeout: Duration) -> io::Result<Port> {
    if !address.ip().is_loopback() {
        return Err(io::Error::other("only loopback endpoints are supported"));
    }
    connect_lan(address, config, timeout)
}
pub fn connect_lan(address: SocketAddr, config: Config, timeout: Duration) -> io::Result<Port> {
    let deadline = Instant::now() + timeout;
    let stream = TcpStream::connect_timeout(&address, timeout)?;
    Port::from_stream(
        progress(
            stream,
            &mut Handshake::new(false, deadline.saturating_duration_since(Instant::now())),
        )?,
        config,
    )
}
pub fn inspect(address: SocketAddr, timeout: Duration) -> io::Result<TcpStream> {
    let deadline = Instant::now() + timeout;
    let stream = TcpStream::connect_timeout(&address, timeout)?;
    progress(
        stream,
        &mut Handshake::inspect(deadline.saturating_duration_since(Instant::now())),
    )
}
fn progress(stream: TcpStream, handshake: &mut Handshake) -> io::Result<TcpStream> {
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;
    let mut io = NativeStream(stream);
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        match handshake.advance(&mut io, now.saturating_duration_since(last)) {
            Ok(true) => return Ok(io.0),
            Ok(false) => {}
            Err(error) => {
                return Err(io::Error::new(
                    if error == shoop_transport::handshake::Error::Timeout {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::InvalidData
                    },
                    format!("connection negotiation failed: {error:?}"),
                ))
            }
        }
        last = now;
        std::thread::sleep(Duration::from_millis(1));
    }
}
