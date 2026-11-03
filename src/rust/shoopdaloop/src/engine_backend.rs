//! Application settings mapped to native engine hosts and their selected transport.

use anyhow::{anyhow, Result};
use shoop_backend::Backend;
#[cfg(all(test, feature = "cpal"))]
use shoop_egui::CpalAudioDriverConfig;
use shoop_egui::{AudioDriverConfig, DummyAudioDriverConfig, NewEnginePools};
use shoop_rt_backend::{RtBackend, Sleep};
#[cfg(feature = "cpal")]
use shoop_rt_host_native::Discovery;
#[cfg(all(test, feature = "cpal"))]
use shoop_rt_host_native::{AudioDevice, CpalConfig, DeviceSelection};
use shoop_rt_host_native::{DriverConfig, NativeConfig, NativeHost, StartError};

#[cfg(not(feature = "cpal"))]
use shoop_host::engine::Discovery;
#[cfg(not(test))]
const DUMMY_INPUT: Option<fn(usize, u64) -> f32> = None;
#[cfg(test)]
const DUMMY_INPUT: Option<fn(usize, u64) -> f32> = Some(tests::dummy_input);

pub(crate) struct NewEngine {
    pub backend: Box<dyn Backend + Send>,
    pub host: EngineHost,
    /// Settings that could not be used as given, and how the backend started instead.
    pub warnings: Vec<String>,
}

/// Stops the native host when dropped. Drop the backend first: the engine then sees its
/// client leave and resets MIDI before the driver stops.
pub(crate) struct EngineHost(Option<NativeHost>);
impl EngineHost {
    #[cfg(test)]
    pub fn stop(mut self) -> Vec<String> {
        self.stop_host()
    }
    fn stop_host(&mut self) -> Vec<String> {
        let Some(host) = self.0.take() else {
            return Vec::new();
        };
        let report = host.stop();
        let problems = stop_problems(
            report.midi_reset,
            report.phase,
            report.stopped.as_ref().err().map(ToString::to_string),
            &report.stats,
        );
        for problem in &problems {
            tracing::warn!(%problem, "frontend.engine.stop_problem");
        }
        problems
    }
}

impl Drop for EngineHost {
    fn drop(&mut self) {
        self.stop_host();
    }
}

/// Starts the new engine backend on the configured driver. A device driver that fails to
/// start is replaced by the dummy driver with a warning; if the engine cannot start even
/// then, this fails.
pub(crate) fn start(driver: &AudioDriverConfig, pools: &NewEnginePools) -> Result<NewEngine> {
    start_with(driver, pools, &inventory(driver))
}

pub(crate) fn start_selected(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    transport: shoop_egui::EngineTransportKind,
) -> Result<NewEngine> {
    anyhow::ensure!(transport == shoop_egui::EngineTransportKind::InProcess,
        "external sessions must be launched as independent host/UI roles or attached with --host-file");
    start(driver, pools)
}

fn start_with(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    inventory: &Discovery,
) -> Result<NewEngine> {
    shoop_egui::HostSettings {
        driver: driver.clone(),
        pools: *pools,
        ..Default::default()
    }
    .validate()
    .map_err(anyhow::Error::msg)?;
    let _span = tracing::info_span!("frontend.new_engine.start").entered();
    let (config, mut warnings) = native_config(driver, pools, inventory);
    let started = match shoop_rt_host_native::start(&config) {
        Ok(started) => started,
        Err(error) if !matches!(config.driver, DriverConfig::Dummy(_)) => {
            warnings.push(format!(
                "Could not start the CPAL driver of the new engine backend: {error}; using the dummy driver"
            ));
            let dummy = DummyAudioDriverConfig::default();
            let fallback = NativeConfig {
                driver: DriverConfig::Dummy(dummy_config(&dummy)),
                engine: engine_description(pools, dummy.buffer_size),
            };
            shoop_rt_host_native::start(&fallback).map_err(start_failure)?
        }
        Err(error) => return Err(start_failure(error)),
    };
    warnings.extend(
        started
            .host
            .warnings()
            .iter()
            .map(|warning| format!("Audio driver: {warning}")),
    );
    let host = EngineHost(Some(started.host));
    let backend = RtBackend::connect(started.client, Sleep::default())
        .map_err(|error| anyhow!("the new engine backend did not connect: {error:#}"))?;
    Ok(NewEngine {
        backend: Box::new(backend),
        host,
        warnings,
    })
}

fn start_failure(error: StartError) -> anyhow::Error {
    anyhow!("session engine could not start: {error}; check driver settings and engine capacities")
}

