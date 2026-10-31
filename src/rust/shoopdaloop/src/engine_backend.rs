//! The new engine backend natively: the application's driver and capacity settings become
//! a native host configuration, the host renders the engine on its driver thread and the
//! adapter reaches it through the in-process carrier.

use anyhow::{anyhow, Result};
use shoop_backend::Backend;
#[cfg(feature = "cpal")]
use shoop_egui::CpalAudioDriverConfig;
use shoop_egui::{AudioDriverConfig, DummyAudioDriverConfig, NewEnginePools};
use shoop_rt_backend::{RtBackend, Sleep};
#[cfg(feature = "cpal")]
use shoop_rt_engine::MAX_HOST_PORTS;
use shoop_rt_engine::{EngineDescription, Segment};
#[cfg(feature = "cpal")]
use shoop_rt_host_native::{AudioDevice, CpalConfig, DeviceSelection, Discovery};
use shoop_rt_host_native::{
    DriverConfig, DummyConfig, NativeConfig, NativeHost, RenderPhase, StartError, Stats,
};

/// Without CPAL there is nothing to discover.
#[cfg(not(feature = "cpal"))]
#[derive(Default)]
struct Discovery;

/// The largest callback when the device chooses its buffer size. Larger callbacks are
/// rendered in parts.
#[cfg(feature = "cpal")]
const DEVICE_MAX_FRAMES: u32 = 1024;
/// The loop edge smoothing setting's largest value, 100 ms, at 384 kHz.
const SMOOTHING_LIMIT_FRAMES: u32 = 38_400;

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

    /// Stops the host and returns the problems of its stop report, which are also logged.
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
            tracing::warn!(problem = %problem, "frontend.new_engine.stop_problem");
        }
        tracing::info!(
            callbacks = report.stats.callbacks,
            problems = problems.len(),
            "frontend.new_engine.stopped"
        );
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

fn start_with(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    inventory: &Discovery,
) -> Result<NewEngine> {
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
    anyhow!(
        "the new engine backend could not start: {error}; start with --audio-backend legacy to use the legacy backend"
    )
}

/// What the mapping needs to know about the platform's CPAL hosts, devices and MIDI ports.
#[cfg(feature = "cpal")]
fn inventory(driver: &AudioDriverConfig) -> Discovery {
    let named = |selector: &str| !matches!(selector.trim(), "" | "default" | "none");
    let listed = || {
        let (midi_inputs, midi_outputs) = shoop_rt_host_native::midi_ports();
        Discovery {
            hosts: shoop_rt_host_native::cpal_hosts(),
            midi_inputs,
            midi_outputs,
            ..Discovery::default()
        }
    };
    match driver {
        AudioDriverConfig::Cpal(config)
            if named(&config.output_device) || named(&config.input_device) =>
        {
            let host = Some(config.host.as_str()).filter(|host| named(host));
            shoop_rt_host_native::discover(host).unwrap_or_else(|_| listed())
        }
        AudioDriverConfig::Cpal(_) | AudioDriverConfig::Jack(_) => listed(),
        AudioDriverConfig::Dummy(_) | AudioDriverConfig::WebAudio => Discovery::default(),
    }
}

#[cfg(not(feature = "cpal"))]
fn inventory(_driver: &AudioDriverConfig) -> Discovery {
    Discovery
}

