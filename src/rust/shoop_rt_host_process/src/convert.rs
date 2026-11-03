use anyhow::{bail, Result};
use shoop_rt_engine::EngineDescription;
use shoop_rt_host_native::{DriverConfig, DummyConfig, DummyMode, NativeConfig};
use shoop_rt_protocol::{host::*, DriverKind};
pub(crate) fn text(value: &str) -> Result<Text> {
    Text::new(value).ok_or_else(|| anyhow::anyhow!("host name exceeds 256 UTF-8 bytes"))
}
#[cfg(feature = "cpal")]
fn selection(value: &shoop_rt_host_native::DeviceSelection) -> Result<DeviceSelection> {
    Ok(match value {
        shoop_rt_host_native::DeviceSelection::Default => DeviceSelection::Default,
        shoop_rt_host_native::DeviceSelection::None => DeviceSelection::None,
        shoop_rt_host_native::DeviceSelection::Named(name) => DeviceSelection::Named(text(name)?),
    })
}
pub fn driver(config: &NativeConfig) -> Result<DriverSettings> {
    Ok(match &config.driver {
        #[cfg(feature = "jack")]
        DriverConfig::Jack(jack) => {
            jack.validate()?;
            DriverSettings {
                kind: DriverKind::Jack,
                host: Text::default(),
                input: DeviceSelection::None,
                output: DeviceSelection::None,
                sample_rate: 0,
                buffer_frames: 0,
                audio_inputs: jack.audio_inputs.len() as u16,
                audio_outputs: jack.audio_outputs.len() as u16,
                midi_inputs: jack.midi_inputs.len() as u16,
                midi_outputs: jack.midi_outputs.len() as u16,
                client_name: text(&jack.client_name)?,
                dummy_input_ramp: false,
            }
        }
        DriverConfig::Dummy(dummy) => {
            if dummy.mode != DummyMode::Automatic {
                bail!("controlled dummy timing is local to a test host");
            }
            if dummy.input.is_some() || dummy.wrap_cycle.is_some() {
                bail!("dummy callback functions cannot cross a process boundary")
            }
            DriverSettings {
                kind: DriverKind::Dummy,
                host: Text::default(),
                input: DeviceSelection::Default,
                output: DeviceSelection::Default,
                sample_rate: dummy.sample_rate,
                buffer_frames: dummy.buffer_frames,
                audio_inputs: dummy.audio_inputs,
                audio_outputs: dummy.audio_outputs,
                midi_inputs: dummy.midi_inputs,
                midi_outputs: dummy.midi_outputs,
                client_name: text("ShoopDaLoop")?,
                dummy_input_ramp: false,
            }
        }
        #[cfg(feature = "cpal")]
        DriverConfig::Cpal(cpal) => DriverSettings {
            kind: DriverKind::Cpal,
            host: text(cpal.host.as_deref().unwrap_or(""))?,
            input: selection(&cpal.input_device)?,
            output: selection(&cpal.output_device)?,
            sample_rate: cpal.sample_rate.unwrap_or(0),
            buffer_frames: cpal.buffer_frames.unwrap_or(0),
            audio_inputs: cpal.input_channels.unwrap_or(0),
            audio_outputs: cpal.output_channels.unwrap_or(0),
            midi_inputs: 0,
            midi_outputs: 0,
            client_name: text(&cpal.client_name)?,
            dummy_input_ramp: false,
        },
    })
}
#[cfg(feature = "cpal")]
fn native_selection(value: DeviceSelection) -> shoop_rt_host_native::DeviceSelection {
    match value {
        DeviceSelection::Default => shoop_rt_host_native::DeviceSelection::Default,
        DeviceSelection::None => shoop_rt_host_native::DeviceSelection::None,
        DeviceSelection::Named(name) => {
            shoop_rt_host_native::DeviceSelection::Named(name.as_str().into())
        }
    }
}
#[derive(Default)]
pub(crate) struct JackSetup {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub connections: Vec<(String, String)>,
}
pub fn native_driver(
    settings: DriverSettings,
    inputs: Vec<String>,
    outputs: Vec<String>,
    jack: JackSetup,
) -> Result<DriverConfig> {
    if settings.kind != DriverKind::Jack
        && (!jack.inputs.is_empty() || !jack.outputs.is_empty() || !jack.connections.is_empty())
    {
        bail!("JACK setup supplied for a different driver");
    }
    Ok(match settings.kind {
        #[cfg(feature = "jack")]
        DriverKind::Jack => {
            if [
                jack.inputs.len(),
                jack.outputs.len(),
                inputs.len(),
                outputs.len(),
            ] != [
                settings.audio_inputs as usize,
                settings.audio_outputs as usize,
                settings.midi_inputs as usize,
                settings.midi_outputs as usize,
            ] {
                bail!("JACK port counts do not match setup");
            }
            let config = shoop_rt_host_native::JackConfig {
                client_name: settings.client_name.as_str().into(),
                audio_inputs: jack.inputs,
                audio_outputs: jack.outputs,
                midi_inputs: inputs,
                midi_outputs: outputs,
                connections: jack.connections,
            };
            config.validate()?;
            DriverConfig::Jack(config)
        }
        DriverKind::Dummy => DriverConfig::Dummy(DummyConfig {
            sample_rate: settings.sample_rate,
            buffer_frames: settings.buffer_frames,
            audio_inputs: settings.audio_inputs,
            audio_outputs: settings.audio_outputs,
            midi_inputs: settings.midi_inputs,
            midi_outputs: settings.midi_outputs,
            mode: DummyMode::Automatic,
            input: settings.dummy_input_ramp.then_some(ramp),
            wrap_cycle: None,
        }),
        #[cfg(feature = "cpal")]
        DriverKind::Cpal => DriverConfig::Cpal(shoop_rt_host_native::CpalConfig {
            host: (!settings.host.as_str().is_empty()).then(|| settings.host.as_str().into()),
            input_device: native_selection(settings.input),
            output_device: native_selection(settings.output),
            sample_rate: (settings.sample_rate != 0).then_some(settings.sample_rate),
            buffer_frames: (settings.buffer_frames != 0).then_some(settings.buffer_frames),
            input_channels: (settings.audio_inputs != 0).then_some(settings.audio_inputs),
            output_channels: (settings.audio_outputs != 0).then_some(settings.audio_outputs),
            midi_inputs: inputs,
            midi_outputs: outputs,
            client_name: settings.client_name.as_str().into(),
        }),
        _ => {
            let _ = (inputs, outputs);
            bail!("this host does not support the selected driver")
        }
    })
}
fn ramp(port: usize, frame: u64) -> f32 {
    if port == 0 {
        frame as f32 / 1000.0
    } else {
        0.0
    }
}

