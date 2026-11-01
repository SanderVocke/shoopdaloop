use crate::registry::AppTransport;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use shoop_app_api::{AudioDriverConfig, NewEnginePools};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostConfig {
    pub driver: AudioDriverConfig,
    pub pools: NewEnginePools,
    pub master_auto_connect: bool,
    pub loop_smoothing_ms: u32,
    pub transport: AppTransport,
    #[cfg(feature = "testing")]
    pub test_signal: bool,
}
impl Default for HostConfig {
    fn default() -> Self {
        Self {
            driver: AudioDriverConfig::default(),
            pools: NewEnginePools::default(),
            master_auto_connect: true,
            loop_smoothing_ms: 3,
            transport: AppTransport::Socket,
            #[cfg(feature = "testing")]
            test_signal: false,
        }
    }
}
impl HostConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=64).contains(&self.pools.tracks)
                && (1..=256).contains(&self.pools.loops)
                && (1..=16).contains(&self.pools.channels)
                && (1..=1024).contains(&self.pools.loop_memory_mb),
            "session engine capacities exceed supported bounds"
        );
        ensure!(
            self.loop_smoothing_ms <= 100,
            "loop smoothing exceeds 100 ms"
        );
        if let AudioDriverConfig::Dummy(driver) = &self.driver {
            ensure!(
                (8000..=384000).contains(&driver.sample_rate)
                    && (1..=16384).contains(&driver.buffer_size),
                "invalid dummy driver configuration"
            );
        }
        Ok(())
    }
}
