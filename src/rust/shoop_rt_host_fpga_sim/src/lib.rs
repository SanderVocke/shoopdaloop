//! Thread-affine, unpaced FPGA RTL host. No device or application dependencies.
use shoop_rt_engine::ControlEndpoint;
use shoop_rt_transport::{carrier::InProcess, client::ProtocolClient};
use std::{ffi::OsString, path::PathBuf, time::Duration};

#[cfg(all(feature = "sim", target_os = "linux"))]
mod worker;
#[cfg(all(feature = "sim", target_os = "linux"))]
pub use worker::{Handle, RunMode, SimulationMetrics, Worker};
#[cfg(not(all(feature = "sim", target_os = "linux")))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    Continuous,
    Stepped,
}

pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(180);
#[derive(Clone, Debug)]
pub struct Config {
    pub artifacts: PathBuf,
    pub rtl_directory: PathBuf,
    pub verilator: OsString,
    pub jobs: usize,
    pub synthetic_input: bool,
    pub mode: RunMode,
    pub wave: Option<PathBuf>,
    pub capture_output: bool,
}
impl Default for Config {
    fn default() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        Self {
            artifacts: std::env::var_os("SHOOP_FPGA_CACHE")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("work/marlin-host")),
            rtl_directory: std::env::var_os("SHOOP_FPGA_RTL")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("fpga")),
            verilator: std::env::var_os("SHOOP_VERILATOR").unwrap_or_else(|| "verilator".into()),
            jobs: std::env::var("SHOOP_VERILATOR_JOBS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            synthetic_input: false,
            mode: RunMode::Continuous,
            wave: None,
            capture_output: false,
        }
    }
}
pub struct Started {
    pub host: Host,
    pub client: ProtocolClient<InProcess<ControlEndpoint>>,
    pub status: shoop_rt_engine::HostStatus,
}
pub struct Host {
    #[cfg(all(feature = "sim", target_os = "linux"))]
    worker: Worker,
}
impl Host {
    pub fn failure(&self) -> Option<String> {
        #[cfg(all(feature = "sim", target_os = "linux"))]
        {
            self.worker.failure.lock().unwrap().clone()
        }
        #[cfg(not(all(feature = "sim", target_os = "linux")))]
        {
            None
        }
    }
    pub fn stop(self) -> Result<(), String> {
        let failure = self.failure();
        drop(self);
        failure.map_or(Ok(()), Err)
    }
}
pub fn start(config: &Config) -> Result<Started, String> {
    #[cfg(all(feature = "sim", target_os = "linux"))]
    {
        let prepared = Worker::prepare_config(config.clone()).map_err(|error| error.to_string())?;
        let outbox = prepared.outbox();
        Ok(Started {
            host: Host {
                worker: prepared.execution,
            },
            client: ProtocolClient::new(InProcess::new(prepared.endpoint, outbox)),
            status: prepared.host,
        })
    }
    #[cfg(not(all(feature = "sim", target_os = "linux")))]
    {
        let _ = config;
        Err(if !cfg!(target_os = "linux") {
            "FPGA RTL simulation is unsupported on this platform; Linux is required"
        } else {
            "FPGA RTL simulation requires the fpga-sim feature"
        }
        .into())
    }
}

pub fn input(port: usize, frame: u64) -> f32 {
    let sample = ((frame.wrapping_mul(17) + port as u64 * 97) % 4096) as i32 - 2048;
    sample as f32 / 16384.
}

pub fn description() -> Result<shoop_rt_engine::EngineDescription, String> {
    #[cfg(all(feature = "sim", target_os = "linux"))]
    {
        Ok(rt_fpga_adapter::engine::profile_description())
    }
    #[cfg(not(all(feature = "sim", target_os = "linux")))]
    {
        Err(
            "FPGA RTL simulation is unsupported on this platform or requires the fpga-sim feature"
                .into(),
        )
    }
}
