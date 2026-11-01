//! Private launch authentication; tokens never enter the engine schema.
use crate::{Config, Port};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};
pub type Token = [u8; 32];
pub struct Listener {
    listener: TcpListener,
    token: Token,
}
impl Listener {
    pub fn bind() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let mut token = [0; 32];
        getrandom::fill(&mut token).map_err(io::Error::other)?;
        Ok(Self { listener, token })
    }
    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub fn token(&self) -> &Token {
        &self.token
    }
    pub fn accept(&self, config: Config, timeout: Duration) -> io::Result<Port> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    stream.set_nonblocking(false)?;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    stream.set_read_timeout(Some(remaining.max(Duration::from_millis(1))))?;
                    stream.set_write_timeout(Some(remaining.max(Duration::from_millis(1))))?;
                    let mut token = [0; 32];
                    if stream.read_exact(&mut token).is_err() || token != self.token {
                        continue;
                    }
                    stream.write_all(&[1])?;
                    stream.set_read_timeout(None)?;
                    stream.set_write_timeout(None)?;
                    return Port::from_stream(stream, config);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "local engine connection timed out",
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
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(token)?;
    let mut ack = [0];
    stream.read_exact(&mut ack)?;
    if ack != [1] {
        return Err(io::Error::other("local engine authentication failed"));
    }
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;
    Port::from_stream(stream, config)
}
