use anyhow::Result;
use serde::{Deserialize, Serialize};
pub use shoop_app_api::HostSettings;

#[derive(Clone, Debug, Default, Serialize)]
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
        match self.settings.backend {
            shoop_app_api::BackendType::Native => {}
            shoop_app_api::BackendType::RustFpga => {
                anyhow::ensure!(
                    cfg!(feature = "fpga-reference"),
                    "rust-fpga backend is unavailable; rebuild with the fpga-reference feature"
                );
                anyhow::ensure!(
                    self.settings.loop_smoothing_ms == 0 && self.settings.master_auto_connect,
                    "rust-fpga currently requires zero smoothing and automatic Master connections"
                );
            }
            shoop_app_api::BackendType::RtlSim => {
                anyhow::ensure!(
                    cfg!(all(feature = "rtl-sim", target_os = "linux")),
                    "rtl-sim is unavailable; use a Linux host built with rtl-sim"
                );
                anyhow::ensure!(
                    self.settings.loop_smoothing_ms == 0 && self.settings.master_auto_connect,
                    "rtl-sim requires zero smoothing and automatic Master connections"
                );
            }
        }
        self.settings.validate().map_err(anyhow::Error::msg)
    }
}

// Host configuration is JSON. Apply backend defaults here while keeping the shared
// HostSettings serialization unchanged for the binary application protocol.
impl<'de> Deserialize<'de> for HostConfig {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> core::result::Result<Self, D::Error> {
        use serde::de::Error;
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| D::Error::custom("expected a host configuration object"))?;
        let backend = object
            .get("backend")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?
            .unwrap_or_default();
        let defaults =
            serde_json::to_value(HostSettings::for_backend(backend)).map_err(D::Error::custom)?;
        for (key, default) in defaults.as_object().unwrap() {
            if key == "pools" {
                if let Some(pools) = object
                    .get_mut(key)
                    .and_then(serde_json::Value::as_object_mut)
                {
                    for (key, value) in default.as_object().unwrap() {
                        pools.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                    continue;
                }
            }
            object.entry(key.clone()).or_insert_with(|| default.clone());
        }
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Fields {
            #[serde(flatten)]
            settings: HostSettings,
            transport: AppTransport,
            #[cfg(feature = "testing")]
            test_signal: bool,
        }
        let fields: Fields = serde_json::from_value(value).map_err(D::Error::custom)?;
        Ok(Self {
            settings: fields.settings,
            transport: fields.transport,
            #[cfg(feature = "testing")]
            test_signal: fields.test_signal,
        })
    }
}

#[cfg(test)]
mod tests {
    #[shoop_wasm_test_support::shoop_test]
    fn backend_selection_has_defaults_and_rejects_unavailable_implementations() {
        use shoop_app_api::{BackendType, HostSettings};
        let native: super::HostConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(native.settings, HostSettings::default());
        let reference: super::HostConfig =
            serde_json::from_str(r#"{"backend":"rust-fpga"}"#).unwrap();
        assert_eq!(reference.backend, BackendType::RustFpga);
        assert_eq!(reference.loop_smoothing_ms, 0);
        assert_eq!(reference.pools.loop_memory_mb, 256);
        let partial: super::HostConfig =
            serde_json::from_str(r#"{"backend":"rust-fpga","pools":{"quantum":16}}"#).unwrap();
        assert_eq!(partial.pools.quantum, 16);
        assert_eq!(partial.pools.loop_memory_mb, 256);
        assert_eq!(
            reference.validate().is_ok(),
            cfg!(feature = "fpga-reference")
        );
        let rtl: super::HostConfig = serde_json::from_str(r#"{"backend":"rtl-sim"}"#).unwrap();
        assert_eq!(
            rtl.validate().is_ok(),
            cfg!(all(feature = "rtl-sim", target_os = "linux"))
        );
        assert!(serde_json::from_str::<super::HostConfig>(r#"{"backend":"unknown"}"#).is_err());
        assert!(serde_json::from_str::<super::HostConfig>(r#"{"fpga_reference":true}"#).is_err());
        let roundtrip: super::HostConfig =
            serde_json::from_slice(&serde_json::to_vec(&reference).unwrap()).unwrap();
        assert_eq!(roundtrip.settings, reference.settings);
        let invalid: super::HostConfig =
            serde_json::from_str(r#"{"backend":"rust-fpga","loop_smoothing_ms":3}"#).unwrap();
        assert!(invalid.validate().is_err());
    }

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
            carla: Default::default(),
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
