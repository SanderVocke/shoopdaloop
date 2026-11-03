use crate::config::AppTransport;
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
    #[serde(default)]
    pub network: NetworkOptions,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub transport: AppTransport,
    #[serde(default)]
    pub shared_memory: Option<Vec<u8>>,
    #[serde(default)]
    pub log_file: Option<PathBuf>,
}
impl RemoteProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == VERSION, "incompatible session protocol");
        ensure!(self.name.len() <= 128, "invalid profile name");
        endpoint(&self.endpoint)?;
        endpoint(&self.management)?;
        self.network.validate()?;
        if self.local {
            ensure!(
                [&self.endpoint, &self.management]
                    .iter()
                    .all(|endpoint| endpoint
                        .parse::<SocketAddr>()
                        .is_ok_and(|address| address.ip().is_loopback())),
                "local profile endpoints must be loopback addresses"
            );
        }
        ensure!(
            self.local || (self.transport == AppTransport::Socket && self.shared_memory.is_none()),
            "remote shared memory is unsupported"
        );
        ensure!(
            match self.transport {
                AppTransport::Socket => self.shared_memory.is_none(),
                AppTransport::SharedMemory => self.shared_memory.as_ref().is_some_and(|bytes| {
                    shoop_transport_shared_memory::Descriptor::decode(bytes).is_ok()
                }),
            },
            "invalid profile transport descriptor"
        );
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "remote profile exceeds 64 KiB");
        let mut profile: Self = serde_json::from_slice(&bytes)?;
        profile.validate()?;
        if let Some(log) = profile.log_file.as_mut().filter(|log| log.is_relative()) {
            *log = path.parent().unwrap_or(Path::new(".")).join(&*log);
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub can_attach: bool,
    #[serde(default)]
    pub local_endpoint: Option<String>,
    #[serde(default)]
    pub local_management: Option<String>,
    #[serde(default)]
    pub process: Option<shoop_host_processes::Identity>,
    #[serde(default)]
    pub instance: u128,
    pub version: u16,

    pub name: String,
    pub endpoint: String,
    pub management: String,
    pub transport: AppTransport,
    pub shared_memory: Option<Vec<u8>>,
    pub warnings: Vec<String>,
    pub network: Option<NetworkOptions>,
    pub log_file: Option<PathBuf>,
}
impl From<RemoteProfile> for Target {
    fn from(e: RemoteProfile) -> Self {
        Self {
            can_attach: true,
            local_endpoint: None,
            local_management: None,
            process: None,
            instance: 0,
            version: e.version,

            name: e.name,
            endpoint: e.endpoint,
            management: e.management,
            transport: e.transport,
            shared_memory: e.shared_memory,
            warnings: Vec::new(),
            network: (!e.local).then_some(e.network),
            log_file: e.log_file,
        }
    }
}
pub trait ConnectionTarget {
    fn target(&self) -> Target;
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
    pub fn read(path: &Path) -> Result<Self> {
        Ok(RemoteProfile::read(path)?.into())
    }
    pub fn validate(&self) -> Result<()> {
        let profile = self.profile();
        profile.validate()
    }
    pub fn profile(&self) -> RemoteProfile {
        RemoteProfile {
            version: self.version,
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
            management: self.management.clone(),
            network: self.network.unwrap_or_default(),
            local: self.network.is_none(),
            transport: self.transport,
            shared_memory: self.shared_memory.clone(),
            log_file: self.log_file.clone(),
        }
    }
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

pub fn identity() -> Result<u128> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| anyhow!("session identity entropy is unavailable"))?;
    let value = u128::from_le_bytes(bytes);
    ensure!(value != 0, "invalid session identity entropy");
    Ok(value)
}
