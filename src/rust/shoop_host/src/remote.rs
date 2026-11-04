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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<PathBuf>,
    #[serde(default)]
    pub network: NetworkOptions,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub transport: AppTransport,
    #[serde(default)]
    pub log_file: Option<PathBuf>,
}
impl RemoteProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == VERSION, "incompatible session protocol");
        self.validate_deployment()
    }
    /// Deployment admission survives changes to the session schema.
    pub fn validate_deployment(&self) -> Result<()> {
        ensure!(self.name.len() <= 128, "invalid profile name");
        if let Some(credential) = &self.credential {
            ensure!(
                !credential.as_os_str().is_empty()
                    && !self.local
                    && self.transport == AppTransport::Socket,
                "embedded credentials require a remote socket profile"
            );
        }
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
            self.local || self.transport == AppTransport::Socket,
            "remote shared memory is unsupported"
        );
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_profile(path, true)
    }
    pub fn read_deployment(path: &Path) -> Result<Self> {
        Self::read_profile(path, false)
    }
    fn read_profile(path: &Path, session: bool) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "remote profile exceeds 64 KiB");
        let mut profile: Self = serde_json::from_slice(&bytes)?;
        if session {
            profile.validate()?;
        } else {
            profile.validate_deployment()?;
        }
        if let Some(log) = profile.log_file.as_mut().filter(|log| log.is_relative()) {
            *log = path.parent().unwrap_or(Path::new(".")).join(&*log);
        }
        if let Some(credential) = profile
            .credential
            .as_mut()
            .filter(|path| path.is_relative())
        {
            *credential = path.parent().unwrap_or(Path::new(".")).join(&*credential);
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<PathBuf>,
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
            credential: e.credential,
            transport: e.transport,
            shared_memory: None,
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
    pub fn token(&self) -> Result<[u8; 32]> {
        ensure!(self.version == VERSION, "incompatible session protocol");
        self.deployment_token()
    }
    pub(crate) fn deployment_token(&self) -> Result<[u8; 32]> {
        self.profile().validate_deployment()?;
        let path = self
            .credential
            .as_deref()
            .context("embedded credential is required")?;
        crate::private::read(path, 32)?
            .try_into()
            .map_err(|_| anyhow!("invalid credential length"))
    }
    pub fn use_local_addresses(&mut self) {
        if let Some(endpoint) = &self.local_endpoint {
            self.endpoint = endpoint.clone();
        }
        if let Some(management) = &self.local_management {
            self.management = management.clone();
        }
    }
    pub fn read(path: &Path) -> Result<Self> {
        Ok(RemoteProfile::read(path)?.into())
    }
    pub fn validate(&self) -> Result<()> {
        self.profile().validate()?;
        ensure!(
            match self.transport {
                AppTransport::Socket => self.shared_memory.is_none(),
                AppTransport::SharedMemory => self.shared_memory.as_ref().is_some_and(|bytes| {
                    shoop_transport_shared_memory::Descriptor::decode(bytes).is_ok()
                }),
            },
            "invalid live transport descriptor"
        );
        Ok(())
    }
    pub fn profile(&self) -> RemoteProfile {
        RemoteProfile {
            version: self.version,
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
            management: self.management.clone(),
            credential: self.credential.clone(),
            network: self.network.unwrap_or_default(),
            local: self.network.is_none(),
            transport: self.transport,
            log_file: self
                .network
                .is_none()
                .then(|| self.log_file.clone())
                .flatten(),
        }
    }
}
pub fn create_credential(path: &Path) -> Result<()> {
    let mut token = [0; 32];
    getrandom::fill(&mut token).map_err(|_| anyhow!("credential entropy unavailable"))?;
    crate::private::create(path, &token)
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
