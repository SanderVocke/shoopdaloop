//! Authenticated loopback connection setup; credentials stay outside logical protocols.
use crate::{Config, NativeStream, Port};
use shoop_transport::handshake::Handshake;
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};
pub type Token = [u8; 32];
pub struct Listener {
    listener: TcpListener,
    token: Token,
    lan: bool,
}
impl Listener {
    pub fn bind() -> io::Result<Self> {
        let mut token = [0; 32];
        getrandom::fill(&mut token).map_err(io::Error::other)?;
        Self::bind_with("127.0.0.1:0".parse().expect("loopback address"), token)
    }
    pub fn bind_with(address: SocketAddr, token: Token) -> io::Result<Self> {
        if !address.ip().is_loopback() {
            return Err(io::Error::other("only loopback endpoints are supported"));
        }
        Self::bind_policy(address, token, false)
    }
    pub fn bind_lan(address: SocketAddr, token: Token) -> io::Result<Self> {
        Self::bind_policy(address, token, true)
    }
    fn bind_policy(address: SocketAddr, token: Token, lan: bool) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            token,
            lan,
        })
    }
    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub fn accept_stream(&self) -> io::Result<Option<TcpStream>> {
        match self.listener.accept() {
            Ok((stream, peer)) if self.lan || peer.ip().is_loopback() => {
                stream.set_nonblocking(true)?;
                Ok(Some(stream))
            }
            Ok(_) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn token(&self) -> &Token {
        &self.token
    }
    pub fn accept(&self, config: Config, timeout: Duration) -> io::Result<Port> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if !self.lan && !peer.ip().is_loopback() {
                        continue;
                    }
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    match authenticate(stream, self.token, true, remaining) {
                        Ok(stream) => return Port::from_stream(stream, config),
                        Err(_) => continue,
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "local connection timed out",
        ))
    }
}
pub fn connect(
    address: SocketAddr,
    token: &Token,
    config: Config,
    timeout: Duration,
) -> io::Result<Port> {
    if !address.ip().is_loopback() {
        return Err(io::Error::other("only loopback endpoints are supported"));
    }
    connect_lan(address, token, config, timeout)
}
pub fn connect_lan(
    address: SocketAddr,
    token: &Token,
    config: Config,
    timeout: Duration,
) -> io::Result<Port> {
    let deadline = Instant::now() + timeout;
    let stream = TcpStream::connect_timeout(&address, timeout)?;
    let stream = authenticate(
        stream,
        *token,
        false,
        deadline.saturating_duration_since(Instant::now()),
    )?;
    Port::from_stream(stream, config)
}

pub fn authenticate(
    stream: TcpStream,
    token: Token,
    server: bool,
    timeout: Duration,
) -> io::Result<TcpStream> {
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;
    let mut io = NativeStream(stream);
    let mut handshake = Handshake::new(token, server, timeout);
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
                        io::ErrorKind::PermissionDenied
                    },
                    format!("connection authentication failed: {error:?}"),
                ))
            }
        }
        last = now;
        std::thread::sleep(Duration::from_millis(1));
    }
}
