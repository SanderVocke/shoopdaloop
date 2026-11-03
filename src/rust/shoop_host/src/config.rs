use crate::registry::AppTransport;
use anyhow::Result;
use serde::{Deserialize, Serialize};
pub use shoop_app_api::HostSettings;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostConfig {
    #[serde(flatten)]
    pub settings: HostSettings,
    pub transport: AppTransport,
    #[cfg(feature = "testing")]
    pub test_signal: bool,
}
impl core::ops::Deref for HostConfig {
    type Target = HostSettings;
    fn deref(&self) -> &Self::Target {
        &self.settings
    }
}
impl core::ops::DerefMut for HostConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.settings
    }
}
impl HostConfig {
    pub fn validate(&self) -> Result<()> {
        self.settings.validate().map_err(anyhow::Error::msg)
    }
}

#[cfg(test)]
mod tests {
    #[shoop_wasm_test_support::shoop_test]
    fn flat_native_json_uses_the_shared_host_settings_contract() {
        let config: super::HostConfig = serde_json::from_str(r#"{"driver":{"Dummy":{"sample_rate":44100,"buffer_size":128}},"pools":{"tracks":4,"loops":16,"channels":2,"loop_memory_mb":32},"transport":"socket"}"#).unwrap();
        config.validate().unwrap();
        assert_eq!(config.pools.loop_memory_mb, 32);
        assert_eq!(config.pools.buses, 8);
        assert_eq!(config.pools.bus_channels, 8);
        assert!(serde_json::from_str::<super::HostConfig>(r#"{"misspelled_setting":7}"#).is_err());
        let mut invalid = config.settings.clone();
        invalid.pools.loops = 0;
        assert!(invalid.validate().is_err());
        let decoded: super::HostConfig =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(decoded.settings, config.settings);
    }
}