#[cfg(all(test, feature = "cpal"))]
use shoop_host::engine::DEVICE_MAX_FRAMES;
use shoop_host::engine::{dummy_config, engine_description, inventory, stop_problems};
fn native_config(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    inventory: &Discovery,
) -> (NativeConfig, Vec<String>) {
    let (mut config, warnings) = shoop_host::engine::native_config(driver, pools, inventory);
    match &mut config.driver {
        DriverConfig::Dummy(dummy) => dummy.input = DUMMY_INPUT,
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(_) => {}
    }
    (config, warnings)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use shoop_rt_engine::Segment;
    use shoop_rt_host_native::{RenderPhase, Stats};

    /// The dummy driver's input in tests: a constant signal on the first input.
    pub(crate) const DUMMY_SIGNAL: f32 = 0.25;

    pub(crate) fn dummy_input(port: usize, _frame: u64) -> f32 {
        if port == 0 {
            DUMMY_SIGNAL
        } else {
            0.0
        }
    }

    fn pools() -> NewEnginePools {
        NewEnginePools {
            buses: 8,
            bus_channels: 8,
            tracks: 3,
            loops: 17,
            channels: 2,
            loop_memory_mb: 16,
        }
    }

    #[cfg(feature = "cpal")]
    fn device(name: &str) -> AudioDevice {
        AudioDevice {
            name: name.to_owned(),
            default: false,
            channels: 2,
            sample_rate: 48_000,
            buffer_frames: None,
        }
    }

    #[cfg(feature = "cpal")]
    fn inventory() -> Discovery {
        Discovery {
            host: "alsa".to_owned(),
            hosts: vec!["alsa".to_owned(), "jack".to_owned()],
            outputs: vec![device("HDA Intel PCH"), device("USB Audio CODEC")],
            inputs: vec![device("HDA Intel PCH"), device("USB Audio CODEC")],
            midi_inputs: vec!["Midi Through:0".to_owned(), "Keystation:0".to_owned()],
            midi_outputs: vec!["Midi Through:0".to_owned(), "Synth:0".to_owned()],
        }
    }

    #[cfg(feature = "cpal")]
    fn cpal(config: CpalAudioDriverConfig) -> (CpalConfig, u32, Vec<String>) {
        let (native, warnings) =
            native_config(&AudioDriverConfig::Cpal(config), &pools(), &inventory());
        match native.driver {
            DriverConfig::Cpal(cpal) => (cpal, native.engine.max_frames as u32, warnings),
            DriverConfig::Dummy(_) => panic!("expected the CPAL driver"),
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn capacity_settings_become_one_prepared_segment() {
        let description = engine_description(&pools(), 128);
        assert_eq!(
            description.segments,
            [Segment {
                strips: 3,
                loops: 17
            }]
        );
        assert_eq!(description.strip_channels, 2);
        assert_eq!(description.max_frames, 128);
        assert_eq!(
            description.resource_pages * description.page_bytes,
            16 << 20
        );
        assert_eq!(description.master_channels, 2);
        assert!(description.smoothing_limit_frames >= 100 * 384);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dummy_settings_map_onto_the_dummy_driver() {
        let (native, warnings) = native_config(
            &AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 32_000,
                buffer_size: 64,
            }),
            &pools(),
            &Discovery::default(),
        );
        assert!(warnings.is_empty());
        let dummy = match native.driver {
            DriverConfig::Dummy(dummy) => dummy,
            #[cfg(feature = "cpal")]
            DriverConfig::Cpal(_) => panic!("expected the dummy driver"),
        };
        assert_eq!((dummy.sample_rate, dummy.buffer_frames), (32_000, 64));
        assert_eq!(native.engine.max_frames, 64);
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn cpal_defaults_open_default_devices_and_every_midi_port() {
        let (config, max_frames, warnings) = cpal(CpalAudioDriverConfig::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            config,
            CpalConfig {
                midi_inputs: inventory().midi_inputs,
                midi_outputs: inventory().midi_outputs,
                ..CpalConfig::default()
            }
        );
        assert_eq!(max_frames, DEVICE_MAX_FRAMES);
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn cpal_selectors_resolve_like_the_legacy_driver() {
        let (config, max_frames, warnings) = cpal(CpalAudioDriverConfig {
            client_name: "Looper".to_owned(),
            host: "ALSA".to_owned(),
            output_device: "USB".to_owned(),
            input_device: "0".to_owned(),
            sample_rate: 44_100,
            buffer_size: 128,
            output_channels: "2".to_owned(),
            input_channels: "all".to_owned(),
            midi_inputs: vec!["Keystation".to_owned()],
            midi_outputs: vec!["none".to_owned()],
            ..CpalAudioDriverConfig::default()
        });
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            config,
            CpalConfig {
                host: Some("alsa".to_owned()),
                output_device: DeviceSelection::Named("USB Audio CODEC".to_owned()),
                input_device: DeviceSelection::Named("HDA Intel PCH".to_owned()),
                sample_rate: Some(44_100),
                buffer_frames: Some(128),
                output_channels: Some(2),
                input_channels: None,
                midi_inputs: vec!["Keystation:0".to_owned()],
                midi_outputs: Vec::new(),
                client_name: "Looper".to_owned(),
            }
        );
        assert_eq!(max_frames, 128);

        let (config, _, _) = cpal(CpalAudioDriverConfig {
            input_device: "none".to_owned(),
            midi_inputs: vec!["1".to_owned()],
            ..CpalAudioDriverConfig::default()
        });
        assert_eq!(config.input_device, DeviceSelection::None);
        assert_eq!(config.midi_inputs, ["Keystation:0"]);
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn unusable_cpal_settings_fall_back_with_warnings() {
        let (config, _, warnings) = cpal(CpalAudioDriverConfig {
            host: "asio".to_owned(),
            output_device: "Missing".to_owned(),
            input_channels: "many".to_owned(),
            midi_inputs: vec!["Keystation".to_owned(), "Pads".to_owned()],
            ..CpalAudioDriverConfig::default()
        });
        assert_eq!(config.host, None);
        assert_eq!(config.output_device, DeviceSelection::Default);
        assert_eq!(config.input_channels, None);
        assert_eq!(config.midi_inputs, ["Keystation:0"]);
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        for expected in ["\"asio\"", "\"Missing\"", "\"many\"", "\"Pads\""] {
            assert!(
                warnings.iter().any(|warning| warning.contains(expected)),
                "{expected} in {warnings:?}"
            );
        }
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn jack_uses_the_nearest_cpal_host_with_a_warning() {
        let jack = AudioDriverConfig::Jack(shoop_egui::JackAudioDriverConfig {
            client_name: "Looper".to_owned(),
        });
        for (hosts, expected, wording) in [
            (
                vec!["alsa".to_owned(), "jack".to_owned()],
                Some("jack"),
                "CPAL's JACK host",
            ),
            (vec!["alsa".to_owned()], None, "CPAL's default devices"),
        ] {
            let inventory = Discovery {
                hosts,
                ..inventory()
            };
            let (native, warnings) = native_config(&jack, &pools(), &inventory);
            let DriverConfig::Cpal(config) = native.driver else {
                panic!("expected the CPAL driver");
            };
            assert_eq!(config.host.as_deref(), expected);
            assert_eq!(config.client_name, "Looper");
            assert_eq!(config.output_device, DeviceSelection::Default);
            assert_eq!(config.midi_inputs, inventory.midi_inputs);
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].contains("no JACK driver"), "{warnings:?}");
            assert!(warnings[0].contains(wording), "{warnings:?}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stop_reports_name_their_problems() {
        assert!(stop_problems(true, RenderPhase::Disconnected, None, &Stats::default()).is_empty());
        let problems = stop_problems(
            false,
            RenderPhase::Faulted,
            Some("lost".to_owned()),
            &Stats {
                late_cycles: 2,
                midi_out_dropped: 3,
                ..Stats::default()
            },
        );
        assert_eq!(problems.len(), 5, "{problems:?}");
        assert!(problems.iter().any(|problem| problem == "2 xruns"));
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn a_failing_device_driver_falls_back_to_the_dummy_driver() {
        let driver = AudioDriverConfig::Cpal(CpalAudioDriverConfig {
            output_device: "none".to_owned(),
            midi_inputs: vec!["none".to_owned()],
            midi_outputs: vec!["none".to_owned()],
            ..CpalAudioDriverConfig::default()
        });
        let started = start_with(&driver, &pools(), &Discovery::default()).unwrap();
        assert!(
            started
                .warnings
                .iter()
                .any(|warning| warning.contains("using the dummy driver")),
            "{:?}",
            started.warnings
        );
        let mut backend = started.backend;
        let snapshot = backend.poll().unwrap();
        assert_eq!(
            snapshot.status.driver_state,
            shoop_backend::BackendDriverState::Dummy
        );
        drop(backend);
        let problems = started.host.stop();
        assert!(
            !problems
                .iter()
                .any(|problem| problem.contains("MIDI") || problem.contains("fault")),
            "{problems:?}"
        );
    }

    #[cfg(not(feature = "cpal"))]
    #[shoop_wasm_test_support::shoop_test]
    fn device_drivers_use_the_dummy_driver_without_cpal() {
        for driver in [
            AudioDriverConfig::Cpal(Default::default()),
            AudioDriverConfig::Jack(Default::default()),
        ] {
            let (native, warnings) = native_config(&driver, &pools(), &Discovery);
            assert!(matches!(native.driver, DriverConfig::Dummy(_)));
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.contains("using the dummy driver")),
                "{warnings:?}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stopping_the_host_before_the_backend_is_reported() {
        let driver = AudioDriverConfig::Dummy(DummyAudioDriverConfig::default());
        let started = start_with(&driver, &pools(), &Discovery::default()).unwrap();
        let problems = started.host.stop();
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("without resetting MIDI")),
            "{problems:?}"
        );
        drop(started.backend);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_engine_that_cannot_be_prepared_reports_capacity_guidance() {
        let driver = AudioDriverConfig::Dummy(DummyAudioDriverConfig::default());
        let invalid = NewEnginePools {
            buses: 8,
            bus_channels: 8,
            channels: 9,
            ..pools()
        };
        let error = start_with(&driver, &invalid, &Discovery::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("engine capacities"), "{error}");
    }
}