/// The host configuration for the application's driver settings and the new engine's
/// capacities, with a warning for every setting that cannot be used as given.
fn native_config(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    inventory: &Discovery,
) -> (NativeConfig, Vec<String>) {
    let mut warnings = Vec::new();
    let (driver, max_frames) = match driver {
        AudioDriverConfig::Dummy(dummy) => {
            (DriverConfig::Dummy(dummy_config(dummy)), dummy.buffer_size)
        }
        #[cfg(feature = "cpal")]
        AudioDriverConfig::Cpal(cpal) => {
            let config = cpal_config(cpal, inventory, &mut warnings);
            let max_frames = config.buffer_frames.unwrap_or(DEVICE_MAX_FRAMES);
            (DriverConfig::Cpal(config), max_frames)
        }
        #[cfg(feature = "cpal")]
        AudioDriverConfig::Jack(jack) => {
            let jack_host = inventory.hosts.iter().any(|host| host == "jack");
            warnings.push(
                if jack_host {
                    "The new engine backend has no JACK driver; it uses CPAL's JACK host instead"
                } else {
                    "The new engine backend has no JACK driver; it uses CPAL's default devices instead"
                }
                .to_owned(),
            );
            let config = CpalConfig {
                host: jack_host.then(|| "jack".to_owned()),
                midi_inputs: midi_ports(&[], &inventory.midi_inputs, "input", &mut warnings),
                midi_outputs: midi_ports(&[], &inventory.midi_outputs, "output", &mut warnings),
                client_name: jack.client_name.clone(),
                ..CpalConfig::default()
            };
            (DriverConfig::Cpal(config), DEVICE_MAX_FRAMES)
        }
        #[cfg(not(feature = "cpal"))]
        AudioDriverConfig::Cpal(_) | AudioDriverConfig::Jack(_) => {
            let _ = inventory;
            warnings.push(
                "The new engine backend has no CPAL or JACK driver in this build; using the dummy driver"
                    .to_owned(),
            );
            let dummy = DummyAudioDriverConfig::default();
            (DriverConfig::Dummy(dummy_config(&dummy)), dummy.buffer_size)
        }
        AudioDriverConfig::WebAudio => {
            warnings.push(
                "Web Audio is unavailable in native builds; the new engine backend uses the dummy driver"
                    .to_owned(),
            );
            let dummy = DummyAudioDriverConfig::default();
            (DriverConfig::Dummy(dummy_config(&dummy)), dummy.buffer_size)
        }
    };
    let config = NativeConfig {
        driver,
        engine: engine_description(pools, max_frames),
    };
    (config, warnings)
}

fn engine_description(pools: &NewEnginePools, max_frames: u32) -> EngineDescription {
    let defaults = EngineDescription::default();
    let loop_memory = pools.loop_memory_mb as usize * (1 << 20);
    EngineDescription {
        max_frames: max_frames.max(1) as usize,
        segments: vec![Segment {
            strips: pools.tracks,
            loops: pools.loops,
        }],
        strip_channels: u8::try_from(pools.channels).unwrap_or(u8::MAX),
        smoothing_limit_frames: SMOOTHING_LIMIT_FRAMES,
        resource_pages: (loop_memory / defaults.page_bytes).max(1),
        pending_transitions: defaults.pending_transitions.max(2 * pools.loops as usize),
        ..defaults
    }
}

fn dummy_config(config: &DummyAudioDriverConfig) -> DummyConfig {
    DummyConfig {
        sample_rate: config.sample_rate,
        buffer_frames: config.buffer_size,
        input: DUMMY_INPUT,
        ..DummyConfig::default()
    }
}

/// CPAL settings, with the legacy driver's selectors: `default`, `none`, an index, an exact
/// name or part of one for devices; `all`, `none`, indices, names or parts of names for MIDI
/// ports.
#[cfg(feature = "cpal")]
fn cpal_config(
    config: &CpalAudioDriverConfig,
    inventory: &Discovery,
    warnings: &mut Vec<String>,
) -> CpalConfig {
    let host = match config.host.trim() {
        "" | "default" => None,
        wanted => {
            let host = inventory
                .hosts
                .iter()
                .find(|host| host.eq_ignore_ascii_case(wanted))
                .cloned();
            if host.is_none() {
                warnings.push(format!(
                    "The CPAL host {wanted:?} is unavailable; the new engine backend uses the default host"
                ));
            }
            host
        }
    };
    CpalConfig {
        host,
        output_device: device(
            &config.output_device,
            &inventory.outputs,
            "output",
            warnings,
        ),
        input_device: device(&config.input_device, &inventory.inputs, "input", warnings),
        sample_rate: (config.sample_rate > 0).then_some(config.sample_rate),
        buffer_frames: (config.buffer_size > 0).then_some(config.buffer_size),
        output_channels: channels(&config.output_channels, "output", warnings),
        input_channels: channels(&config.input_channels, "input", warnings),
        midi_inputs: midi_ports(
            &config.midi_inputs,
            &inventory.midi_inputs,
            "input",
            warnings,
        ),
        midi_outputs: midi_ports(
            &config.midi_outputs,
            &inventory.midi_outputs,
            "output",
            warnings,
        ),
        client_name: config.client_name.clone(),
    }
}

