use crate::private;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppTransport {
    #[default]
    Socket,
    SharedMemory,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostEntry {
    pub version: u16,

    pub name: String,
    pub process: u32,
    pub endpoint: SocketAddr,
    pub management: SocketAddr,
    pub credential: PathBuf,
    pub transport: AppTransport,
    pub shared_memory: Option<Vec<u8>>,
    pub warnings: Vec<String>,
}
impl HostEntry {
    pub fn read(path: &Path) -> Result<Self> {
        let entry: Self = serde_json::from_slice(&private::read(path, 64 * 1024)?)?;
        ensure!(
            entry.version == shoop_app_protocol::VERSION,
            "incompatible session protocol"
        );
        ensure!(
            entry.name.len() <= 128
                && entry.warnings.len() <= 64
                && entry.warnings.iter().all(|warning| warning.len() <= 1024),
            "invalid session registry entry"
        );
        ensure!(
            entry.endpoint.ip().is_loopback() && entry.management.ip().is_loopback(),
            "only loopback session endpoints are supported"
        );
        ensure!(
            entry
                .shared_memory
                .as_ref()
                .is_none_or(|bytes| bytes.len() == shoop_transport_shared_memory::DESCRIPTOR_BYTES),
            "invalid shared-memory descriptor"
        );
        Ok(entry)
    }
    pub fn token(&self) -> Result<[u8; 32]> {
        let bytes = private::read(&self.credential, 32)?;
        bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid credential file length"))
    }
}

pub struct Registry {
    root: PathBuf,
}
impl Registry {
    pub fn default_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("org", "ShoopDaLoop", "ShoopDaLoop egui")
            .context("no per-user session directory")?;
        Ok(dirs.data_local_dir().join("sessions"))
    }
    pub fn open(root: &Path) -> Result<Self> {
        private::directory(root)?;
        Ok(Self {
            root: root.canonicalize()?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn entry_path(&self, process: u32) -> PathBuf {
        self.root.join(format!("{process}.json"))
    }
    pub fn credential_path(&self, process: u32) -> PathBuf {
        self.root.join(format!("{process}.key"))
    }
    pub fn create_credential(&self, process: u32, token: &[u8; 32]) -> Result<PathBuf> {
        let path = self.credential_path(process);
        private::create(&path, token)?;
        Ok(path)
    }
    pub fn publish(&self, entry: &HostEntry) -> Result<PathBuf> {
        let path = self.entry_path(entry.process);
        let temp = path.with_extension("pending");
        private::create(&temp, &serde_json::to_vec(entry)?)?;
        // Never replace another process's registration.
        ensure!(!path.exists(), "session identity is already registered");
        fs::rename(&temp, &path)?;
        Ok(path)
    }
    pub fn list(&self) -> Result<Vec<(PathBuf, HostEntry)>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root)? {
            let path = item?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                entries.push((path.clone(), HostEntry::read(&path)?));
            }
        }
        entries.sort_by_key(|(_, entry)| (entry.name.clone(), entry.process));
        Ok(entries)
    }
    pub fn prune_stale(&self) -> Result<usize> {
        let mut removed = 0;
        for (_, entry) in self.list()? {
            let refused = |address| matches!(std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(100)), Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused);
            if process_gone(entry.process) && refused(entry.endpoint) && refused(entry.management) {
                self.remove(&entry)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
    pub fn remove(&self, entry: &HostEntry) -> Result<()> {
        let path = self.entry_path(entry.process);
        if path.exists() {
            let current = HostEntry::read(&path)?;
            ensure!(
                current.process == entry.process
                    && current.endpoint == entry.endpoint
                    && current.management == entry.management
                    && current.credential == entry.credential,
                "registry identity changed"
            );
            fs::remove_file(path)?;
        }
        if entry.credential == self.credential_path(entry.process) && entry.credential.exists() {
            private::check(&entry.credential, false)?;
            fs::remove_file(&entry.credential)?;
        }
        Ok(())
    }
}

pub fn identity() -> Result<u128> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("session identity entropy is unavailable"))?;
    let value = u128::from_le_bytes(bytes);
    ensure!(value != 0, "invalid session identity entropy");
    Ok(value)
}

fn process_gone(process: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(process) = i32::try_from(process) else {
            return false;
        };
        matches!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(process), None),
            Err(nix::errno::Errno::ESRCH)
        )
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {process}"), "/FO", "CSV", "/NH"])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                !String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                    line.split(',')
                        .nth(1)
                        .is_some_and(|value| value.trim_matches('"') == process.to_string())
                })
            }
            _ => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = process;
        false
    }
}
