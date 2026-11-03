use crate::{
    private,
    registry::{AppTransport, HostEntry},
};
use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use shoop_app_protocol::VERSION;
use std::{
    io::{Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkOptions {
    pub heartbeat_interval_ms: u64,
    pub heartbeat_timeout_ms: u64,
    pub snapshot_hz: u32,
}
impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 1000,
            heartbeat_timeout_ms: 5000,
            snapshot_hz: 30,
        }
    }
}
impl NetworkOptions {
    pub fn heartbeat(self) -> shoop_app_connection::HeartbeatConfig {
        shoop_app_connection::HeartbeatConfig {
            interval: Duration::from_millis(self.heartbeat_interval_ms),
            timeout: Duration::from_millis(self.heartbeat_timeout_ms),
        }
    }
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.heartbeat().valid() && (1..=60).contains(&self.snapshot_hz),
            "invalid network heartbeat or snapshot settings"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProfile {
    pub version: u16,

    pub name: String,
    pub endpoint: String,
    pub management: String,
    pub credential: PathBuf,
    #[serde(default)]
    pub network: NetworkOptions,
}
impl RemoteProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == VERSION, "incompatible session protocol");
        ensure!(
            self.name.len() <= 128 && !self.credential.as_os_str().is_empty(),
            "invalid remote profile identity or credential path"
        );
        endpoint(&self.endpoint)?;
        endpoint(&self.management)?;
        self.network.validate()
    }
    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "remote profile exceeds 64 KiB");
        let mut profile: Self = serde_json::from_slice(&bytes)?;
        profile.validate()?;
        if profile.credential.is_relative() {
            profile.credential = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&profile.credential);
        }
        Ok(profile)
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        ensure!(bytes.len() <= 65536, "remote profile exceeds 64 KiB");
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path).map_err(|error| error.error)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Target {
    pub version: u16,

    pub name: String,
    pub endpoint: String,
    pub management: String,
    pub credential: PathBuf,
    pub transport: AppTransport,
    pub shared_memory: Option<Vec<u8>>,
    pub warnings: Vec<String>,
    pub network: Option<NetworkOptions>,
}
impl From<HostEntry> for Target {
    fn from(e: HostEntry) -> Self {
        Self {
            version: e.version,

            name: e.name,
            endpoint: e.endpoint.to_string(),
            management: e.management.to_string(),
            credential: e.credential,
            transport: e.transport,
            shared_memory: e.shared_memory,
            warnings: e.warnings,
            network: None,
        }
    }
}
impl From<RemoteProfile> for Target {
    fn from(e: RemoteProfile) -> Self {
        Self {
            version: e.version,

            name: e.name,
            endpoint: e.endpoint,
            management: e.management,
            credential: e.credential,
            transport: AppTransport::Socket,
            shared_memory: None,
            warnings: Vec::new(),
            network: Some(e.network),
        }
    }
}
pub trait ConnectionTarget {
    fn target(&self) -> Target;
}
impl ConnectionTarget for HostEntry {
    fn target(&self) -> Target {
        self.clone().into()
    }
}
impl ConnectionTarget for RemoteProfile {
    fn target(&self) -> Target {
        self.clone().into()
    }
}
impl ConnectionTarget for Target {
    fn target(&self) -> Target {
        self.clone()
    }
}
impl Target {
    pub fn token(&self) -> Result<[u8; 32]> {
        ensure!(self.version == VERSION, "incompatible session protocol");
        if let Some(network) = self.network {
            network.validate()?;
            ensure!(
                self.transport == AppTransport::Socket && self.shared_memory.is_none(),
                "remote shared memory is unsupported"
            );
        }

        private::read(&self.credential, 32)?
            .try_into()
            .map_err(|_| anyhow!("invalid credential length"))
    }
}
pub fn create_credential(path: &Path) -> Result<()> {
    let mut token = [0; 32];
    getrandom::fill(&mut token).map_err(|_| anyhow!("credential entropy unavailable"))?;
    private::create(path, &token)
}
pub fn remaining(deadline: Instant) -> Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(!remaining.is_zero(), "connection deadline expired");
    Ok(remaining)
}
fn endpoint(value: &str) -> Result<()> {
    ensure!(value.len() <= 320, "endpoint exceeds its size limit");
    if let Ok(address) = value.parse::<SocketAddr>() {
        ensure!(
            !address.ip().is_unspecified() && address.port() != 0,
            "client endpoint requires a concrete address and port"
        );
        return Ok(());
    }
    let (host, port) = value
        .rsplit_once(':')
        .context("expected HOST:PORT endpoint")?;
    ensure!(
        !host.is_empty() && host.len() <= 253 && port.parse::<u16>().is_ok_and(|port| port != 0),
        "invalid endpoint hostname or port"
    );
    Ok(())
}
type Answer = Result<Vec<SocketAddr>, String>;
struct Resolve {
    name: String,
    reply: mpsc::SyncSender<Answer>,
    deadline: Instant,
}
static RESOLVER: OnceLock<mpsc::SyncSender<Resolve>> = OnceLock::new();
pub fn resolve(value: &str, deadline: Instant) -> Result<Vec<SocketAddr>> {
    endpoint(value)?;
    if let Ok(address) = value.parse() {
        return Ok(vec![address]);
    }
    let sender = RESOLVER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<Resolve>(4);
        std::thread::Builder::new()
            .name("shoop-dns".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    if Instant::now() >= job.deadline {
                        continue;
                    }
                    let answer = job
                        .name
                        .to_socket_addrs()
                        .map(|addresses| addresses.take(8).collect())
                        .map_err(|error| error.to_string());
                    let _ = job.reply.try_send(answer);
                }
            })
            .expect("start bounded DNS worker");
        sender
    });
    let (reply, receiver) = mpsc::sync_channel(1);
    sender
        .try_send(Resolve {
            name: value.into(),
            reply,
            deadline,
        })
        .map_err(|_| anyhow!("DNS resolver is busy"))?;
    let answer = receiver
        .recv_timeout(remaining(deadline)?)
        .context("DNS deadline expired")?
        .map_err(anyhow::Error::msg)?;
    ensure!(!answer.is_empty(), "hostname has no usable addresses");
    Ok(answer)
}