pub fn engine(value: &EngineDescription) -> Result<EngineSettings> {
    Ok(EngineSettings {
        buses: value.buses.into(),
        bus_channels: value.bus_channels.into(),
        max_frames: value
            .max_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("max_frames exceeds wire capacity"))?,
        strip_channels: value
            .strip_channels
            .try_into()
            .map_err(|_| anyhow::anyhow!("strip_channels exceeds wire capacity"))?,
        master_channels: value
            .master_channels
            .try_into()
            .map_err(|_| anyhow::anyhow!("master_channels exceeds wire capacity"))?,
        smoothing_frames: value
            .smoothing_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("smoothing_frames exceeds wire capacity"))?,
        smoothing_limit_frames: value
            .smoothing_limit_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("smoothing_limit_frames exceeds wire capacity"))?,
        ramp_frames: value
            .ramp_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("ramp_frames exceeds wire capacity"))?,
        control_lead_frames: value
            .control_lead_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("control_lead_frames exceeds wire capacity"))?,
        resource_pages: value
            .resource_pages
            .try_into()
            .map_err(|_| anyhow::anyhow!("resource_pages exceeds wire capacity"))?,
        page_bytes: value
            .page_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("page_bytes exceeds wire capacity"))?,
        midi_events: value
            .midi_events
            .try_into()
            .map_err(|_| anyhow::anyhow!("midi_events exceeds wire capacity"))?,
        midi_bytes: value
            .midi_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("midi_bytes exceeds wire capacity"))?,
        pending_transitions: value
            .pending_transitions
            .try_into()
            .map_err(|_| anyhow::anyhow!("pending_transitions exceeds wire capacity"))?,
        reliable_credits: value
            .reliable_credits
            .try_into()
            .map_err(|_| anyhow::anyhow!("reliable_credits exceeds wire capacity"))?,
        bulk_credits: value
            .bulk_credits
            .try_into()
            .map_err(|_| anyhow::anyhow!("bulk_credits exceeds wire capacity"))?,
        telemetry_interval_frames: value
            .telemetry_interval_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("telemetry_interval_frames exceeds wire capacity"))?,
    })
}
pub fn native_engine(value: EngineSettings) -> Result<EngineDescription> {
    Ok(EngineDescription {
        buses: value
            .buses
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid buses"))?,
        bus_channels: value
            .bus_channels
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid bus_channels"))?,
        max_frames: value
            .max_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid max_frames"))?,
        strip_channels: value
            .strip_channels
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid strip_channels"))?,
        master_channels: value
            .master_channels
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid master_channels"))?,
        smoothing_frames: value
            .smoothing_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid smoothing_frames"))?,
        smoothing_limit_frames: value
            .smoothing_limit_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid smoothing_limit_frames"))?,
        ramp_frames: value
            .ramp_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid ramp_frames"))?,
        control_lead_frames: value
            .control_lead_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid control_lead_frames"))?,
        resource_pages: value
            .resource_pages
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid resource_pages"))?,
        page_bytes: value
            .page_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid page_bytes"))?,
        midi_events: value
            .midi_events
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid midi_events"))?,
        midi_bytes: value
            .midi_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid midi_bytes"))?,
        pending_transitions: value
            .pending_transitions
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid pending_transitions"))?,
        reliable_credits: value
            .reliable_credits
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid reliable_credits"))?,
        bulk_credits: value
            .bulk_credits
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid bulk_credits"))?,
        telemetry_interval_frames: value
            .telemetry_interval_frames
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid telemetry_interval_frames"))?,
        segments: Vec::new(),
        ..EngineDescription::default()
    })
}
