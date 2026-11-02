use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use shoop_remote_dsp::{stream::Stats, Config};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    time::{Duration, Instant},
};

pub const VERSION: u16 = 1;
pub const MAX_CONTROL: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Algorithm {
    DelayCopy {
        delay: u32,
    },
    Carla {
        patchbay16: bool,
        state: Option<String>,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Configure {
        version: u16,
        config: Config,
        epoch: u64,
        udp_port: u16,
        algorithm: Algorithm,
    },
    Start,
    Pause,
    Stats,
    Save,
    Restore {
        state: String,
    },
    Show {
        visible: bool,
    },
    Ping,
    Stop,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Reply {
    Ready {
        version: u16,
        udp_port: u16,
        latency_frames: Option<u32>,
        receive_buffer: usize,
        send_buffer: usize,
    },
    Ok,
    Status {
        stats: Stats,
        service_gaps: crate::polling::ServiceGaps,
        concealment: Vec<(u64, u64)>,
        next_sequence: u64,
    },
    State {
        state: String,
    },
    Error {
        error: String,
    },
}
pub fn token(path: &Path) -> Result<[u8; 32]> {
    shoop_transport_socket::private::read(path, 32)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("credential must contain exactly 32 bytes"))
}
pub fn write<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<()> {
    let raw = serde_json::to_vec(value)?;
    ensure!(raw.len() <= MAX_CONTROL, "control frame too large");
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(&(raw.len() as u32).to_le_bytes())?;
    stream.write_all(&raw)?;
    stream.set_nonblocking(true)?;
    Ok(())
}
pub fn read<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    timeout: Duration,
    mut tick: impl FnMut(),
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    let mut prefix = [0; 4];
    let mut got = 0;
    while got < 4 {
        tick();
        ensure!(Instant::now() < deadline, "control read timeout");
        match stream.read(&mut prefix[got..]) {
            Ok(0) => anyhow::bail!("control disconnected"),
            Ok(n) => got += n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                std::thread::sleep(Duration::from_micros(100))
            }
            Err(e) => return Err(e.into()),
        }
    }
    let count = u32::from_le_bytes(prefix) as usize;
    ensure!(count > 0 && count <= MAX_CONTROL, "invalid control length");
    let mut raw = vec![0; count];
    got = 0;
    while got < count {
        tick();
        ensure!(Instant::now() < deadline, "control body timeout");
        match stream.read(&mut raw[got..]) {
            Ok(0) => anyhow::bail!("control disconnected"),
            Ok(n) => got += n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                std::thread::sleep(Duration::from_micros(100))
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(serde_json::from_slice(&raw)?)
}
pub fn call(stream: &mut TcpStream, request: &Request) -> Result<Reply> {
    write(stream, request)?;
    let reply = read(stream, Duration::from_secs(10), || {})?;
    if let Reply::Error { error } = reply {
        anyhow::bail!(error);
    }
    Ok(reply)
}
