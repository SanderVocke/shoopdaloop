//! Shared startup policy for the optional simulation backend.
use shoop_app_api::{AudioBackendKind, HostSettings};
use std::time::Duration;

pub fn startup_timeout(backend: AudioBackendKind) -> Duration {
    match backend {
        AudioBackendKind::NewEngine => Duration::from_secs(12),
        AudioBackendKind::FpgaRtlSim => Duration::from_secs(180),
    }
}

pub fn validate_available(settings: &HostSettings) -> anyhow::Result<()> {
    if settings.backend == AudioBackendKind::FpgaRtlSim {
        anyhow::ensure!(
            cfg!(feature = "fpga-sim"),
            "FPGA RTL simulation requires a binary built with --features fpga-sim"
        );
        anyhow::ensure!(
            cfg!(target_os = "linux"),
            "FPGA RTL simulation is unsupported on this platform; Linux is required"
        );
    }
    Ok(())
}

#[cfg(feature = "fpga-sim")]
pub use shoop_rt_host_fpga_sim::Host;
#[cfg(feature = "fpga-sim")]
pub fn start(
    settings: &HostSettings,
) -> anyhow::Result<(
    shoop_rt_host_fpga_sim::Started,
    shoop_app_api::ResolvedAudioDriverConfig,
    HostSettings,
)> {
    use shoop_app_api::{AudioDriverConfig, SimulatedInputMode};
    settings.validate().map_err(anyhow::Error::msg)?;
    validate_available(settings)?;
    let AudioDriverConfig::Simulated(driver) = &settings.driver else {
        anyhow::bail!("FPGA RTL simulation requires a Simulated clock")
    };
    let profile = shoop_rt_host_fpga_sim::description().map_err(anyhow::Error::msg)?;
    let mut effective = settings.clone();
    effective.pools.quantum = profile.quantum as u32;
    effective.pools.tracks = profile.strips() as u32;
    effective.pools.loops = profile.loops() as u32;
    effective.pools.channels = u32::from(profile.strip_channels);
    effective.pools.buses = u32::from(profile.buses);
    effective.pools.bus_channels = u32::from(profile.bus_channels);
    effective.processors = shoop_app_api::ProcessorSettings::disabled();
    let config = shoop_rt_host_fpga_sim::Config {
        synthetic_input: driver.input == SimulatedInputMode::Synthetic,
        ..Default::default()
    };
    let started = shoop_rt_host_fpga_sim::start(&config).map_err(anyhow::Error::msg)?;
    let resolved = shoop_app_api::ResolvedAudioDriverConfig {
        configured: effective.driver.clone(),
        sample_rate: profile.sample_rate,
        buffer_size: profile.max_frames as u32,
        instance_name: "FPGA RTL simulation".into(),
    };
    Ok((started, resolved, effective))
}
