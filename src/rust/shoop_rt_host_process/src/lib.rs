//! Engine-process composition and supervision, independent of application vocabulary.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]
mod convert;
mod process;
mod serve;
use anyhow::Result;
#[cfg(feature = "testing")]
pub use process::TestingClock;
pub use process::{Pending, Started, StopReport, Supervisor};
pub use serve::run;
#[cfg(feature = "testing")]
pub use serve::run_with_start;
use shoop_rt_protocol::Lane;
use shoop_rt_transport::carrier::{CarrierError, RecordPort};
use std::path::{Path, PathBuf};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transport {
    Socket,
    SharedMemory,
}
impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Socket => "socket",
            Self::SharedMemory => "shared_memory",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "socket" => Self::Socket,
            "shared_memory" => Self::SharedMemory,
            _ => anyhow::bail!("unsupported process transport"),
        })
    }
}
enum RawPort {
    Socket(shoop_rt_transport_socket::Port),
    SharedMemory(shoop_rt_transport_shared_memory::Port),
}
pub struct Port {
    inner: RawPort,
    #[cfg(feature = "testing")]
    sent: std::sync::Arc<std::sync::atomic::AtomicU64>,
    #[cfg(feature = "testing")]
    received: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl Port {
    fn new(inner: RawPort) -> Self {
        Self {
            inner,
            #[cfg(feature = "testing")]
            sent: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            #[cfg(feature = "testing")]
            received: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
    fn socket(port: shoop_rt_transport_socket::Port) -> Self {
        Self::new(RawPort::Socket(port))
    }
    fn shared(port: shoop_rt_transport_shared_memory::Port) -> Self {
        Self::new(RawPort::SharedMemory(port))
    }
}
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.post_on_lane(record, Lane::Reliable)
    }
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        let result = match &mut self.inner {
            RawPort::Socket(p) => p.post_on_lane(record, lane),
            RawPort::SharedMemory(p) => p.post_on_lane(record, lane),
        };
        #[cfg(feature = "testing")]
        if result.is_ok() {
            self.sent.fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        result
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let result = match &mut self.inner {
            RawPort::Socket(p) => p.take(buf),
            RawPort::SharedMemory(p) => p.take(buf),
        };
        #[cfg(feature = "testing")]
        if result.is_some() {
            self.received
                .fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        result
    }
    fn failure(&mut self) -> Option<CarrierError> {
        match &mut self.inner {
            RawPort::Socket(p) => p.failure(),
            RawPort::SharedMemory(p) => p.failure(),
        }
    }
}
pub type Client =
    shoop_rt_transport::client::ProtocolClient<shoop_rt_transport::records::RecordClient<Port>>;
pub fn companion_path(application: &Path) -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "shoop_rt_host_process.exe"
    } else {
        "shoop_rt_host_process"
    };
    Ok(application
        .parent()
        .ok_or_else(|| anyhow::anyhow!("application has no parent directory"))?
        .join(name))
}