#[cfg(feature = "cpal")]
fn device(
    selector: &str,
    devices: &[AudioDevice],
    direction: &str,
    warnings: &mut Vec<String>,
) -> DeviceSelection {
    let wanted = selector.trim();
    match wanted {
        "" | "default" => DeviceSelection::Default,
        "none" => DeviceSelection::None,
        _ => {
            let found = wanted
                .parse::<usize>()
                .ok()
                .and_then(|index| devices.get(index))
                .or_else(|| devices.iter().find(|device| device.name == wanted))
                .or_else(|| devices.iter().find(|device| device.name.contains(wanted)));
            match found {
                Some(device) => DeviceSelection::Named(device.name.clone()),
                None => {
                    warnings.push(format!(
                        "The CPAL {direction} device {wanted:?} was not found; the new engine backend uses the default device"
                    ));
                    DeviceSelection::Default
                }
            }
        }
    }
}

#[cfg(feature = "cpal")]
fn channels(selector: &str, direction: &str, warnings: &mut Vec<String>) -> Option<u16> {
    match selector.trim() {
        "" | "all" => None,
        wanted => match wanted.parse::<u16>() {
            Ok(count) => Some(count.max(1)),
            Err(_) => {
                warnings.push(format!(
                    "{wanted:?} is not a number of CPAL {direction} channels; the new engine backend uses all of them"
                ));
                None
            }
        },
    }
}

#[cfg(feature = "cpal")]
fn midi_ports(
    selectors: &[String],
    ports: &[String],
    direction: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    if selectors.iter().any(|selector| selector == "none") {
        return Vec::new();
    }
    let matches = |selector: &String, index: usize, port: &String| {
        *selector == index.to_string() || selector == port || port.contains(selector.as_str())
    };
    let all = selectors.is_empty() || selectors.iter().any(|selector| selector == "all");
    if !all {
        for selector in selectors {
            if !ports
                .iter()
                .enumerate()
                .any(|(index, port)| matches(selector, index, port))
            {
                warnings.push(format!("No MIDI {direction} matches {selector:?}"));
            }
        }
    }
    let mut selected = ports
        .iter()
        .enumerate()
        .filter(|(index, port)| {
            all || selectors
                .iter()
                .any(|selector| matches(selector, *index, port))
        })
        .map(|(_, port)| port.clone())
        .collect::<Vec<_>>();
    if selected.len() > MAX_HOST_PORTS {
        warnings.push(format!(
            "The new engine backend opens only the first {MAX_HOST_PORTS} of {} MIDI {direction}s",
            selected.len()
        ));
        selected.truncate(MAX_HOST_PORTS);
    }
    selected
}

fn stop_problems(
    midi_reset: bool,
    phase: RenderPhase,
    cleanup: Option<String>,
    stats: &Stats,
) -> Vec<String> {
    let mut problems = Vec::new();
    if !midi_reset {
        problems.push("the engine stopped without resetting MIDI outputs".to_owned());
    }
    if phase == RenderPhase::Faulted {
        problems.push("rendering had faulted".to_owned());
    }
    if let Some(error) = cleanup {
        problems.push(format!("cleaning up the engine failed: {error}"));
    }
    if stats.xruns() > 0 {
        problems.push(format!("{} xruns", stats.xruns()));
    }
    let midi_lost = stats.midi_in_dropped
        + stats.midi_in_refused
        + stats.midi_out_dropped
        + stats.midi_out_failed;
    if midi_lost > 0 {
        problems.push(format!("{midi_lost} MIDI messages were dropped or refused"));
    }
    problems
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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
    fn an_engine_that_cannot_be_prepared_fails_with_the_override_named() {
        let driver = AudioDriverConfig::Dummy(DummyAudioDriverConfig::default());
        let invalid = NewEnginePools {
            channels: 9,
            ..pools()
        };
        let error = start_with(&driver, &invalid, &Discovery::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("--audio-backend legacy"), "{error}");
    }
}
