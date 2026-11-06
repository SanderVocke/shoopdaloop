//! Native driver settings and prepared RT capacities, independent of any UI.
#[cfg(feature = "cpal")]
use shoop_app_api::CpalAudioDriverConfig;
use shoop_app_api::{AudioDriverConfig, DummyAudioDriverConfig, NewEnginePools};
#[cfg(feature = "cpal")]
use shoop_rt_engine::MAX_HOST_PORTS;
use shoop_rt_engine::{EngineDescription, Segment};
#[cfg(feature = "cpal")]
use shoop_rt_host_native::{AudioDevice, CpalConfig, DeviceSelection, Discovery};
use shoop_rt_host_native::{DriverConfig, DummyConfig, NativeConfig, RenderPhase, Stats};
#[cfg(not(feature = "cpal"))]
#[derive(Default)]
pub struct Discovery;
#[cfg(feature = "cpal")]
pub const DEVICE_MAX_FRAMES: u32 = 1024;
pub const SMOOTHING_LIMIT_FRAMES: u32 = 38_400;
const DUMMY_INPUT: Option<fn(usize, u64) -> f32> = None;
/// What the mapping needs to know about the platform's CPAL hosts, devices and MIDI ports.
#[cfg(feature = "cpal")]
pub fn inventory(driver: &AudioDriverConfig) -> Discovery {
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
        AudioDriverConfig::Cpal(_) => listed(),
        AudioDriverConfig::Jack(_) => Discovery::default(),
        AudioDriverConfig::Dummy(_) | AudioDriverConfig::WebAudio => Discovery::default(),
    }
}

#[cfg(not(feature = "cpal"))]
pub fn inventory(_driver: &AudioDriverConfig) -> Discovery {
    Discovery
}

/// The host configuration for the application's driver settings and the new engine's
/// capacities, with a warning for every setting that cannot be used as given.
pub fn native_config(
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
        #[cfg(feature = "jack")]
        AudioDriverConfig::Jack(jack) => {
            let config = shoop_rt_host_native::JackConfig {
                client_name: jack.client_name.clone(),
            };
            (DriverConfig::Jack(config), 1024)
        }
        #[cfg(not(feature = "jack"))]
        AudioDriverConfig::Jack(_) => {
            warnings.push(
                "Native JACK is unavailable in this build; using the dummy driver".to_owned(),
            );
            let dummy = DummyAudioDriverConfig::default();
            (DriverConfig::Dummy(dummy_config(&dummy)), dummy.buffer_size)
        }
        #[cfg(not(feature = "cpal"))]
        AudioDriverConfig::Cpal(_) => {
            let _ = inventory;
            warnings.push("CPAL is unavailable in this build; using the dummy driver".to_owned());
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

pub fn effective_driver_config(
    requested: &AudioDriverConfig,
    driver: &DriverConfig,
    sample_rate: u32,
    buffer_size: u32,
) -> AudioDriverConfig {
    #[cfg(not(feature = "cpal"))]
    let _ = requested;
    match driver {
        DriverConfig::Dummy(_) => AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate,
            buffer_size,
        }),
        #[cfg(feature = "jack")]
        DriverConfig::Jack(config) => {
            AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig {
                client_name: config.client_name.clone(),
            })
        }
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(config) => {
            let mut effective = match requested {
                AudioDriverConfig::Cpal(config) => config.clone(),
                _ => CpalAudioDriverConfig::default(),
            };
            let name = |device: &DeviceSelection| match device {
                DeviceSelection::Default => "default".into(),
                DeviceSelection::None => "none".into(),
                DeviceSelection::Named(name) => name.clone(),
            };
            effective.host = config.host.clone().unwrap_or_else(|| "default".into());
            effective.client_name = config.client_name.clone();
            effective.output_device = name(&config.output_device);
            effective.input_device = name(&config.input_device);
            effective.sample_rate = config.sample_rate.unwrap_or(0);
            effective.buffer_size = config.buffer_frames.unwrap_or(0);
            effective.output_channels = config
                .output_channels
                .map_or_else(|| "all".into(), |n| n.to_string());
            effective.input_channels = config
                .input_channels
                .map_or_else(|| "all".into(), |n| n.to_string());
            effective.midi_inputs = config.midi_inputs.clone();
            effective.midi_outputs = config.midi_outputs.clone();
            AudioDriverConfig::Cpal(effective)
        }
    }
}

pub fn engine_description(pools: &NewEnginePools, max_frames: u32) -> EngineDescription {
    let defaults = EngineDescription::default();
    let loop_memory = pools.loop_memory_mb as usize * (1 << 20);
    EngineDescription {
        external_dry_wet: true,
        buses: pools.buses as u8,
        bus_channels: pools.bus_channels as u8,
        max_frames: max_frames.max(1) as usize,
        quantum: pools.quantum as usize,
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

pub fn dummy_config(config: &DummyAudioDriverConfig) -> DummyConfig {
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
pub fn cpal_config(
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
pub fn device(
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
pub fn channels(selector: &str, direction: &str, warnings: &mut Vec<String>) -> Option<u16> {
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
pub fn midi_ports(
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

pub fn stop_problems(
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
