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
enum Host {
    Native(NativeHost),
    Process(shoop_rt_host_process::Supervisor),
}
pub(crate) struct EngineHost(Option<Host>);

impl EngineHost {
    #[cfg(test)]
    pub fn process(&self) -> &shoop_rt_host_process::Supervisor {
        match self.0.as_ref().unwrap() {
            Host::Process(process) => process,
            _ => panic!("expected process"),
        }
    }
    pub fn is_process(&self) -> bool {
        matches!(self.0, Some(Host::Process(_)))
    }

    #[cfg(test)]
    pub fn stop(mut self) -> Vec<String> {
        self.stop_host()
    }

    /// Stops the host and returns the problems of its stop report, which are also logged.
    fn stop_host(&mut self) -> Vec<String> {
        let Some(host) = self.0.take() else {
            return Vec::new();
        };
        let Host::Native(host) = host else {
            let Host::Process(supervisor) = host else {
                unreachable!()
            };
            let report = supervisor.stop();
            let mut problems = Vec::new();
            if !report.success {
                problems.push("Engine process did not exit cleanly".into());
            }
            if !report.midi_reset {
                problems.push("Engine process did not confirm MIDI reset".into());
            }
            if !report.cleaned {
                problems.push("Engine process did not confirm cleanup".into());
            }
            for problem in &problems {
                tracing::warn!(problem=%problem, "frontend.new_engine.stop_problem");
            }
            return problems;
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

pub(crate) fn start_selected(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    transport: shoop_egui::EngineTransportKind,
) -> Result<NewEngine> {
    use shoop_egui::EngineTransportKind;
    let transport = match transport {
        EngineTransportKind::InProcess => return start(driver, pools),
        EngineTransportKind::Socket => shoop_rt_host_process::Transport::Socket,
        EngineTransportKind::SharedMemory => shoop_rt_host_process::Transport::SharedMemory,
    };
    let path = shoop_rt_host_process::companion_path(&std::env::current_exe()?)?;
    let pending = shoop_rt_host_process::Pending::spawn(&path, transport).map_err(|e| {
        anyhow!(
            "could not start companion engine at {}: {e:#}",
            path.display()
        )
    })?;
    start_process(driver, pools, pending)
}

fn start_process(
    driver: &AudioDriverConfig,
    pools: &NewEnginePools,
    pending: shoop_rt_host_process::Pending,
) -> Result<NewEngine> {
    #[cfg(feature = "cpal")]
    let mut pending = pending;
    let mut warnings = Vec::new();
    #[cfg(feature = "cpal")]
    let inventory = match driver {
        AudioDriverConfig::Dummy(_) | AudioDriverConfig::WebAudio => Discovery::default(),
        _ => {
            let host = match driver {
                AudioDriverConfig::Cpal(c) if !matches!(c.host.trim(), "" | "default") => {
                    Some(c.host.as_str())
                }
                _ => None,
            };
            match pending.discover(host) {
                Ok(found) => found,
                Err(error) if host.is_some() => {
                    warnings.push(format!(
                        "Could not discover selected CPAL host: {error}; using default host"
                    ));
                    pending.discover(None)?
                }
                Err(error) => return Err(error),
            }
        }
    };
    #[cfg(not(feature = "cpal"))]
    let inventory = Discovery;
    let (mut config, mapped_warnings) = native_config(driver, pools, &inventory);
    warnings.extend(mapped_warnings);
    // Function pointers are local to the in-process dummy test host.
    match &mut config.driver {
        DriverConfig::Dummy(dummy) => dummy.input = None,
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(_) => {}
    }
    let started = pending.start(&config)?;
    warnings.extend(started.warnings);
    let host = EngineHost(Some(Host::Process(started.supervisor)));
    let backend = RtBackend::connect(started.client, Sleep::default())
        .map_err(|e| anyhow!("engine process did not synchronize: {e:#}"))?;
    Ok(NewEngine {
        backend: Box::new(backend),
        host,
        warnings,
    })
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
    let host = EngineHost(Some(Host::Native(started.host)));
    let backend = RtBackend::connect(started.client, Sleep::default())
        .map_err(|error| anyhow!("the new engine backend did not connect: {error:#}"))?;
    Ok(NewEngine {
        backend: Box::new(backend),
        host,
        warnings,
    })
}

/// Headless packaged-application smoke: uses the same launch and adapter as the UI.
pub(crate) fn probe(transport: shoop_egui::EngineTransportKind) -> Result<()> {
    use shoop_backend::{
        BackendLoopMode as Mode, BackendMidiEvent, BackendPortRole, DirectTrackRequest,
    };
    use std::time::Duration;
    let NewEngine {
        mut backend,
        mut host,
        warnings,
    } = start_selected(
        &AudioDriverConfig::Dummy(DummyAudioDriverConfig::default()),
        &NewEnginePools {
            tracks: 3,
            loops: 4,
            channels: 2,
            loop_memory_mb: 16,
        },
        transport,
    )?;
    anyhow::ensure!(
        warnings.is_empty(),
        "unexpected startup warnings: {warnings:?}"
    );
    let track = backend.create_direct_track(DirectTrackRequest {
        port_name_base: "package probe".into(),
        audio_channels: 2,
        midi: true,
        initial_loops: 1,
    })?;
    for port in &track.ports {
        let name = match port.role {
            BackendPortRole::AudioInput => "audio_in_1",
            BackendPortRole::MidiInput => "midi_in_1",
            _ => continue,
        };
        backend.set_port_connected(port.id, name, true)?;
    }
    let id = track.loops[0];
    probe_wait(&mut *backend, |s| s.loops.contains_key(&id))?;
    backend.transition_loop(id, Mode::Recording, None)?;
    probe_wait(&mut *backend, |s| s.loops[&id].mode == Mode::Recording)?;
    backend.inject_midi_input(
        track.track_id,
        &[BackendMidiEvent {
            time: 0,
            data: vec![0x90, 60, 100],
        }],
    )?;
    backend.wait_idle();
    std::thread::sleep(Duration::from_millis(40));
    backend.inject_midi_input(
        track.track_id,
        &[BackendMidiEvent {
            time: 0,
            data: vec![0x80, 60, 0],
        }],
    )?;
    backend.wait_idle();
    std::thread::sleep(Duration::from_millis(40));
    backend.transition_loop(id, Mode::Playing, None)?;
    probe_wait(&mut *backend, |s| {
        s.loops[&id].mode == Mode::Playing && s.loops[&id].length > 0
    })?;
    backend.transition_loop(id, Mode::Stopped, None)?;
    probe_wait(&mut *backend, |s| s.loops[&id].mode == Mode::Stopped)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut audio = false;
    let mut midi = false;
    while !audio || !midi {
        if !audio {
            if let Some(channels) = backend.loop_audio_data(id)? {
                audio = channels.len() == 2 && channels.iter().all(|c| !c.is_empty());
            }
        }
        if !midi {
            if let Some(data) = backend.loop_midi_data(id)? {
                midi = data.channels.iter().any(|c| c.events.len() == 2);
            }
        }
        probe_wait(&mut *backend, |_| true)?;
        anyhow::ensure!(std::time::Instant::now() < deadline, "media read timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(backend);
    let problems = host.stop_host();
    anyhow::ensure!(
        !problems.iter().any(|p| !p.contains("xruns")),
        "engine shutdown: {problems:?}"
    );
    Ok(())
}
fn probe_wait(
    backend: &mut dyn Backend,
    ready: impl Fn(&shoop_backend::BackendSnapshot) -> bool,
) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = backend.poll()?;
        anyhow::ensure!(
            snapshot.status.driver_state != shoop_backend::BackendDriverState::Failed,
            "engine failed"
        );
        anyhow::ensure!(
            snapshot.mutation_failures.is_empty(),
            "mutation failed: {:?}",
            snapshot.mutation_failures
        );
        if ready(&snapshot) {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "engine operation timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn start_failure(error: StartError) -> anyhow::Error {
    anyhow!(
        "the new engine backend could not start: {error}; start with --audio-backend legacy to use the legacy backend"
    )
}

#[cfg(test)]
use shoop_host::engine::SMOOTHING_LIMIT_FRAMES;
#[cfg(all(test, feature = "cpal"))]
use shoop_host::engine::{cpal_config, DEVICE_MAX_FRAMES};
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
    use shoop_rt_host_native::{DummyConfig, RenderPhase, Stats};

    /// The dummy driver's input in tests: a constant signal on the first input.
    pub(crate) const DUMMY_SIGNAL: f32 = 0.25;

    pub(crate) fn dummy_input(port: usize, _frame: u64) -> f32 {
        if port == 0 {
            DUMMY_SIGNAL
        } else {
            0.0
        }
    }

    #[shoop_wasm_test_support::shoop_test(no_wasm = "native engine process fixture")]
    fn process_peer() {
        if std::env::var_os("SHOOP_RT_PROCESS_ADDRESS").is_none() {
            return;
        }
        shoop_rt_host_process::run_with_start(|config| {
            let mut config = config.clone();
            match &mut config.driver {
                DriverConfig::Dummy(dummy) => dummy.input = Some(dummy_input),
                #[cfg(feature = "cpal")]
                DriverConfig::Cpal(_) => {}
            }
            shoop_rt_host_native::start_endpoint(&config)
        })
        .unwrap();
    }
    pub(crate) fn process_start(
        driver: &AudioDriverConfig,
        pools: &NewEnginePools,
        transport: shoop_egui::EngineTransportKind,
    ) -> Result<NewEngine> {
        let transport = match transport {
            shoop_egui::EngineTransportKind::Socket => shoop_rt_host_process::Transport::Socket,
            shoop_egui::EngineTransportKind::SharedMemory => {
                shoop_rt_host_process::Transport::SharedMemory
            }
            _ => anyhow::bail!("expected IPC carrier"),
        };
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command.args([
            "--exact",
            "engine_backend::tests::process_peer",
            "--nocapture",
            "--format=terse",
        ]);
        start_process(
            driver,
            pools,
            shoop_rt_host_process::Pending::spawn_command(command, transport)?,
        )
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
