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
        self.settings.validate().map_err(anyhow::Error::msg)?;
        crate::fpga::validate_available(&self.settings)
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
        assert_eq!(config.pools.quantum, 32);
        let mut custom = config.clone();
        custom.settings.pools.quantum = 64;
        custom.settings.processors = shoop_app_api::ProcessorSettings {
            builtin_fx: shoop_app_api::ProcessorPolicy::Off,
            oxisynth: shoop_app_api::ProcessorPolicy::On,
            fx_instances: 5,
            synth_instances: 3,
            memory_mb: 384,
        };
        let decoded: super::HostConfig =
            serde_json::from_slice(&serde_json::to_vec(&custom).unwrap()).unwrap();
        assert_eq!(decoded.pools.quantum, 64);
        assert_eq!(decoded.processors, custom.processors);
        let mut invalid_width = config.clone();
        invalid_width.pools.channels = 9;
        assert!(invalid_width.validate().is_err());
        custom.settings.pools.quantum = 17;
        assert!(custom.validate().is_err());
        assert!(serde_json::from_str::<super::HostConfig>(r#"{"misspelled_setting":7}"#).is_err());
        let mut invalid = config.settings.clone();
        invalid.pools.loops = 0;
        assert!(invalid.validate().is_err());
        let decoded: super::HostConfig =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(decoded.settings, config.settings);
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppTransport {
    #[default]
    Socket,
    SharedMemory,
}

#[cfg(test)]
mod jack_tests {
    use shoop_app_api::{AudioDriverConfig, JackAudioDriverConfig};
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "native host configuration")]
    fn jack_profiles_only_accept_the_dynamic_port_format() {
        let config: super::HostConfig =
            serde_json::from_str(r#"{"driver":{"Jack":{"client_name":"Looper"}}}"#).unwrap();
        let AudioDriverConfig::Jack(mut jack) = config.settings.driver else {
            panic!("JACK profile changed kind");
        };
        assert_eq!(jack.client_name, "Looper");
        assert_eq!(
            serde_json::to_value(&jack).unwrap(),
            serde_json::json!({"client_name":"Looper"})
        );
        assert_eq!(
            serde_json::from_slice::<JackAudioDriverConfig>(&serde_json::to_vec(&jack).unwrap())
                .unwrap(),
            jack
        );
        for obsolete in [
            "audio_inputs",
            "audio_outputs",
            "midi_inputs",
            "midi_outputs",
            "connections",
        ] {
            let mut old = serde_json::json!({"driver":{"Jack":{"client_name":"Looper"}}});
            old["driver"]["Jack"][obsolete] = serde_json::json!([]);
            assert!(
                serde_json::from_value::<super::HostConfig>(old).is_err(),
                "obsolete field accepted: {obsolete}"
            );
        }
        assert!(jack.validate().is_ok());
        jack.client_name.push('\0');
        assert!(jack.validate().is_err());
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    use shoop_app_api::{AudioBackendKind, AudioDriverConfig};
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "native host startup")]
    fn backend_config_defaults_and_availability_are_explicit() {
        let native: HostConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(native.backend, AudioBackendKind::NewEngine);
        let config: HostConfig = serde_json::from_str(
            r#"{"backend":"fpga_rtl_sim","driver":{"Simulated":{"input":"synthetic"}}}"#,
        )
        .unwrap();
        assert!(matches!(config.driver, AudioDriverConfig::Simulated(_)));
        assert_eq!(
            serde_json::from_slice::<HostConfig>(&serde_json::to_vec(&config).unwrap())
                .unwrap()
                .settings,
            config.settings
        );
        if !cfg!(feature = "fpga-sim") {
            assert!(config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--features fpga-sim"));
        } else if !cfg!(target_os = "linux") {
            assert!(config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("unsupported on this platform"));
        } else {
            config.validate().unwrap();
        }
    }
}
