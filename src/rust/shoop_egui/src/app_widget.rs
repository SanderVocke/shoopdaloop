use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    click_track_dialog::ClickTrackDialog, colors, ephemeral_script_display_name,
    is_ephemeral_script_version, script_dialogs::ScriptDialogs, AppAction, AppState,
    AudioDriverConfig, AudioDriverKind, BusControls, ConnectionDialog, ConnectionScope,
    CpalAudioDriverConfig, DetailsPane, DummyAudioDriverConfig, GlobalControls,
    JackAudioDriverConfig, PianoPane, ProcessorLatencyAdjustmentState,
    RecordingOffsetAdjustmentState, SettingsAction, SettingsDialog, TracingStatus, TracingStopped,
    TrackLatencySpec, TrackProcessorDescriptor, TrackProcessorTypeId, TrackSpec, TrackSpecTopology,
    TrackWidget, TracksWidget,
};
use shoop_settings::{
    SettingDefinition, SettingEditor, SettingEffect, SettingKey, SettingsDraft, SettingsDraftError,
    SettingsRegistry, SettingsRegistryBuilder, SettingsRegistryError, SettingsSnapshot,
    SettingsViewState, StringToggleList,
};
use std::sync::Arc;

const LOGO_BYTES: &[u8] = include_bytes!("../../../../resources/logo-small.png");
const LOGO_AREA_HEIGHT: f32 = 112.0;
const SYNC_TRACK_HEIGHT: f32 = 118.0;
const BUS_BLOCK_HEIGHT: f32 = 58.0;
const SIDEBAR_SECTION_GAP: f32 = 8.0;

pub const DEFAULT_NEW_TRACK_MODE: SettingKey<String> = SettingKey::new("tracks.new.default_mode");
pub const DEFAULT_NEW_TRACK_AUDIO_CHANNELS: SettingKey<u32> =
    SettingKey::new("tracks.new.default_audio_channels");
pub const DEFAULT_NEW_TRACK_MIDI: SettingKey<bool> = SettingKey::new("tracks.new.default_midi");
pub const DEFAULT_NEW_TRACK_DRY_MIDI: SettingKey<bool> =
    SettingKey::new("tracks.new.default_dry_midi");
pub const DEFAULT_NEW_TRACK_PROCESSOR: SettingKey<String> =
    SettingKey::new("tracks.new.default_processor");
pub const DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT: SettingKey<String> =
    SettingKey::new("tracks.new.default_recording_adjustment");
pub const DEFAULT_NEW_TRACK_RECORDING_FRAMES: SettingKey<i32> =
    SettingKey::new("tracks.new.default_recording_frames");
pub const DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT: SettingKey<String> =
    SettingKey::new("tracks.new.default_processor_adjustment");
pub const DEFAULT_NEW_TRACK_PROCESSOR_FRAMES: SettingKey<i32> =
    SettingKey::new("tracks.new.default_processor_frames");
pub const UI_SCALE_FACTOR: SettingKey<f64> = SettingKey::new("appearance.ui_scale_factor");
pub const TOUCH_MODE: SettingKey<bool> = SettingKey::new("appearance.touch_mode");
pub const KEYBOARD_SCRIPT_ENABLED: SettingKey<bool> =
    SettingKey::new("scripting.bundled.keyboard.enabled");
pub const APC_MINI_SCRIPT_ENABLED: SettingKey<bool> =
    SettingKey::new("scripting.bundled.akai_apc_mini_mk1.enabled");
pub const BUILTINS_LOCATION: SettingKey<String> = SettingKey::new("scripting.builtins.location");
pub const BUILTIN_SCRIPTS: SettingKey<StringToggleList> =
    SettingKey::new("scripting.builtins.scripts");
pub const USER_SCRIPTS: SettingKey<StringToggleList> = SettingKey::new("scripting.user_scripts");
pub const CARLA_HOSTING_MODE: SettingKey<String> = SettingKey::new("carla.hosting_mode");

pub const LOOP_EDGE_SMOOTHING_MS: SettingKey<u32> = SettingKey::new("audio.loop_edge_smoothing_ms");
pub const SELECTED_AUDIO_DRIVER: SettingKey<String> = SettingKey::new("audio.selected_driver");
pub const DUMMY_SAMPLE_RATE: SettingKey<u32> = SettingKey::new("audio.dummy.sample_rate");
pub const DUMMY_BUFFER_SIZE: SettingKey<u32> = SettingKey::new("audio.dummy.buffer_size");
pub const JACK_CLIENT_NAME: SettingKey<String> = SettingKey::new("audio.jack.client_name");
pub const CPAL_CLIENT_NAME: SettingKey<String> = SettingKey::new("audio.cpal.client_name");
pub const CPAL_HOST: SettingKey<String> = SettingKey::new("audio.cpal.host");
pub const CPAL_OUTPUT_DEVICE: SettingKey<String> = SettingKey::new("audio.cpal.output_device");
pub const CPAL_INPUT_DEVICE: SettingKey<String> = SettingKey::new("audio.cpal.input_device");
pub const CPAL_SAMPLE_RATE: SettingKey<u32> = SettingKey::new("audio.cpal.sample_rate");
pub const CPAL_BUFFER_SIZE: SettingKey<u32> = SettingKey::new("audio.cpal.buffer_size");
pub const CPAL_OUTPUT_CHANNELS: SettingKey<String> = SettingKey::new("audio.cpal.output_channels");
pub const CPAL_INPUT_CHANNELS: SettingKey<String> = SettingKey::new("audio.cpal.input_channels");
pub const CPAL_CAPTURE_RING_FRAMES: SettingKey<u32> =
    SettingKey::new("audio.cpal.capture_ring_frames");
pub const CPAL_MIDI_INPUTS: SettingKey<String> = SettingKey::new("audio.cpal.midi_inputs");
pub const CPAL_MIDI_OUTPUTS: SettingKey<String> = SettingKey::new("audio.cpal.midi_outputs");

const TRACK_MODE_CHOICES: &[(&str, &str)] = &[
    ("regular", "Regular"),
    ("trigger", "Trigger"),
    ("dry_wet", "Dry + Wet"),
];

const LATENCY_ADJUSTMENT_CHOICES: &[(&str, &str)] = &[
    ("automatic", "Automatic"),
    ("manual", "Manual"),
    ("automatic_plus_trim", "Automatic + trim"),
];

fn validate_track_default_latency(draft: &SettingsDraft) -> Result<(), SettingsDraftError> {
    let adjustment = draft
        .get(DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT)
        .map_err(|_| {
            SettingsDraftError::MissingValue(DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT.id().to_owned())
        })?;
    let frames = draft.get(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES).map_err(|_| {
        SettingsDraftError::MissingValue(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES.id().to_owned())
    })?;
    if adjustment == "manual" && frames < 0 {
        return Err(SettingsDraftError::InvalidValue(
            DEFAULT_NEW_TRACK_PROCESSOR_FRAMES.id().to_owned(),
        ));
    }
    let mode = draft
        .get(DEFAULT_NEW_TRACK_MODE)
        .map_err(|_| SettingsDraftError::MissingValue(DEFAULT_NEW_TRACK_MODE.id().to_owned()))?;
    let processor = draft.get(DEFAULT_NEW_TRACK_PROCESSOR).map_err(|_| {
        SettingsDraftError::MissingValue(DEFAULT_NEW_TRACK_PROCESSOR.id().to_owned())
    })?;
    if mode == "dry_wet" && processor.is_empty() {
        return Err(SettingsDraftError::InvalidValue(
            DEFAULT_NEW_TRACK_PROCESSOR.id().to_owned(),
        ));
    }
    Ok(())
}

pub fn register_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    register_settings_with_ui_scale_default(builder, 1.0)
}

pub fn register_settings_with_ui_scale_default(
    builder: &mut SettingsRegistryBuilder,
    ui_scale_default: f64,
) -> Result<(), SettingsRegistryError> {
    register_settings_with_appearance_defaults(builder, ui_scale_default, false)
}

pub fn register_settings_with_appearance_defaults(
    builder: &mut SettingsRegistryBuilder,
    ui_scale_default: f64,
    touch_mode_default: bool,
) -> Result<(), SettingsRegistryError> {
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_MODE,
            "regular".to_owned(),
            "Track defaults",
            "Track type",
            "Topology used when a new Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(5)
        .effect(SettingEffect::NextUse)
        .editor(SettingEditor::StringChoice {
            choices: TRACK_MODE_CHOICES,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_AUDIO_CHANNELS,
            2,
            "Track defaults",
            "New track audio channels",
            "Audio channel count used when a new Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(10)
        .effect(SettingEffect::NextUse),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_MIDI,
            false,
            "Track defaults",
            "Enable MIDI on new tracks",
            "MIDI state used when a new Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(20)
        .effect(SettingEffect::NextUse),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_DRY_MIDI,
            false,
            "Track defaults",
            "Enable MIDI on new dry/wet tracks",
            "Dry MIDI state used when a new dry/wet Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(25)
        .effect(SettingEffect::NextUse),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_PROCESSOR,
            String::new(),
            "Track defaults",
            "Processor",
            "Processor used when a new dry/wet Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(27)
        .effect(SettingEffect::NextUse),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT,
            "manual".to_owned(),
            "Track defaults",
            "Recording alignment",
            "Automatic or manual recording alignment used for new tracks.",
        )
        .category_order(10)
        .setting_order(30)
        .effect(SettingEffect::NextUse)
        .editor(SettingEditor::StringChoice {
            choices: LATENCY_ADJUSTMENT_CHOICES,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_RECORDING_FRAMES,
            0,
            "Track defaults",
            "Recording offset or trim",
            "Manual recording alignment value used for new tracks, in frames.",
        )
        .category_order(10)
        .setting_order(40)
        .effect(SettingEffect::NextUse)
        .editor(SettingEditor::SignedInteger {
            min: -crate::MAX_TRACK_LATENCY_FRAMES,
            max: crate::MAX_TRACK_LATENCY_FRAMES,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT,
            "manual".to_owned(),
            "Track defaults",
            "Processor latency",
            "Automatic or manual processor latency compensation used for new tracks.",
        )
        .category_order(10)
        .setting_order(50)
        .effect(SettingEffect::NextUse)
        .editor(SettingEditor::StringChoice {
            choices: LATENCY_ADJUSTMENT_CHOICES,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_PROCESSOR_FRAMES,
            0,
            "Track defaults",
            "Processor latency or trim",
            "Manual processor latency value used for new tracks, in frames.",
        )
        .category_order(10)
        .setting_order(60)
        .effect(SettingEffect::NextUse)
        .editor(SettingEditor::SignedInteger {
            min: -crate::MAX_TRACK_LATENCY_FRAMES,
            max: crate::MAX_TRACK_LATENCY_FRAMES,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            UI_SCALE_FACTOR,
            ui_scale_default,
            "Appearance",
            "Pixels-per-point scale",
            "Multiplier for egui's native pixels-per-point value, scaling the entire UI.",
        )
        .category_order(2)
        .setting_order(10)
        .effect(SettingEffect::ExplicitApply)
        .editor(SettingEditor::Number {
            min: 0.75,
            max: 2.0,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            TOUCH_MODE,
            touch_mode_default,
            "Appearance",
            "Touch mode",
            "Always show direct loop controls and disable hover-only loop actions.",
        )
        .category_order(2)
        .setting_order(20)
        .effect(SettingEffect::Immediate),
    )?;
    builder.register_draft_validator(validate_track_default_latency);
    Ok(())
}

pub fn register_audio_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    let effect = SettingEffect::ExplicitApply;
    builder.register(
        SettingDefinition::new(
            LOOP_EDGE_SMOOTHING_MS,
            3,
            "Audio",
            "Loop edge smoothing",
            "Duration in milliseconds for smoothing loop playback discontinuities; 0 disables smoothing.",
        )
        .category_order(5)
        .setting_order(0)
        .effect(effect)
        .editor(SettingEditor::UnsignedInteger { min: 0, max: 100 }),
    )?;
    builder.register(
        SettingDefinition::new(
            SELECTED_AUDIO_DRIVER,
            "dummy".to_owned(),
            "Audio",
            "Preferred audio driver",
            "Driver attempted first on the next native launch. Runtime changes require Switch confirmation.",
        )
        .category_order(5)
        .setting_order(1)
        .effect(effect),
    )?;
    builder.register(
        SettingDefinition::new(
            DUMMY_SAMPLE_RATE,
            48_000,
            "Audio",
            "Dummy sample rate",
            "Offline driver sample rate in Hz.",
        )
        .category_order(5)
        .setting_order(10)
        .effect(effect)
        .editor(shoop_settings::SettingEditor::UnsignedInteger {
            min: 8_000,
            max: 384_000,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            DUMMY_BUFFER_SIZE,
            256,
            "Audio",
            "Dummy buffer size",
            "Offline driver processing buffer in frames.",
        )
        .category_order(5)
        .setting_order(11)
        .effect(effect)
        .editor(shoop_settings::SettingEditor::UnsignedInteger {
            min: 1,
            max: 65_536,
        }),
    )?;
    builder.register(
        SettingDefinition::new(
            JACK_CLIENT_NAME,
            "ShoopDaLoop".to_owned(),
            "Audio",
            "JACK client name",
            "Client-name hint used when connecting to JACK.",
        )
        .category_order(5)
        .setting_order(20)
        .effect(effect),
    )?;
    for definition in [
        SettingDefinition::new(
            CPAL_CLIENT_NAME,
            "ShoopDaLoop".to_owned(),
            "Audio",
            "CPAL client name",
            "Name used for CPAL and MIDI clients.",
        )
        .category_order(5)
        .setting_order(30)
        .effect(effect),
        SettingDefinition::new(
            CPAL_HOST,
            "default".to_owned(),
            "Audio",
            "CPAL host",
            "Host/API selector; available values are shown from current discovery.",
        )
        .category_order(5)
        .setting_order(31)
        .effect(effect),
        SettingDefinition::new(
            CPAL_OUTPUT_DEVICE,
            "default".to_owned(),
            "Audio",
            "CPAL output device",
            "Output device name, default, or none.",
        )
        .category_order(5)
        .setting_order(32)
        .effect(effect),
        SettingDefinition::new(
            CPAL_INPUT_DEVICE,
            "default".to_owned(),
            "Audio",
            "CPAL input device",
            "Input device name, default, or none.",
        )
        .category_order(5)
        .setting_order(33)
        .effect(effect),
        SettingDefinition::new(
            CPAL_OUTPUT_CHANNELS,
            "all".to_owned(),
            "Audio",
            "CPAL output channels",
            "Number of output channels to use, or all.",
        )
        .category_order(5)
        .setting_order(36)
        .effect(effect),
        SettingDefinition::new(
            CPAL_INPUT_CHANNELS,
            "all".to_owned(),
            "Audio",
            "CPAL input channels",
            "Number of input channels to use, or all.",
        )
        .category_order(5)
        .setting_order(37)
        .effect(effect),
        SettingDefinition::new(
            CPAL_MIDI_INPUTS,
            "all".to_owned(),
            "Audio",
            "MIDI inputs",
            "Comma-separated midir input names/selectors.",
        )
        .category_order(5)
        .setting_order(39)
        .effect(effect),
        SettingDefinition::new(
            CPAL_MIDI_OUTPUTS,
            "all".to_owned(),
            "Audio",
            "MIDI outputs",
            "Comma-separated midir output names/selectors.",
        )
        .category_order(5)
        .setting_order(40)
        .effect(effect),
    ] {
        builder.register(definition)?;
    }
    for definition in [
        SettingDefinition::new(
            CPAL_SAMPLE_RATE,
            0,
            "Audio",
            "CPAL sample rate",
            "Requested sample rate in Hz, or 0 for the device default.",
        )
        .category_order(5)
        .setting_order(34)
        .effect(effect)
        .editor(shoop_settings::SettingEditor::UnsignedInteger {
            min: 0,
            max: 384_000,
        }),
        SettingDefinition::new(
            CPAL_BUFFER_SIZE,
            0,
            "Audio",
            "CPAL buffer size",
            "Requested buffer size in frames, or 0 for the device default.",
        )
        .category_order(5)
        .setting_order(35)
        .effect(effect)
        .editor(shoop_settings::SettingEditor::UnsignedInteger {
            min: 0,
            max: 65_536,
        }),
        SettingDefinition::new(
            CPAL_CAPTURE_RING_FRAMES,
            4096,
            "Audio",
            "CPAL capture ring",
            "Always-on capture ring size in frames.",
        )
        .category_order(5)
        .setting_order(38)
        .effect(effect)
        .editor(shoop_settings::SettingEditor::UnsignedInteger {
            min: 1,
            max: 16_777_216,
        }),
    ] {
        builder.register(definition)?;
    }
    Ok(())
}

pub fn register_carla_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    builder.register(
        SettingDefinition::new(
            CARLA_HOSTING_MODE,
            shoop_settings::CarlaHostingMode::InProcess.as_str().to_owned(),
            "Carla",
            "Hosting mode",
            "Run Carla in this process or in a supervised subprocess. This is a global machine setting, is not stored in sessions, and takes effect after restart.",
        )
        .category_order(7)
        .setting_order(10)
        .effect(SettingEffect::RestartRequired)
        .editor(shoop_settings::SettingEditor::StringChoice {
            choices: &[
                ("in_process", "In application process"),
                ("subprocess", "One subprocess per FX chain"),
            ],
        }),
    )
}

pub fn carla_hosting_mode_from_snapshot(
    snapshot: &SettingsSnapshot,
) -> Result<shoop_settings::CarlaHostingMode, String> {
    snapshot
        .get(CARLA_HOSTING_MODE)
        .map_err(|error| error.to_string())?
        .parse()
        .map_err(|error: shoop_settings::CarlaHostingModeParseError| error.to_string())
}

pub fn loop_edge_smoothing_ms(snapshot: &SettingsSnapshot) -> Result<u32, String> {
    snapshot
        .get(LOOP_EDGE_SMOOTHING_MS)
        .map_err(|error| error.to_string())
}

pub fn selected_audio_driver(snapshot: &SettingsSnapshot) -> Result<AudioDriverKind, String> {
    parse_audio_driver_kind(
        snapshot
            .get(SELECTED_AUDIO_DRIVER)
            .map_err(|error| error.to_string())?,
    )
}

pub fn audio_driver_config_from_snapshot(
    snapshot: &SettingsSnapshot,
    kind: AudioDriverKind,
) -> Result<AudioDriverConfig, String> {
    audio_driver_config(
        kind,
        |key| snapshot.get(key).map_err(|error| error.to_string()),
        |key| snapshot.get(key).map_err(|error| error.to_string()),
    )
}

pub fn audio_driver_config_from_draft(
    draft: &SettingsDraft,
    kind: AudioDriverKind,
) -> Result<AudioDriverConfig, String> {
    audio_driver_config(
        kind,
        |key| draft.get(key).map_err(|error| error.to_string()),
        |key| draft.get(key).map_err(|error| error.to_string()),
    )
}

pub fn set_selected_audio_driver(draft: &mut SettingsDraft, kind: AudioDriverKind) {
    draft.set(SELECTED_AUDIO_DRIVER, kind.id().to_owned());
}

fn audio_driver_config(
    kind: AudioDriverKind,
    mut string: impl FnMut(SettingKey<String>) -> Result<String, String>,
    mut unsigned: impl FnMut(SettingKey<u32>) -> Result<u32, String>,
) -> Result<AudioDriverConfig, String> {
    match kind {
        AudioDriverKind::Dummy => Ok(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: unsigned(DUMMY_SAMPLE_RATE)?,
            buffer_size: unsigned(DUMMY_BUFFER_SIZE)?,
        })),
        AudioDriverKind::Jack => {
            let client_name = string(JACK_CLIENT_NAME)?;
            if client_name.trim().is_empty() {
                return Err("JACK client name must not be empty".to_owned());
            }
            Ok(AudioDriverConfig::Jack(JackAudioDriverConfig {
                client_name,
            }))
        }
        AudioDriverKind::Cpal => {
            let config = CpalAudioDriverConfig {
                client_name: string(CPAL_CLIENT_NAME)?,
                host: string(CPAL_HOST)?,
                output_device: string(CPAL_OUTPUT_DEVICE)?,
                input_device: string(CPAL_INPUT_DEVICE)?,
                sample_rate: unsigned(CPAL_SAMPLE_RATE)?,
                buffer_size: unsigned(CPAL_BUFFER_SIZE)?,
                output_channels: string(CPAL_OUTPUT_CHANNELS)?,
                input_channels: string(CPAL_INPUT_CHANNELS)?,
                capture_ring_frames: unsigned(CPAL_CAPTURE_RING_FRAMES)?,
                midi_inputs: split_selectors(&string(CPAL_MIDI_INPUTS)?),
                midi_outputs: split_selectors(&string(CPAL_MIDI_OUTPUTS)?),
            };
            if config.client_name.trim().is_empty()
                || config.host.trim().is_empty()
                || config.output_device.trim().is_empty()
                || config.input_device.trim().is_empty()
                || config.output_channels.trim().is_empty()
                || config.input_channels.trim().is_empty()
                || config.capture_ring_frames == 0
            {
                return Err("CPAL selectors and capture ring must be non-empty".to_owned());
            }
            Ok(AudioDriverConfig::Cpal(config))
        }
        AudioDriverKind::WebAudio => Ok(AudioDriverConfig::WebAudio),
    }
}

fn parse_audio_driver_kind(value: String) -> Result<AudioDriverKind, String> {
    match value.as_str() {
        "dummy" => Ok(AudioDriverKind::Dummy),
        "jack" => Ok(AudioDriverKind::Jack),
        "cpal" => Ok(AudioDriverKind::Cpal),
        "webaudio" => Ok(AudioDriverKind::WebAudio),
        _ => Err(format!("unknown audio driver {value:?}")),
    }
}

fn split_selectors(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn register_bundled_script_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    builder.register(
        SettingDefinition::new(
            BUILTINS_LOCATION,
            default_builtins_location(),
            "Scripts",
            "Built-in scripts location",
            "Directory containing the distributable built-in Lua scripts and their resources.",
        )
        .category_order(20)
        .setting_order(10)
        .effect(SettingEffect::Immediate),
    )?;
    builder.register(
        SettingDefinition::new(
            BUILTIN_SCRIPTS,
            StringToggleList::default(),
            "Scripts",
            "Enabled built-in scripts",
            "Discovered built-in identities and whether they run at startup.",
        )
        .category_order(20)
        .setting_order(20)
        .effect(SettingEffect::Immediate),
    )?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
const SOURCE_TREE_MARKER: &str = "SHOOP_SRC_TREE";
#[cfg(not(target_arch = "wasm32"))]
const MAX_SOURCE_TREE_MARKER_BYTES: u64 = 4096;

#[cfg(not(target_arch = "wasm32"))]
fn packaged_builtins_location(executable: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let path = executable
        .parent()
        .and_then(std::path::Path::parent)
        .map(|contents| contents.join("Resources/builtins"));
    #[cfg(not(target_os = "macos"))]
    let path = executable
        .parent()
        .map(|directory| directory.join("builtins"));
    path.unwrap_or_else(|| "builtins".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn marked_source_builtins_location(executable: &std::path::Path) -> Option<std::path::PathBuf> {
    let executable_directory = executable.parent()?;
    if executable_directory.as_os_str().is_empty() {
        return None;
    }
    let marker = executable_directory.join(SOURCE_TREE_MARKER);
    if std::fs::metadata(&marker).ok()?.len() > MAX_SOURCE_TREE_MARKER_BYTES {
        return None;
    }
    let source_root = std::fs::read_to_string(marker).ok()?;
    let source_root = std::path::Path::new(source_root.trim());
    if source_root.as_os_str().is_empty() || source_root.is_absolute() {
        return None;
    }
    Some(
        executable_directory
            .join(source_root)
            .join("resources/builtins"),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn builtins_location_for_executable(executable: &std::path::Path) -> std::path::PathBuf {
    marked_source_builtins_location(executable)
        .unwrap_or_else(|| packaged_builtins_location(executable))
}

pub fn default_builtins_location() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        "builtins".to_owned()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = match std::env::current_exe() {
            Ok(executable) => builtins_location_for_executable(&executable),
            Err(_) => "builtins".into(),
        };
        path.to_string_lossy().into_owned()
    }
}

pub fn register_script_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    register_bundled_script_settings(builder)?;
    builder.register(
        SettingDefinition::new(
            USER_SCRIPTS,
            StringToggleList::default(),
            "Scripts",
            "User Lua scripts",
            "Lua source files known to this machine and whether they run at startup.",
        )
        .category_order(20)
        .setting_order(30)
        .effect(SettingEffect::Immediate),
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AddTrackMode {
    #[default]
    Regular,
    Trigger,
    DryWet,
}

fn track_mode_value(value: AddTrackMode) -> &'static str {
    match value {
        AddTrackMode::Regular => "regular",
        AddTrackMode::Trigger => "trigger",
        AddTrackMode::DryWet => "dry_wet",
    }
}

fn track_mode_from_value(value: &str) -> Option<AddTrackMode> {
    match value {
        "regular" => Some(AddTrackMode::Regular),
        "trigger" => Some(AddTrackMode::Trigger),
        "dry_wet" => Some(AddTrackMode::DryWet),
        _ => None,
    }
}

fn recording_adjustment_value(value: RecordingOffsetAdjustmentState) -> &'static str {
    match value {
        RecordingOffsetAdjustmentState::Automatic => "automatic",
        RecordingOffsetAdjustmentState::ManualOverride => "manual",
        RecordingOffsetAdjustmentState::AutomaticPlusTrim => "automatic_plus_trim",
    }
}

fn recording_adjustment_from_value(value: &str) -> Option<RecordingOffsetAdjustmentState> {
    match value {
        "automatic" => Some(RecordingOffsetAdjustmentState::Automatic),
        "manual" => Some(RecordingOffsetAdjustmentState::ManualOverride),
        "automatic_plus_trim" => Some(RecordingOffsetAdjustmentState::AutomaticPlusTrim),
        _ => None,
    }
}

fn processor_adjustment_value(value: ProcessorLatencyAdjustmentState) -> &'static str {
    match value {
        ProcessorLatencyAdjustmentState::Automatic => "automatic",
        ProcessorLatencyAdjustmentState::ManualOverride => "manual",
        ProcessorLatencyAdjustmentState::AutomaticPlusTrim => "automatic_plus_trim",
    }
}

fn processor_adjustment_from_value(value: &str) -> Option<ProcessorLatencyAdjustmentState> {
    match value {
        "automatic" => Some(ProcessorLatencyAdjustmentState::Automatic),
        "manual" => Some(ProcessorLatencyAdjustmentState::ManualOverride),
        "automatic_plus_trim" => Some(ProcessorLatencyAdjustmentState::AutomaticPlusTrim),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewTrackConfiguration {
    pub mode: AddTrackMode,
    pub audio_channels: u32,
    pub midi: bool,
    pub dry_midi: bool,
    pub processor: Option<TrackProcessorTypeId>,
    pub recording_adjustment: RecordingOffsetAdjustmentState,
    pub recording_frames: i32,
    pub processor_adjustment: ProcessorLatencyAdjustmentState,
    pub processor_frames: i32,
}

impl NewTrackConfiguration {
    pub(crate) fn from_settings_draft(draft: &SettingsDraft) -> Option<Self> {
        Some(Self {
            mode: track_mode_from_value(&draft.get(DEFAULT_NEW_TRACK_MODE).ok()?)?,
            audio_channels: draft.get(DEFAULT_NEW_TRACK_AUDIO_CHANNELS).ok()?,
            midi: draft.get(DEFAULT_NEW_TRACK_MIDI).ok()?,
            dry_midi: draft.get(DEFAULT_NEW_TRACK_DRY_MIDI).ok()?,
            processor: match draft.get(DEFAULT_NEW_TRACK_PROCESSOR).ok()? {
                value if value.is_empty() => None,
                value => Some(TrackProcessorTypeId::new(value)),
            },
            recording_adjustment: recording_adjustment_from_value(
                &draft.get(DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT).ok()?,
            )?,
            recording_frames: draft.get(DEFAULT_NEW_TRACK_RECORDING_FRAMES).ok()?,
            processor_adjustment: processor_adjustment_from_value(
                &draft.get(DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT).ok()?,
            )?,
            processor_frames: draft.get(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES).ok()?,
        })
    }

    pub(crate) fn write_to_settings_draft(&self, draft: &mut SettingsDraft) {
        draft.set(
            DEFAULT_NEW_TRACK_MODE,
            track_mode_value(self.mode).to_owned(),
        );
        draft.set(DEFAULT_NEW_TRACK_AUDIO_CHANNELS, self.audio_channels);
        draft.set(DEFAULT_NEW_TRACK_MIDI, self.midi);
        draft.set(DEFAULT_NEW_TRACK_DRY_MIDI, self.dry_midi);
        draft.set(
            DEFAULT_NEW_TRACK_PROCESSOR,
            self.processor
                .as_ref()
                .map_or_else(String::new, |processor| processor.as_str().to_owned()),
        );
        draft.set(
            DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT,
            recording_adjustment_value(self.recording_adjustment).to_owned(),
        );
        draft.set(DEFAULT_NEW_TRACK_RECORDING_FRAMES, self.recording_frames);
        draft.set(
            DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT,
            processor_adjustment_value(self.processor_adjustment).to_owned(),
        );
        draft.set(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES, self.processor_frames);
    }
}

fn recording_adjustment_label(value: RecordingOffsetAdjustmentState) -> &'static str {
    match value {
        RecordingOffsetAdjustmentState::Automatic => "Automatic",
        RecordingOffsetAdjustmentState::ManualOverride => "Manual",
        RecordingOffsetAdjustmentState::AutomaticPlusTrim => "Automatic + trim",
    }
}

fn processor_adjustment_label(value: ProcessorLatencyAdjustmentState) -> &'static str {
    match value {
        ProcessorLatencyAdjustmentState::Automatic => "Automatic",
        ProcessorLatencyAdjustmentState::ManualOverride => "Manual",
        ProcessorLatencyAdjustmentState::AutomaticPlusTrim => "Automatic + trim",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BottomPane {
    Details,
    Piano,
}

pub struct AppWidgetResponse {
    pub app_actions: Vec<AppAction>,
    pub settings_actions: Vec<SettingsAction>,
    pub about_requested: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackDefaultSaveResult {
    Accepted,
    Retry,
    Failed,
}

#[derive(Clone)]
struct PendingEphemeralScript {
    name: String,
    source: Arc<str>,
    source_path: Option<String>,
}

pub struct AppWidget {
    tracks: TracksWidget,
    global_controls: GlobalControls,
    details: DetailsPane,
    piano: PianoPane,
    sync_track: TrackWidget,
    bus_controls: BTreeMap<crate::BusId, BusControls>,
    connections: ConnectionDialog,
    click_track: ClickTrackDialog,
    settings: SettingsDialog,
    script_dialogs: ScriptDialogs,
    bottom_pane: Option<BottomPane>,
    add_track_open: bool,
    add_track_name: String,
    add_track_mode: AddTrackMode,
    add_track_audio_channels: u32,
    add_track_midi: bool,
    add_track_dry_midi: bool,
    add_track_processor: Option<TrackProcessorTypeId>,
    add_track_recording_adjustment: RecordingOffsetAdjustmentState,
    add_track_recording_frames: i32,
    add_track_processor_adjustment: ProcessorLatencyAdjustmentState,
    add_track_processor_frames: i32,
    add_track_make_default: bool,
    next_add_track_request_id: u64,
    pending_track_defaults: BTreeMap<u64, SettingsDraft>,
    confirmed_track_defaults: BTreeSet<u64>,
    accepted_track_defaults: BTreeMap<u64, SettingsDraft>,
    logo: Option<egui::TextureHandle>,
    io_channel_mappings: BTreeMap<crate::TaskId, Vec<u32>>,
    io_channel_selections: BTreeMap<crate::TaskId, Vec<u32>>,
    pressed_script_keys: BTreeMap<egui::Key, (i64, i64)>,
    script_control_pressed: bool,
    pending_ephemeral_scripts: VecDeque<PendingEphemeralScript>,
    new_session_confirmation_open: bool,
    tracing_status: TracingStatus,
    tracing_stopped: Option<TracingStopped>,
    last_callback_count: u64,
    callbacks_active_until: f64,
    #[cfg(test)]
    tracing_save_rect: Option<egui::Rect>,
    #[cfg(test)]
    tracing_discard_rect: Option<egui::Rect>,
    #[cfg(test)]
    ephemeral_script_accept_rect: Option<egui::Rect>,
    #[cfg(test)]
    ephemeral_script_cancel_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_accept_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_cancel_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_midi_id: Option<egui::Id>,
    #[cfg(test)]
    add_track_recording_frames_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_processor_frames_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_make_default_rect: Option<egui::Rect>,
    #[cfg(test)]
    details_toggle_rect: Option<egui::Rect>,
    #[cfg(test)]
    piano_toggle_rect: Option<egui::Rect>,
    #[cfg(test)]
    xrun_menu_rect: Option<egui::Rect>,
    #[cfg(test)]
    reset_xruns_rect: Option<egui::Rect>,
    #[cfg(test)]
    bus_area_rect: Option<egui::Rect>,
    #[cfg(test)]
    logo_area_rect: Option<egui::Rect>,
    #[cfg(test)]
    sync_area_rect: Option<egui::Rect>,
}

impl Default for AppWidget {
    fn default() -> Self {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).expect("built-in settings must be valid");
        Self::new(Arc::new(builder.finish()))
    }
}

impl AppWidget {
    pub fn new(settings_registry: Arc<SettingsRegistry>) -> Self {
        let mut sync_track = TrackWidget::default();
        sync_track.set_width_resizable(false);
        Self {
            tracks: TracksWidget::default(),
            global_controls: GlobalControls::default(),
            details: DetailsPane::default(),
            piano: PianoPane::default(),
            sync_track,
            bus_controls: BTreeMap::new(),
            connections: ConnectionDialog::default(),
            click_track: ClickTrackDialog::default(),
            settings: SettingsDialog::new(settings_registry),
            script_dialogs: ScriptDialogs::default(),
            bottom_pane: None,
            add_track_open: false,
            add_track_name: String::new(),
            add_track_mode: AddTrackMode::Regular,
            add_track_audio_channels: 2,
            add_track_midi: false,
            add_track_dry_midi: false,
            add_track_processor: None,
            add_track_recording_adjustment: RecordingOffsetAdjustmentState::default(),
            add_track_recording_frames: 0,
            add_track_processor_adjustment: ProcessorLatencyAdjustmentState::default(),
            add_track_processor_frames: 0,
            add_track_make_default: false,
            next_add_track_request_id: 1,
            pending_track_defaults: BTreeMap::new(),
            confirmed_track_defaults: BTreeSet::new(),
            accepted_track_defaults: BTreeMap::new(),
            logo: None,
            io_channel_mappings: BTreeMap::new(),
            io_channel_selections: BTreeMap::new(),
            pressed_script_keys: BTreeMap::new(),
            script_control_pressed: false,
            pending_ephemeral_scripts: VecDeque::new(),
            new_session_confirmation_open: false,
            tracing_status: TracingStatus::default(),
            tracing_stopped: None,
            last_callback_count: 0,
            callbacks_active_until: 0.0,
            #[cfg(test)]
            tracing_save_rect: None,
            #[cfg(test)]
            tracing_discard_rect: None,
            #[cfg(test)]
            ephemeral_script_accept_rect: None,
            #[cfg(test)]
            ephemeral_script_cancel_rect: None,
            #[cfg(test)]
            add_track_accept_rect: None,
            #[cfg(test)]
            add_track_cancel_rect: None,
            #[cfg(test)]
            add_track_midi_id: None,
            #[cfg(test)]
            add_track_recording_frames_rect: None,
            #[cfg(test)]
            add_track_processor_frames_rect: None,
            #[cfg(test)]
            add_track_make_default_rect: None,
            #[cfg(test)]
            details_toggle_rect: None,
            #[cfg(test)]
            piano_toggle_rect: None,
            #[cfg(test)]
            xrun_menu_rect: None,
            #[cfg(test)]
            reset_xruns_rect: None,
            #[cfg(test)]
            bus_area_rect: None,
            #[cfg(test)]
            logo_area_rect: None,
            #[cfg(test)]
            sync_area_rect: None,
        }
    }

    pub fn open_connections(&mut self, scope: ConnectionScope) {
        self.connections.open(scope);
    }

    fn set_bottom_pane(&mut self, next: Option<BottomPane>, actions: &mut Vec<AppAction>) {
        if self.bottom_pane == Some(BottomPane::Piano) && next != self.bottom_pane {
            if let Some(action) = self.piano.release_all() {
                actions.push(AppAction::Piano(action));
            }
        }
        self.bottom_pane = next;
    }

    pub fn set_click_track_preview_available(&mut self, available: bool) {
        self.click_track.set_preview_available(available);
    }

    pub fn set_tracing_status(&mut self, status: TracingStatus) {
        self.tracing_status = status;
        self.settings.set_tracing_status(status);
    }

    pub fn notify_tracing_stopped(&mut self, stopped: TracingStopped) {
        self.tracing_stopped = Some(stopped);
    }

    pub fn add_user_script_path(&mut self, path: String) -> Result<(), &'static str> {
        self.settings.add_user_script_path(path)
    }

    pub fn queue_ephemeral_script(&mut self, name: String, source: Arc<str>) {
        self.queue_ephemeral_script_from_path(name, source, None);
    }

    pub fn queue_ephemeral_script_from_path(
        &mut self,
        name: String,
        source: Arc<str>,
        source_path: Option<String>,
    ) {
        self.pending_ephemeral_scripts
            .push_back(PendingEphemeralScript {
                name,
                source,
                source_path,
            });
    }

    pub fn open_connection_scope(&self) -> Option<ConnectionScope> {
        self.connections.is_open().then(|| self.connections.scope())
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &AppState,
        settings_state: &SettingsViewState,
        script_paths: Option<&BTreeMap<crate::ScriptId, String>>,
    ) -> AppWidgetResponse {
        let _span = tracing::trace_span!(
            "frontend.egui.frame",
            revision = state.revision,
            track_count = state.tracks.len()
        )
        .entered();
        self.ensure_logo(ui.ctx());
        let (events, modifiers) = ui
            .ctx()
            .input(|input| (input.events.clone(), input.modifiers));
        let text_entry_active = ui.ctx().egui_wants_keyboard_input();
        let mut actions = crate::key_input::translate_events(
            &events,
            modifiers,
            text_entry_active,
            &mut self.pressed_script_keys,
            &mut self.script_control_pressed,
        )
        .into_iter()
        .map(AppAction::KeyEvent)
        .collect::<Vec<_>>();
        let mut settings_actions = Vec::new();
        self.reconcile_accepted_track_defaults(settings_state);
        self.resolve_track_default_saves(state, settings_state, &mut settings_actions);
        let mut about_requested = false;
        let touch_mode = settings_state.active.get(TOUCH_MODE).unwrap_or(false);
        crate::loop_widget::set_touch_mode(ui.ctx(), touch_mode);

        egui::Panel::top("global_controls")
            .frame(
                egui::Frame::new()
                    .fill(colors::DARK_BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("global_controls_scroll")
                    .scroll_source(crate::control_safe_scroll_source())
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            actions.extend(
                                self.global_controls
                                    .show(ui, &state.global_controls)
                                    .into_iter()
                                    .map(AppAction::Global),
                            );
                            if self.global_controls.take_connections_requested() {
                                self.connections.open(ConnectionScope::AllTracks);
                            }
                            if self.global_controls.take_new_session_requested() {
                                self.new_session_confirmation_open = true;
                            }
                            if self.global_controls.take_save_session_requested() {
                                actions.push(AppAction::RequestSaveSession);
                            }
                            if self.global_controls.take_load_session_requested() {
                                actions.push(AppAction::RequestLoadSessionPicker);
                            }
                            if self.global_controls.take_load_session_url_requested() {
                                actions.push(AppAction::RequestLoadSessionUrl);
                            }
                            if self.global_controls.take_settings_requested() {
                                let defaults = self.effective_track_defaults(settings_state);
                                self.settings.open(settings_state);
                                self.settings.apply_track_defaults(&defaults);
                            }
                            about_requested |= self.global_controls.take_about_requested();
                            ui.separator();
                            self.script_dialogs
                                .show_control(ui, &state.scripting.dialogs);
                        });
                    });
            });

        egui::Panel::bottom("bottom_bar")
            .resizable(false)
            .show_separator_line(false)
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(colors::DARK_BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(6, 1)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (pane, label) in [
                        (BottomPane::Details, "details"),
                        (BottomPane::Piano, "piano"),
                    ] {
                        let response = ui.selectable_label(self.bottom_pane == Some(pane), label);
                        #[cfg(test)]
                        match pane {
                            BottomPane::Details => self.details_toggle_rect = Some(response.rect),
                            BottomPane::Piano => self.piano_toggle_rect = Some(response.rect),
                        }
                        if response.clicked() {
                            let next = (self.bottom_pane != Some(pane)).then_some(pane);
                            self.set_bottom_pane(next, &mut actions);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.show_bottom_status(ui, state, &mut actions, &mut settings_actions)
                    });
                });
            });

        egui::Panel::right("logo_and_sync")
            .resizable(false)
            .show_separator_line(false)
            .exact_size(150.0)
            .frame(
                egui::Frame::new()
                    .fill(colors::SIDEBAR_BACKGROUND)
                    .inner_margin(egui::Margin::same(5)),
            )
            .show(ui, |ui| {
                self.bus_controls
                    .retain(|bus_id, _| state.buses.iter().any(|bus| bus.id == *bus_id));
                let sidebar = ui.max_rect();
                let logo_rect = egui::Rect::from_min_size(
                    egui::pos2(sidebar.left(), sidebar.bottom() - LOGO_AREA_HEIGHT),
                    egui::vec2(sidebar.width(), LOGO_AREA_HEIGHT),
                );
                #[cfg(test)]
                {
                    self.logo_area_rect = Some(logo_rect);
                }
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt("logo_area")
                        .max_rect(logo_rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                    |ui| self.show_logo(ui, state),
                );

                let mut content_top = sidebar.top();
                if let Some(sync) = state.tracks.iter().find(|track| track.is_sync) {
                    let sync_height = SYNC_TRACK_HEIGHT
                        .min((logo_rect.top() - SIDEBAR_SECTION_GAP - content_top).max(0.0));
                    let sync_rect = egui::Rect::from_min_size(
                        egui::pos2(sidebar.left(), content_top),
                        egui::vec2(sidebar.width(), sync_height),
                    );
                    #[cfg(test)]
                    {
                        self.sync_area_rect = Some(sync_rect);
                    }
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .id_salt("sync_track_area")
                            .max_rect(sync_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| self.show_sync_track(ui, sync, state, &mut actions),
                    );
                    content_top = sync_rect.bottom() + SIDEBAR_SECTION_GAP;
                } else {
                    #[cfg(test)]
                    {
                        self.sync_area_rect = None;
                    }
                }

                let bus_bottom = logo_rect.top() - SIDEBAR_SECTION_GAP;
                let available_height = (bus_bottom - content_top).max(0.0);
                let desired_height = (state.buses.len() as f32 * BUS_BLOCK_HEIGHT
                    + state.buses.len().saturating_sub(1) as f32 * SIDEBAR_SECTION_GAP)
                    .min(available_height);
                if desired_height > 0.0 {
                    let bus_rect = egui::Rect::from_min_size(
                        egui::pos2(sidebar.left(), bus_bottom - desired_height),
                        egui::vec2(sidebar.width(), desired_height),
                    );
                    #[cfg(test)]
                    {
                        self.bus_area_rect = Some(bus_rect);
                    }
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .id_salt("bus_control_area")
                            .max_rect(bus_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("bus_control_scroll")
                                .scroll_source(crate::control_safe_scroll_source())
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    for bus in state.buses.iter() {
                                        let controls = self.bus_controls.entry(bus.id).or_default();
                                        actions.extend(controls.show(ui, bus).into_iter().map(
                                            |action| AppAction::Bus {
                                                bus_id: bus.id,
                                                action,
                                            },
                                        ));
                                        ui.add_space(SIDEBAR_SECTION_GAP);
                                    }
                                });
                        },
                    );
                } else {
                    #[cfg(test)]
                    {
                        self.bus_area_rect = None;
                    }
                }
            });

        match self.bottom_pane {
            Some(BottomPane::Details) => {
                egui::Panel::bottom("details")
                    .resizable(true)
                    .default_size(200.0)
                    .min_size(70.0)
                    .max_size(400.0)
                    .frame(
                        egui::Frame::new()
                            .fill(colors::RAISED_BACKGROUND)
                            .inner_margin(egui::Margin::same(6)),
                    )
                    .show(ui, |ui| {
                        actions.extend(self.details.show(ui, state.details.as_ref()));
                    });
            }
            Some(BottomPane::Piano) => {
                let destination_ids = piano_destinations(state);
                let destination_centers = self.tracks.track_centers(&destination_ids);
                egui::Panel::bottom("piano")
                    .resizable(true)
                    .default_size(165.0)
                    .min_size(145.0)
                    .max_size(260.0)
                    .frame(
                        egui::Frame::new()
                            .fill(colors::RAISED_BACKGROUND)
                            .inner_margin(egui::Margin::same(6)),
                    )
                    .show(ui, |ui| {
                        actions.extend(
                            self.piano
                                .show(ui, !destination_ids.is_empty(), &destination_centers)
                                .into_iter()
                                .map(AppAction::Piano),
                        );
                    });
            }
            None => {}
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors::DARK_BACKGROUND)
                    .inner_margin(egui::Margin {
                        left: 8,
                        right: 8,
                        top: 8,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                let main_tracks: Vec<_> = state
                    .tracks
                    .iter()
                    .filter(|track| !track.is_sync)
                    .cloned()
                    .collect();
                let response = self.tracks.show_with_global_controls(
                    ui,
                    &main_tracks,
                    &state.track_processors,
                    &state.global_controls,
                );
                if response.add_track_requested {
                    let defaults = self.effective_track_defaults(settings_state);
                    self.open_add_track_dialog(main_tracks.len(), &defaults);
                }
                if let Some(track_id) = response.connection_track_requested {
                    self.connections.open(ConnectionScope::Track(track_id));
                }
                if let Some(loop_id) = response.click_track_requested {
                    if let Some(loop_state) = main_tracks
                        .iter()
                        .flat_map(|track| &track.loops)
                        .find(|loop_| loop_.id == loop_id)
                    {
                        self.click_track.open(loop_state, &state.click_track);
                    }
                }
                actions.extend(response.intents);
            });

        self.show_add_track_dialog(
            ui.ctx(),
            &state.track_processors,
            state,
            settings_state,
            &mut actions,
        );
        actions.extend(self.click_track.show(ui.ctx(), state));
        self.show_io_task_dialog(ui.ctx(), state, &mut actions);
        actions.extend(self.connections.show(ui.ctx(), state));
        actions.extend(self.script_dialogs.show_windows(
            ui.ctx(),
            &state.scripting.dialogs,
            script_paths,
        ));
        let settings_response = self.settings.show(
            ui.ctx(),
            settings_state,
            &state.scripting,
            &state.audio_drivers,
            &state.track_processors,
            script_paths,
        );
        actions.extend(settings_response.app_actions);
        settings_actions.extend(settings_response.settings_actions);
        self.show_ephemeral_script_confirmation(ui.ctx(), state, &mut actions);
        self.show_new_session_confirmation(ui.ctx(), &mut actions);
        self.show_tracing_stopped(ui.ctx());
        if !actions.is_empty() || !settings_actions.is_empty() {
            tracing::debug!(
                target: "Frontend.Egui",
                app_action_count = actions.len(),
                settings_action_count = settings_actions.len(),
                revision = state.revision,
                "frontend.egui.action_batch"
            );
            for action in &actions {
                tracing::trace!(
                    target: "Frontend.Egui",
                    intent = action.kind(),
                    revision = state.revision,
                    "frontend.egui.intent_created"
                );
            }
        }
        AppWidgetResponse {
            app_actions: actions,
            settings_actions,
            about_requested,
        }
    }

    fn show_ephemeral_script_confirmation(
        &mut self,
        context: &egui::Context,
        state: &AppState,
        actions: &mut Vec<AppAction>,
    ) {
        let Some(pending) = self.pending_ephemeral_scripts.front().cloned() else {
            return;
        };
        let matching = state
            .scripting
            .scripts
            .iter()
            .filter(|script| is_ephemeral_script_version(&script.name, &pending.name))
            .collect::<Vec<_>>();
        let display_name = ephemeral_script_display_name(
            &pending.name,
            state
                .scripting
                .scripts
                .iter()
                .map(|script| script.name.as_str()),
        );
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new("Run Lua script?")
            .id(egui::Id::new("ephemeral_script_confirmation"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Load and run {:?}?", pending.name));
                ui.label(
                    "Run-once scripts stay in memory for restart, are independent of the session, and disappear when the app closes.",
                );
                if !matching.is_empty() {
                    ui.add_space(4.0);
                    ui.colored_label(
                        colors::WARNING,
                        "A same-named script is already listed. Loading this version will stop the current same-named script.",
                    );
                    ui.label(format!("The new version will appear as {:?}.", display_name));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let run = ui.button("Run once");
                    #[cfg(test)]
                    {
                        self.ephemeral_script_accept_rect = Some(run.rect);
                    }
                    if run.clicked() {
                        accept = true;
                    }
                    let cancel_button = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        self.ephemeral_script_cancel_rect = Some(cancel_button.rect);
                    }
                    if cancel_button.clicked() {
                        cancel = true;
                    }
                });
            });
        if accept {
            actions.push(self.accept_ephemeral_script().unwrap());
        } else if cancel {
            self.pending_ephemeral_scripts.pop_front();
        }
    }

    fn show_new_session_confirmation(
        &mut self,
        context: &egui::Context,
        actions: &mut Vec<AppAction>,
    ) {
        if !self.new_session_confirmation_open {
            return;
        }
        let mut accept = false;
        let mut cancel = false;
        let modal =
            egui::Modal::new(egui::Id::new("new_session_confirmation")).show(context, |ui| {
                ui.heading("Create a new session?");
                ui.label("All tracks and unsaved session data will be discarded.");
                ui.add_space(6.0);
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    },
                    |ui| {
                        if ui.button("New session").clicked() {
                            accept = true;
                        }
                    },
                );
            });
        if accept {
            actions.push(AppAction::RequestNewSession);
        }
        if accept || cancel || modal.should_close() {
            self.new_session_confirmation_open = false;
        }
    }

    fn accept_ephemeral_script(&mut self) -> Option<AppAction> {
        self.pending_ephemeral_scripts
            .pop_front()
            .map(|pending| AppAction::AddEphemeralScript {
                name: pending.name,
                source: pending.source,
                source_path: pending.source_path,
            })
    }

    fn open_add_track_dialog(&mut self, main_track_count: usize, defaults: &SettingsDraft) {
        self.add_track_name = format!("Track {}", main_track_count + 1);
        let configuration = NewTrackConfiguration::from_settings_draft(defaults)
            .expect("registered new-track settings must retain valid types and choices");
        self.add_track_mode = configuration.mode;
        self.add_track_audio_channels = configuration.audio_channels;
        self.add_track_midi = configuration.midi;
        self.add_track_dry_midi = configuration.dry_midi;
        self.add_track_processor = configuration.processor;
        self.add_track_recording_adjustment = configuration.recording_adjustment;
        self.add_track_recording_frames = configuration.recording_frames;
        self.add_track_processor_adjustment = configuration.processor_adjustment;
        self.add_track_processor_frames = configuration.processor_frames;
        self.add_track_make_default = false;
        self.add_track_open = true;
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_settings_test_open_add_track(
        &mut self,
        settings_state: &SettingsViewState,
    ) -> (u32, bool) {
        let defaults = self.effective_track_defaults(settings_state);
        self.open_add_track_dialog(0, &defaults);
        (self.add_track_audio_channels, self.add_track_midi)
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_builtin_synth_form(
        &mut self,
        processors: &[TrackProcessorDescriptor],
    ) -> bool {
        let Some(processor) = processors
            .iter()
            .find(|processor| processor.id.as_str() == TrackProcessorTypeId::OXISYNTH)
        else {
            return false;
        };
        self.add_track_name = "Browser Built-in Synth capability check".to_owned();
        self.add_track_mode = AddTrackMode::DryWet;
        self.add_track_open = true;
        self.add_track_processor = Some(processor.id.clone());
        self.add_track_audio_channels = 2;
        self.add_track_dry_midi = true;
        self.add_track_open
            && processor.label == "Built-in Synth"
            && processor.constraints.min_dry_audio_channels == Some(2)
            && processor.constraints.max_dry_audio_channels == Some(2)
            && processor.constraints.midi == crate::TrackProcessorMidiPolicy::Required
            && self.add_track_spec().is_some()
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_close_add_track(&mut self) {
        self.cancel_add_track();
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_settings_test_open_scripts(
        &mut self,
        settings_state: &SettingsViewState,
    ) -> bool {
        self.settings
            .browser_test_open_category(settings_state, "Scripts")
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_lua_dialog_state(
        &self,
        id: crate::ScriptDialogId,
    ) -> Option<(bool, usize)> {
        self.script_dialogs.browser_test_state(id)
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_close_lua_dialog(&mut self, id: crate::ScriptDialogId) {
        self.script_dialogs.browser_test_close(id);
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_lua_dialog_from_list(&mut self, id: crate::ScriptDialogId) {
        self.script_dialogs.browser_test_open_from_list(id);
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_set_lua_dialog_page(&mut self, id: crate::ScriptDialogId, page: usize) {
        self.script_dialogs.browser_test_set_page(id, page);
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_lua_dialog_count(&self) -> usize {
        self.script_dialogs.browser_test_count()
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_global_connections(&mut self) {
        self.connections.open(ConnectionScope::AllTracks);
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_click_track(
        &mut self,
        state: &AppState,
        loop_id: crate::LoopId,
    ) -> bool {
        let Some(loop_state) = state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == loop_id)
        else {
            return false;
        };
        self.click_track.open(loop_state, &state.click_track);
        true
    }

    fn show_io_task_dialog(
        &mut self,
        context: &egui::Context,
        state: &AppState,
        actions: &mut Vec<AppAction>,
    ) {
        let Some(task) = &state.io_task else {
            return;
        };
        if matches!(
            task.status,
            crate::IoTaskStatus::Completed | crate::IoTaskStatus::Cancelled
        ) {
            return;
        }
        egui::Window::new("Session and loop I/O")
            .id(egui::Id::new(("io_task", task.id.raw())))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(&task.message);
                ui.add(egui::ProgressBar::new(task.progress).show_percentage());
                if let Some(mapping) = &task.audio_channel_mapping {
                    let draft = self
                        .io_channel_mappings
                        .entry(task.id)
                        .or_insert_with(|| mapping.default_mapping.clone());
                    if draft.len() != mapping.destination_channels.len() {
                        *draft = mapping.default_mapping.clone();
                    }
                    egui::Grid::new(("io_channel_mapping", task.id.raw()))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (index, destination) in
                                mapping.destination_channels.iter().enumerate()
                            {
                                ui.label(destination);
                                let selected = draft
                                    .get(index)
                                    .and_then(|source| {
                                        mapping.source_channels.get(*source as usize)
                                    })
                                    .cloned()
                                    .unwrap_or_else(|| "Unavailable".to_owned());
                                egui::ComboBox::from_id_salt((
                                    "io_channel_source",
                                    task.id.raw(),
                                    index,
                                ))
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (source, label) in
                                        mapping.source_channels.iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut draft[index],
                                            source as u32,
                                            label,
                                        );
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Import with this mapping").clicked() {
                            actions.push(AppAction::ConfirmAudioChannelMapping {
                                task_id: task.id,
                                source_for_destination: draft.clone(),
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::CancelIoTask { task_id: task.id });
                        }
                    });
                } else if let Some(selection) = &task.audio_channel_selection {
                    let draft = self
                        .io_channel_selections
                        .entry(task.id)
                        .or_insert_with(|| selection.default_selection.clone());
                    draft
                        .retain(|channel| (*channel as usize) < selection.available_channels.len());
                    let mut remove = None;
                    let mut move_up = None;
                    let mut move_down = None;
                    for index in 0..draft.len() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Output {}", index + 1));
                            let selected = selection
                                .available_channels
                                .get(draft[index] as usize)
                                .cloned()
                                .unwrap_or_else(|| "Unavailable".to_owned());
                            egui::ComboBox::from_id_salt((
                                "io_export_channel",
                                task.id.raw(),
                                index,
                            ))
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (channel, label) in
                                    selection.available_channels.iter().enumerate()
                                {
                                    if !draft.iter().enumerate().any(|(other, value)| {
                                        other != index && *value == channel as u32
                                    }) {
                                        ui.selectable_value(
                                            &mut draft[index],
                                            channel as u32,
                                            label,
                                        );
                                    }
                                }
                            });
                            if ui.small_button("↑").clicked() && index > 0 {
                                move_up = Some(index);
                            }
                            if ui.small_button("↓").clicked() && index + 1 < draft.len() {
                                move_down = Some(index);
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        draft.remove(index);
                    } else if let Some(index) = move_up {
                        draft.swap(index, index - 1);
                    } else if let Some(index) = move_down {
                        draft.swap(index, index + 1);
                    }
                    if let Some(channel) = (0..selection.available_channels.len() as u32)
                        .find(|channel| !draft.contains(channel))
                    {
                        if ui.button("Add channel").clicked() {
                            draft.push(channel);
                        }
                    }
                    ui.horizontal(|ui| {
                        let confirm =
                            ui.add_enabled(!draft.is_empty(), egui::Button::new("Export channels"));
                        if confirm.clicked() {
                            actions.push(audio_channel_selection_action(task.id, draft));
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::CancelIoTask { task_id: task.id });
                        }
                    });
                } else if let Some(warning) = &task.sample_rate_warning {
                    ui.colored_label(
                        colors::WARNING,
                        format!(
                            "This will resample {} from {} Hz to {} Hz.",
                            warning.affected_media, warning.source_rate, warning.target_rate
                        ),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Resample and load").clicked() {
                            actions.push(AppAction::ConfirmSampleRateConversion {
                                task_id: task.id,
                                accept: true,
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::ConfirmSampleRateConversion {
                                task_id: task.id,
                                accept: false,
                            });
                        }
                    });
                } else if task.status == crate::IoTaskStatus::Running
                    && ui.button("Cancel").clicked()
                {
                    actions.push(AppAction::CancelIoTask { task_id: task.id });
                }
                if task.status == crate::IoTaskStatus::Failed {
                    ui.colored_label(
                        colors::STRONG_ERROR,
                        "The operation failed; the prior session is unchanged.",
                    );
                }
            });
    }

    fn show_add_track_dialog(
        &mut self,
        context: &egui::Context,
        processors: &[TrackProcessorDescriptor],
        state: &AppState,
        settings_state: &SettingsViewState,
        actions: &mut Vec<AppAction>,
    ) {
        if !self.add_track_open {
            return;
        }
        let mut open = self.add_track_open;
        let mut accepted = false;
        let mut cancelled = false;
        egui::Window::new("Add track")
            .id(egui::Id::new("add_track_dialog"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                egui::Grid::new("add_track_name")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.add_track_name);
                        ui.end_row();
                    });
                let mut configuration = NewTrackConfiguration {
                    mode: self.add_track_mode,
                    audio_channels: self.add_track_audio_channels,
                    midi: self.add_track_midi,
                    dry_midi: self.add_track_dry_midi,
                    processor: self.add_track_processor.clone(),
                    recording_adjustment: self.add_track_recording_adjustment,
                    recording_frames: self.add_track_recording_frames,
                    processor_adjustment: self.add_track_processor_adjustment,
                    processor_frames: self.add_track_processor_frames,
                };
                let _configuration_ui = show_new_track_configuration(
                    ui,
                    "add_track_configuration",
                    &mut configuration,
                    processors,
                );
                self.add_track_mode = configuration.mode;
                self.add_track_audio_channels = configuration.audio_channels;
                self.add_track_midi = configuration.midi;
                self.add_track_dry_midi = configuration.dry_midi;
                self.add_track_processor = configuration.processor;
                self.add_track_recording_adjustment = configuration.recording_adjustment;
                self.add_track_recording_frames = configuration.recording_frames;
                self.add_track_processor_adjustment = configuration.processor_adjustment;
                self.add_track_processor_frames = configuration.processor_frames;
                #[cfg(test)]
                {
                    self.add_track_midi_id = _configuration_ui.midi_id;
                    self.add_track_recording_frames_rect = _configuration_ui.recording_frames_rect;
                    self.add_track_processor_frames_rect = _configuration_ui.processor_frames_rect;
                }
                let _make_default = ui.checkbox(&mut self.add_track_make_default, "make default");
                #[cfg(test)]
                {
                    self.add_track_make_default_rect = Some(_make_default.rect);
                }
                let spec = self.add_track_spec();
                let validation = spec
                    .as_ref()
                    .ok_or(crate::TrackSpecError::ProcessorUnavailable)
                    .and_then(|spec| spec.validate(processors));
                if let Err(error) = validation {
                    let message = match error {
                        crate::TrackSpecError::EmptyName => "A track name is required.",
                        crate::TrackSpecError::InvalidLatency => {
                            "Latency values are outside the supported range."
                        }
                        crate::TrackSpecError::ProcessorUnavailable => {
                            "No compatible dry/wet processor is available on this runtime."
                        }
                        crate::TrackSpecError::UnsupportedShape => {
                            "The selected processor does not support this channel/MIDI shape."
                        }
                    };
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let add = ui.add_enabled(
                        validation.is_ok(),
                        egui::Button::new("Add").min_size(egui::vec2(64.0, 28.0)),
                    );
                    #[cfg(test)]
                    {
                        self.add_track_accept_rect = Some(add.rect);
                    }
                    if add.clicked() {
                        accepted = true;
                    }
                    let cancel =
                        ui.add(egui::Button::new("Cancel").min_size(egui::vec2(72.0, 28.0)));
                    #[cfg(test)]
                    {
                        self.add_track_cancel_rect = Some(cancel.rect);
                    }
                    if cancel.clicked() {
                        cancelled = true;
                    }
                });
            });
        if accepted {
            if let Some(action) =
                self.accept_add_track_with_defaults(processors, state, settings_state)
            {
                actions.push(action);
                open = false;
            }
        }
        if cancelled {
            self.cancel_add_track();
            open = false;
        }
        self.add_track_open = open;
    }

    fn add_track_spec(&self) -> Option<TrackSpec> {
        let topology = match self.add_track_mode {
            AddTrackMode::Regular => TrackSpecTopology::Direct {
                audio_channels: self.add_track_audio_channels,
                midi: self.add_track_midi,
            },
            AddTrackMode::Trigger => TrackSpecTopology::Direct {
                audio_channels: 0,
                midi: false,
            },
            AddTrackMode::DryWet => TrackSpecTopology::DryWet {
                dry_audio_channels: self.add_track_audio_channels,
                wet_audio_channels: self.add_track_audio_channels,
                dry_midi: self.add_track_dry_midi,
                processor_type: self.add_track_processor.clone()?,
            },
        };
        Some(TrackSpec {
            name: self.add_track_name.trim().to_owned(),
            topology,
            latency: TrackLatencySpec {
                adjustment: self.add_track_recording_adjustment,
                manual_frames: self.add_track_recording_frames,
                processor_adjustment: self.add_track_processor_adjustment,
                processor_manual_frames: self.add_track_processor_frames,
            },
            creation_request_id: None,
        })
    }

    fn add_track_defaults_draft(&self, settings_state: &SettingsViewState) -> SettingsDraft {
        let mut draft = SettingsDraft::from_snapshot(&settings_state.active);
        NewTrackConfiguration {
            mode: self.add_track_mode,
            audio_channels: self.add_track_audio_channels,
            midi: self.add_track_midi,
            dry_midi: self.add_track_dry_midi,
            processor: self.add_track_processor.clone(),
            recording_adjustment: self.add_track_recording_adjustment,
            recording_frames: self.add_track_recording_frames,
            processor_adjustment: self.add_track_processor_adjustment,
            processor_frames: self.add_track_processor_frames,
        }
        .write_to_settings_draft(&mut draft);
        draft
    }

    #[cfg(test)]
    fn accept_add_track(&mut self, processors: &[TrackProcessorDescriptor]) -> Option<AppAction> {
        let spec = self.add_track_spec()?;
        spec.validate(processors).ok()?;
        self.add_track_open = false;
        Some(AppAction::AddTrackWithTopology(spec))
    }

    fn accept_add_track_with_defaults(
        &mut self,
        processors: &[TrackProcessorDescriptor],
        state: &AppState,
        settings_state: &SettingsViewState,
    ) -> Option<AppAction> {
        let mut spec = self.add_track_spec()?;
        spec.validate(processors).ok()?;
        if self.add_track_make_default {
            while state
                .track_creation_results
                .iter()
                .any(|result| result.request_id == self.next_add_track_request_id)
                || self
                    .pending_track_defaults
                    .contains_key(&self.next_add_track_request_id)
            {
                self.next_add_track_request_id = self.next_add_track_request_id.wrapping_add(1);
            }
            let request_id = self.next_add_track_request_id;
            self.next_add_track_request_id = self.next_add_track_request_id.wrapping_add(1);
            spec.creation_request_id = Some(request_id);
            self.pending_track_defaults
                .insert(request_id, self.add_track_defaults_draft(settings_state));
        }
        self.add_track_open = false;
        Some(AppAction::AddTrackWithTopology(spec))
    }

    fn rebased_track_defaults(
        draft: &SettingsDraft,
        settings_state: &SettingsViewState,
    ) -> SettingsDraft {
        let configuration = NewTrackConfiguration::from_settings_draft(draft)
            .expect("pending new-track defaults must remain complete and valid");
        let mut rebased = SettingsDraft::from_snapshot(&settings_state.active);
        configuration.write_to_settings_draft(&mut rebased);
        rebased
    }

    fn effective_track_defaults(&self, settings_state: &SettingsViewState) -> SettingsDraft {
        let mut selected = self
            .accepted_track_defaults
            .iter()
            .next_back()
            .map(|(request_id, draft)| (*request_id, draft));
        for request_id in self.confirmed_track_defaults.iter().copied() {
            let Some(draft) = self.pending_track_defaults.get(&request_id) else {
                continue;
            };
            if selected.is_none_or(|(selected_id, _)| request_id > selected_id) {
                selected = Some((request_id, draft));
            }
        }
        selected.map_or_else(
            || SettingsDraft::from_snapshot(&settings_state.active),
            |(_, draft)| Self::rebased_track_defaults(draft, settings_state),
        )
    }

    fn reconcile_accepted_track_defaults(&mut self, settings_state: &SettingsViewState) {
        self.accepted_track_defaults
            .retain(|_, draft| draft.base_revision() == settings_state.active.revision());
    }

    fn resolve_track_default_saves(
        &mut self,
        state: &AppState,
        settings_state: &SettingsViewState,
        settings_actions: &mut Vec<SettingsAction>,
    ) {
        for result in state.track_creation_results.iter() {
            if !self.pending_track_defaults.contains_key(&result.request_id) {
                continue;
            }
            if result.success {
                self.confirmed_track_defaults.insert(result.request_id);
            } else {
                self.pending_track_defaults.remove(&result.request_id);
                self.confirmed_track_defaults.remove(&result.request_id);
            }
        }
        if settings_state.persistence == shoop_settings::SettingsPersistenceState::Saving
            || settings_state.recovery_required
        {
            return;
        }
        for request_id in self.confirmed_track_defaults.iter().copied() {
            let Some(draft) = self.pending_track_defaults.get(&request_id) else {
                continue;
            };
            let rebased = Self::rebased_track_defaults(draft, settings_state);
            self.pending_track_defaults
                .insert(request_id, rebased.clone());
            settings_actions.push(SettingsAction::SaveTrackDefaults {
                request_id,
                draft: rebased,
            });
        }
    }

    pub fn notify_track_default_save_result(
        &mut self,
        request_id: u64,
        result: TrackDefaultSaveResult,
    ) {
        match result {
            TrackDefaultSaveResult::Accepted => {
                if let Some(draft) = self.pending_track_defaults.remove(&request_id) {
                    self.accepted_track_defaults.insert(request_id, draft);
                }
                self.confirmed_track_defaults.remove(&request_id);
            }
            TrackDefaultSaveResult::Retry => {}
            TrackDefaultSaveResult::Failed => {
                self.pending_track_defaults.remove(&request_id);
                self.confirmed_track_defaults.remove(&request_id);
                self.accepted_track_defaults.remove(&request_id);
            }
        }
    }

    fn cancel_add_track(&mut self) {
        self.add_track_open = false;
    }

    fn ensure_logo(&mut self, context: &egui::Context) {
        if self.logo.is_some() {
            return;
        }
        let Ok(image) = image::load_from_memory(LOGO_BYTES) else {
            return;
        };
        let rgba = image.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.logo = Some(context.load_texture("shoopdaloop-logo", color_image, Default::default()));
    }

    fn show_logo(&mut self, ui: &mut egui::Ui, state: &AppState) {
        ui.add_space(12.0);
        if let Some(logo) = &self.logo {
            let size = logo.size_vec2();
            let width = (ui.available_width() - 12.0).max(1.0).min(128.0);
            let height = width * size.y / size.x;
            ui.add(egui::Image::new((logo.id(), egui::vec2(width, height))));
        } else {
            ui.heading("ShoopDaLoop");
        }
        ui.add_space(3.0);
        if !state.status.version.is_empty() {
            ui.label(
                egui::RichText::new(format!("ShoopDaLoop v{}", state.status.version))
                    .size(10.0)
                    .color(colors::MUTED_FOREGROUND),
            );
        }
    }

    fn show_bottom_status(
        &mut self,
        ui: &mut egui::Ui,
        state: &AppState,
        actions: &mut Vec<AppAction>,
        settings_actions: &mut Vec<SettingsAction>,
    ) {
        #[cfg(test)]
        {
            self.reset_xruns_rect = None;
        }
        let xruns = ui
            .add(egui::Label::new(format!("({})", state.status.xruns)).sense(egui::Sense::click()))
            .on_hover_text("Audio xruns; click to reset");
        #[cfg(test)]
        {
            self.xrun_menu_rect = Some(xruns.rect);
        }
        egui::Popup::menu(&xruns).show(|ui| {
            let reset = ui.button("Reset xruns to 0");
            #[cfg(test)]
            {
                self.reset_xruns_rect = Some(reset.rect);
            }
            if reset.clicked() {
                actions.push(AppAction::ResetXruns);
                ui.close();
            }
        });
        ui.add(
            egui::ProgressBar::new((state.status.dsp_load_percent / 100.0).clamp(0.0, 1.0))
                .desired_width(86.0)
                .desired_height(4.0)
                .fill(colors::COLORED_HIGHLIGHT)
                .corner_radius(0),
        );
        ui.label("DSP");
        ui.separator();
        let milliseconds = state
            .status
            .latency_ms()
            .map(|latency| format!("{latency:.2} ms"))
            .unwrap_or_else(|| "-- ms".to_owned());
        ui.label(format!(
            "latency: {} frames | {milliseconds}",
            state.status.buffer_size
        ));
        self.show_backend_status(ui, state);
        if self.tracing_status.active {
            ui.separator();
            ui.horizontal(|ui| {
                if self.tracing_status.buffer_capacity_bytes > 0 {
                    ui.label(format!(
                        "Tracing active ({} buffer capacity)",
                        format_memory_usage(self.tracing_status.buffer_capacity_bytes)
                    ));
                } else {
                    ui.label("Tracing active");
                }
                let save = ui.small_button("Save");
                let discard = ui.small_button("Discard");
                #[cfg(test)]
                {
                    self.tracing_save_rect = Some(save.rect);
                    self.tracing_discard_rect = Some(discard.rect);
                }
                if save.clicked() {
                    settings_actions.push(SettingsAction::StopTracing { save: true });
                }
                if discard.clicked() {
                    settings_actions.push(SettingsAction::StopTracing { save: false });
                }
            });
        } else {
            #[cfg(test)]
            {
                self.tracing_save_rect = None;
                self.tracing_discard_rect = None;
            }
        }
    }

    fn show_tracing_stopped(&mut self, context: &egui::Context) {
        let Some(stopped) = self.tracing_stopped.clone() else {
            return;
        };
        let mut open = true;
        let mut acknowledged = false;
        egui::Window::new("Tracing stopped")
            .id(egui::Id::new("tracing_stopped"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                match &stopped {
                    TracingStopped::Saved(path) => {
                        ui.label("The trace was stopped and saved to:");
                        ui.monospace(path);
                    }
                    TracingStopped::Discarded => {
                        ui.label("The trace was stopped and discarded.");
                    }
                }
                if ui.button("OK").clicked() {
                    acknowledged = true;
                }
            });
        if acknowledged || !open {
            self.tracing_stopped = None;
        }
    }

    fn show_backend_status(&mut self, ui: &mut egui::Ui, state: &AppState) {
        let now = ui.input(|input| input.time);
        if state.status.callback_count != self.last_callback_count {
            self.last_callback_count = state.status.callback_count;
            self.callbacks_active_until = now + 1.0;
        }
        let callbacks_active = state.status.callback_count > 0 && now < self.callbacks_active_until;
        if callbacks_active {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    self.callbacks_active_until - now,
                ));
        }
        let health = backend_health(state.status.audio_driver, callbacks_active);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 4.0, health.color());
        response.on_hover_ui(|ui| {
            let backend_type = state
                .audio_drivers
                .active
                .as_ref()
                .map(|active| active.configured.kind().label())
                .unwrap_or("Unavailable");
            ui.label(format!("Backend type: {backend_type}"));
            ui.label(format!("Driver status: {:?}", state.status.audio_driver));
            ui.label(if callbacks_active {
                "Audio callbacks are active"
            } else {
                "Audio callbacks are not currently advancing"
            });
            ui.label(format!("Callbacks: {}", state.status.callback_count));
            if state.audio_drivers.switch.status != crate::AudioDriverSwitchStatus::Idle {
                ui.label(format!(
                    "Driver switch: {:?}",
                    state.audio_drivers.switch.status
                ));
            }
            if !state.audio_drivers.switch.message.is_empty() {
                ui.label(&state.audio_drivers.switch.message);
            }
        });
    }

    fn show_sync_track(
        &mut self,
        ui: &mut egui::Ui,
        sync: &crate::TrackState,
        state: &AppState,
        actions: &mut Vec<AppAction>,
    ) {
        let response = self
            .sync_track
            .show_with_global_controls(ui, sync, &state.global_controls);
        actions.extend(response.io_intents.iter().cloned());
        actions.extend(response.loop_actions.into_iter().map(|(loop_id, action)| {
            AppAction::Loop {
                track_id: sync.id,
                loop_id,
                action,
            }
        }));
        if response.connections_requested {
            self.connections.open(ConnectionScope::Track(sync.id));
        }
        if let Some(loop_id) = response.click_track_requested {
            if let Some(loop_state) = sync.loops.iter().find(|loop_| loop_.id == loop_id) {
                self.click_track.open(loop_state, &state.click_track);
            }
        }
        actions.extend(response.actions.into_iter().map(|action| AppAction::Track {
            track_id: sync.id,
            action,
        }));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackendHealth {
    Active,
    Waiting,
    Failed,
}

impl BackendHealth {
    fn color(self) -> egui::Color32 {
        match self {
            Self::Active => colors::SUCCESS,
            Self::Waiting => colors::WARNING,
            Self::Failed => colors::STRONG_ERROR,
        }
    }
}

fn format_memory_usage(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn backend_health(driver: crate::AudioDriverState, callbacks_active: bool) -> BackendHealth {
    use crate::AudioDriverState;
    match driver {
        AudioDriverState::Dummy | AudioDriverState::Running if callbacks_active => {
            BackendHealth::Active
        }
        AudioDriverState::Denied | AudioDriverState::Unsupported | AudioDriverState::Failed => {
            BackendHealth::Failed
        }
        _ => BackendHealth::Waiting,
    }
}

fn audio_channel_selection_action(task_id: crate::TaskId, channels: &[u32]) -> AppAction {
    AppAction::ConfirmAudioChannelSelection {
        task_id,
        channels: channels.to_vec(),
    }
}

#[derive(Default)]
pub(crate) struct NewTrackConfigurationUi {
    pub midi_id: Option<egui::Id>,
    pub recording_frames_rect: Option<egui::Rect>,
    pub processor_frames_rect: Option<egui::Rect>,
}

pub(crate) fn show_new_track_configuration(
    ui: &mut egui::Ui,
    id: &str,
    configuration: &mut NewTrackConfiguration,
    processors: &[TrackProcessorDescriptor],
) -> NewTrackConfigurationUi {
    if configuration.mode == AddTrackMode::DryWet
        && configuration.processor.as_ref().is_none_or(|selected| {
            !processors
                .iter()
                .any(|processor| processor.available && processor.id == *selected)
        })
    {
        configuration.processor = processors
            .iter()
            .find(|processor| processor.available)
            .map(|processor| processor.id.clone());
    }
    let selected_processor = configuration.processor.as_ref().and_then(|selected| {
        processors
            .iter()
            .find(|processor| processor.id == *selected)
    });
    let midi_policy = selected_processor
        .map(|processor| processor.constraints.midi)
        .unwrap_or(crate::TrackProcessorMidiPolicy::Unsupported);
    if configuration.mode == AddTrackMode::DryWet && selected_processor.is_some() {
        match midi_policy {
            crate::TrackProcessorMidiPolicy::Required => configuration.dry_midi = true,
            crate::TrackProcessorMidiPolicy::Unsupported => configuration.dry_midi = false,
            crate::TrackProcessorMidiPolicy::Optional => {}
        }
    }

    let mut response = NewTrackConfigurationUi::default();
    ui.push_id(id, |ui| {
        egui::Grid::new("fields")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Track type:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut configuration.mode, AddTrackMode::Regular, "Regular");
                    ui.selectable_value(&mut configuration.mode, AddTrackMode::Trigger, "Trigger");
                    ui.selectable_value(&mut configuration.mode, AddTrackMode::DryWet, "Dry + Wet");
                });
                ui.end_row();

                let audio_enabled = configuration.mode != AddTrackMode::Trigger;
                ui.add_enabled(audio_enabled, egui::Label::new("Audio:"));
                ui.add_enabled_ui(audio_enabled, |ui| {
                    show_audio_channel_count(ui, "audio", &mut configuration.audio_channels);
                });
                ui.end_row();

                let midi_applicable = match configuration.mode {
                    AddTrackMode::Regular => true,
                    AddTrackMode::Trigger => false,
                    AddTrackMode::DryWet => {
                        midi_policy != crate::TrackProcessorMidiPolicy::Unsupported
                    }
                };
                ui.add_enabled(midi_applicable, egui::Label::new("MIDI:"));
                let (mut displayed_midi, midi_editable) = match configuration.mode {
                    AddTrackMode::Regular => (configuration.midi, true),
                    AddTrackMode::Trigger => (false, false),
                    AddTrackMode::DryWet => (
                        configuration.dry_midi,
                        midi_policy == crate::TrackProcessorMidiPolicy::Optional,
                    ),
                };
                let midi = ui.add_enabled(
                    midi_editable,
                    egui::Checkbox::new(&mut displayed_midi, "Enabled"),
                );
                response.midi_id = Some(midi.id);
                if midi.changed() {
                    match configuration.mode {
                        AddTrackMode::Regular => configuration.midi = displayed_midi,
                        AddTrackMode::DryWet => configuration.dry_midi = displayed_midi,
                        AddTrackMode::Trigger => {}
                    }
                }
                ui.end_row();

                let processing_enabled = configuration.mode == AddTrackMode::DryWet;
                ui.add_enabled(processing_enabled, egui::Label::new("Processing:"));
                let selected = if processing_enabled {
                    selected_processor
                        .map(|processor| processor.label.as_str())
                        .unwrap_or("No processors available")
                } else {
                    "Not applicable"
                };
                ui.add_enabled_ui(processing_enabled, |ui| {
                    egui::ComboBox::from_id_salt("processor")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for processor in processors {
                                ui.add_enabled_ui(processor.available, |ui| {
                                    ui.selectable_value(
                                        &mut configuration.processor,
                                        Some(processor.id.clone()),
                                        &processor.label,
                                    );
                                });
                                if let Some(reason) = &processor.unavailable_reason {
                                    ui.small(reason);
                                }
                            }
                        });
                });
                ui.end_row();

                ui.label("Recording alignment:");
                egui::ComboBox::from_id_salt("recording_adjustment")
                    .selected_text(recording_adjustment_label(
                        configuration.recording_adjustment,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut configuration.recording_adjustment,
                            RecordingOffsetAdjustmentState::Automatic,
                            "Automatic",
                        );
                        ui.selectable_value(
                            &mut configuration.recording_adjustment,
                            RecordingOffsetAdjustmentState::ManualOverride,
                            "Manual",
                        );
                        ui.selectable_value(
                            &mut configuration.recording_adjustment,
                            RecordingOffsetAdjustmentState::AutomaticPlusTrim,
                            "Automatic + trim",
                        );
                    });
                ui.end_row();

                ui.label(
                    if configuration.recording_adjustment
                        == RecordingOffsetAdjustmentState::AutomaticPlusTrim
                    {
                        "Recording trim:"
                    } else {
                        "Recording offset:"
                    },
                );
                let recording_frames = ui.add_enabled(
                    configuration.recording_adjustment != RecordingOffsetAdjustmentState::Automatic,
                    egui::DragValue::new(&mut configuration.recording_frames)
                        .range(-crate::MAX_TRACK_LATENCY_FRAMES..=crate::MAX_TRACK_LATENCY_FRAMES)
                        .suffix(" frames"),
                );
                response.recording_frames_rect = Some(recording_frames.rect);
                ui.end_row();

                ui.label("Processor latency:");
                egui::ComboBox::from_id_salt("processor_adjustment")
                    .selected_text(processor_adjustment_label(
                        configuration.processor_adjustment,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut configuration.processor_adjustment,
                            ProcessorLatencyAdjustmentState::Automatic,
                            "Automatic",
                        );
                        ui.selectable_value(
                            &mut configuration.processor_adjustment,
                            ProcessorLatencyAdjustmentState::ManualOverride,
                            "Manual",
                        );
                        ui.selectable_value(
                            &mut configuration.processor_adjustment,
                            ProcessorLatencyAdjustmentState::AutomaticPlusTrim,
                            "Automatic + trim",
                        );
                    });
                ui.end_row();

                ui.label(
                    if configuration.processor_adjustment
                        == ProcessorLatencyAdjustmentState::AutomaticPlusTrim
                    {
                        "Processor trim:"
                    } else {
                        "Processor latency value:"
                    },
                );
                let processor_min = if configuration.processor_adjustment
                    == ProcessorLatencyAdjustmentState::ManualOverride
                {
                    0
                } else {
                    -crate::MAX_TRACK_LATENCY_FRAMES
                };
                let processor_frames = ui.add_enabled(
                    configuration.processor_adjustment
                        != ProcessorLatencyAdjustmentState::Automatic,
                    egui::DragValue::new(&mut configuration.processor_frames)
                        .range(processor_min..=crate::MAX_TRACK_LATENCY_FRAMES)
                        .suffix(" frames"),
                );
                response.processor_frames_rect = Some(processor_frames.rect);
                ui.end_row();
            });
    });
    response
}

fn show_audio_channel_count(ui: &mut egui::Ui, id: &str, channels: &mut u32) {
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(channels, 0, "Disabled");
            ui.selectable_value(channels, 1, "Mono");
            ui.selectable_value(channels, 2, "Stereo");

            let custom_selected = *channels > 2;
            if ui.selectable_label(custom_selected, "Other").clicked() && !custom_selected {
                *channels = 3;
            }
            let mut custom_channels = (*channels).max(3);
            let custom = ui.add_enabled(
                *channels > 2,
                egui::DragValue::new(&mut custom_channels)
                    .range(3..=u32::MAX)
                    .speed(1),
            );
            if custom.changed() {
                *channels = custom_channels;
            }
        });
    });
}

fn piano_destinations(state: &AppState) -> Vec<crate::TrackId> {
    state
        .tracks
        .iter()
        .filter(|track| {
            track.controls.input_monitoring
                && track.port_ids.iter().any(|port_id| {
                    state.connections.application_ports.iter().any(|port| {
                        port.id == *port_id
                            && matches!(
                                port.owner,
                                crate::ApplicationPortOwner::Track { track_id, .. }
                                    if track_id == track.id
                            )
                            && port.data_type == crate::PortDataType::Midi
                            && port.direction == crate::PortDirection::Input
                            && port.role == crate::PortRole::MidiInput
                    })
                })
        })
        .map(|track| track.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use super::*;
    use crate::{
        LoopDetailsState, MidiEventState, MidiSequenceChannelState, TrackState,
        WaveformChannelState,
    };
    use shoop_settings::{
        SettingsDraft, SettingsPersistenceState, SettingsRegistryBuilder, SettingsViewState,
    };

    #[shoop_wasm_test_support::shoop_test]
    fn backend_health_requires_live_callbacks_and_distinguishes_waiting_from_failure() {
        assert_eq!(
            backend_health(crate::AudioDriverState::Running, true),
            BackendHealth::Active
        );
        assert_eq!(
            backend_health(crate::AudioDriverState::Running, false),
            BackendHealth::Waiting
        );
        assert_eq!(
            backend_health(crate::AudioDriverState::AwaitingGesture, false),
            BackendHealth::Waiting
        );
        assert_eq!(
            backend_health(crate::AudioDriverState::Failed, false),
            BackendHealth::Failed
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn tracing_memory_usage_uses_readable_binary_units() {
        assert_eq!(format_memory_usage(0), "0 B");
        assert_eq!(format_memory_usage(1536), "1.5 KiB");
        assert_eq!(format_memory_usage(3 * 1024 * 1024), "3.0 MiB");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn active_tracing_status_offers_save_action() {
        let context = egui::Context::default();
        let mut widget = AppWidget::default();
        widget.set_tracing_status(TracingStatus {
            available: true,
            unavailable_reason: None,
            active: true,
            buffer_capacity_bytes: 3 * 1024 * 1024,
        });
        let state = AppState::default();
        let frame = |widget: &mut AppWidget, events: Vec<egui::Event>| {
            let mut settings_actions = Vec::new();
            let mut ignored_output_0 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 100.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| widget.show_bottom_status(ui, &state, &mut Vec::new(), &mut settings_actions),
            );
            ignored_output_0.textures_delta.clear();
            settings_actions
        };

        frame(&mut widget, Vec::new());
        let save = widget.tracing_save_rect.unwrap().center();
        frame(
            &mut widget,
            vec![
                egui::Event::PointerMoved(save),
                egui::Event::PointerButton {
                    pos: save,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let actions = frame(
            &mut widget,
            vec![
                egui::Event::PointerMoved(save),
                egui::Event::PointerButton {
                    pos: save,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(actions
            .iter()
            .any(|action| matches!(action, SettingsAction::StopTracing { save: true })));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bottom_panel_starts_closed() {
        assert_eq!(AppWidget::default().bottom_pane, None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn appearance_defaults_include_detected_touch_mode() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings_with_appearance_defaults(&mut builder, 1.25, true).unwrap();
        let registry = builder.finish();
        let defaults = registry.defaults(1);
        assert_eq!(defaults.get(UI_SCALE_FACTOR).unwrap(), 1.25);
        assert!(defaults.get(TOUCH_MODE).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn manual_processor_track_defaults_reject_negative_frames_but_trim_accepts_them() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES, -1);
        assert_eq!(
            registry.validate_draft(&draft),
            Err(SettingsDraftError::InvalidValue(
                DEFAULT_NEW_TRACK_PROCESSOR_FRAMES.id().to_owned()
            ))
        );

        draft.set(
            DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT,
            "automatic_plus_trim".to_owned(),
        );
        assert_eq!(registry.validate_draft(&draft), Ok(()));

        draft.set(DEFAULT_NEW_TRACK_MODE, "dry_wet".to_owned());
        assert_eq!(
            registry.validate_draft(&draft),
            Err(SettingsDraftError::InvalidValue(
                DEFAULT_NEW_TRACK_PROCESSOR.id().to_owned()
            ))
        );
        draft.set(DEFAULT_NEW_TRACK_PROCESSOR, "processor".to_owned());
        assert_eq!(registry.validate_draft(&draft), Ok(()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connection_open_api_applies_global_sync_and_main_track_presets() {
        let mut widget = AppWidget::default();
        assert_eq!(widget.open_connection_scope(), None);
        widget.open_connections(ConnectionScope::AllTracks);
        assert_eq!(
            widget.open_connection_scope(),
            Some(ConnectionScope::AllTracks)
        );
        assert_eq!(widget.connections.test_selected_tracks(), None);
        assert_eq!(widget.connections.test_data_type_filters(), (true, true));

        for track_id in [crate::TrackId::from_raw(1), crate::TrackId::from_raw(2)] {
            widget.open_connections(ConnectionScope::Track(track_id));
            assert_eq!(
                widget.open_connection_scope(),
                Some(ConnectionScope::Track(track_id))
            );
            assert_eq!(
                widget.connections.test_selected_tracks(),
                Some(BTreeSet::from([track_id]))
            );
            assert_eq!(widget.connections.test_data_type_filters(), (true, true));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn carla_hosting_setting_validates_modes_and_preserves_unknown_keys() {
        let mut builder = SettingsRegistryBuilder::default();
        register_carla_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let defaults = registry.defaults(1);
        assert_eq!(
            carla_hosting_mode_from_snapshot(&defaults).unwrap(),
            shoop_settings::CarlaHostingMode::InProcess
        );
        let definition = registry.definition(CARLA_HOSTING_MODE.id()).unwrap();
        assert_eq!(definition.effect(), SettingEffect::RestartRequired);

        let mut draft = SettingsDraft::from_snapshot(&defaults);
        draft.set(CARLA_HOSTING_MODE, "subprocess".to_owned());
        let mut base = shoop_settings::SettingsDocument::empty("test");
        base.values.insert(
            "future.setting".to_owned(),
            serde_json::Value::String("preserved".to_owned()),
        );
        let document = registry.document_from_draft(&base, &draft, "test").unwrap();
        let resolved = registry.resolve(&document, 2);
        assert_eq!(
            carla_hosting_mode_from_snapshot(&resolved.snapshot).unwrap(),
            shoop_settings::CarlaHostingMode::Subprocess
        );
        assert_eq!(
            document.values.get("future.setting"),
            Some(&serde_json::Value::String("preserved".to_owned()))
        );

        let mut invalid = document;
        invalid.values.insert(
            CARLA_HOSTING_MODE.id().to_owned(),
            serde_json::Value::String("invalid".to_owned()),
        );
        let resolved = registry.resolve(&invalid, 3);
        assert_eq!(
            carla_hosting_mode_from_snapshot(&resolved.snapshot).unwrap(),
            shoop_settings::CarlaHostingMode::InProcess
        );
        assert!(resolved
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.key.as_deref() == Some(CARLA_HOSTING_MODE.id())));
    }

    fn settings_state() -> SettingsViewState {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        }
    }

    fn frame(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        events: Vec<egui::Event>,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let settings = settings_state();
        let mut ignored_output_1 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = widget.show(ui, state, &settings, None).app_actions,
        );
        ignored_output_1.textures_delta.clear();
        actions
    }

    fn click(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        position: egui::Pos2,
    ) -> Vec<AppAction> {
        frame(
            context,
            widget,
            state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            context,
            widget,
            state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ephemeral_script_load_waits_for_confirmation_and_emits_source() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState {
            scripting: Arc::new(crate::ScriptingState {
                supported: true,
                scripts: Arc::from([crate::ScriptState {
                    id: crate::ScriptId::from_raw(1),
                    name: "controller.lua".to_owned(),
                    identity: None,
                    kind: crate::ScriptKind::Bundled,
                    enabled: true,
                    lifecycle: crate::ScriptLifecycle::Listening,
                    documentation: None,
                    resource_base_uri: None,
                    latest_error: None,
                    activity: Default::default(),
                    midi: Default::default(),
                    logs: Arc::from([]),
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        widget.queue_ephemeral_script(
            "controller.lua".to_owned(),
            Arc::from("shoop_announce_api_version(1, 0); print('loaded')"),
        );
        assert!(frame(&context, &mut widget, &state, Vec::new()).is_empty());
        assert!(frame(&context, &mut widget, &state, Vec::new()).is_empty());
        let accept = widget.ephemeral_script_accept_rect.unwrap().center();
        let mut actions = frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(accept),
                egui::Event::PointerButton {
                    pos: accept,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        actions.extend(frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(accept),
                egui::Event::PointerButton {
                    pos: accept,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ));
        assert_eq!(
            actions,
            [AppAction::AddEphemeralScript {
                name: "controller.lua".to_owned(),
                source: Arc::from("shoop_announce_api_version(1, 0); print('loaded')"),
                source_path: None,
            }]
        );
        assert!(widget.pending_ephemeral_scripts.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn xrun_reset_button_emits_one_reset_action() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState {
            status: crate::StatusState {
                xruns: 4,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        frame(&context, &mut widget, &state, Vec::new());
        let menu = widget.xrun_menu_rect.unwrap().center();
        assert!(click(&context, &mut widget, &state, menu).is_empty());
        frame(&context, &mut widget, &state, Vec::new());
        let reset = widget.reset_xruns_rect.unwrap().center();
        assert_eq!(
            click(&context, &mut widget, &state, reset),
            vec![AppAction::ResetXruns]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn details_and_piano_toggle_one_bottom_pane_without_stacking() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        frame(&context, &mut widget, &state, Vec::new());
        let piano = widget.piano_toggle_rect.unwrap().center();
        click(&context, &mut widget, &state, piano);
        assert_eq!(widget.bottom_pane, Some(BottomPane::Piano));

        let details = widget.details_toggle_rect.unwrap().center();
        click(&context, &mut widget, &state, details);
        assert_eq!(widget.bottom_pane, Some(BottomPane::Details));
        click(&context, &mut widget, &state, details);
        assert_eq!(widget.bottom_pane, None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn switching_away_from_piano_releases_a_held_note() {
        let mut widget = AppWidget::default();
        widget.bottom_pane = Some(BottomPane::Piano);
        widget
            .piano
            .hold_for_test(crate::MidiNote::new(crate::MIDDLE_C).unwrap());
        let mut actions = Vec::new();
        widget.set_bottom_pane(Some(BottomPane::Details), &mut actions);
        assert_eq!(widget.bottom_pane, Some(BottomPane::Details));
        assert_eq!(
            actions,
            vec![AppAction::Piano(crate::PianoAction::ReleaseAll)]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn open_piano_routes_pointer_note_actions_as_application_intents() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let track_id = crate::TrackId::from_raw(1);
        let port_id = crate::PortId::from_raw(1);
        let state = AppState {
            tracks: vec![TrackState {
                id: track_id,
                port_ids: Arc::from([port_id]),
                controls: crate::TrackControlState {
                    input_monitoring: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            connections: Arc::new(crate::ConnectionViewState {
                application_ports: Arc::from([crate::ApplicationPortState {
                    id: port_id,
                    owner: crate::ApplicationPortOwner::Track {
                        track_id,
                        kind: crate::TrackPortOwnerKind::Main,
                    },
                    name: "midi_in".to_owned(),
                    data_type: crate::PortDataType::Midi,
                    direction: crate::PortDirection::Input,
                    role: crate::PortRole::MidiInput,
                    connection_policy: crate::ConnectionPolicy::UserManaged,
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        frame(&context, &mut widget, &state, Vec::new());
        let piano_toggle = widget.piano_toggle_rect.unwrap().center();
        click(&context, &mut widget, &state, piano_toggle);
        frame(&context, &mut widget, &state, Vec::new());
        let keyboard = widget.piano.keyboard_rect().unwrap();
        let note_position = crate::PianoLayout::new(keyboard.min)
            .key_rect(crate::MIDDLE_C)
            .unwrap()
            .center();
        let pressed = frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(note_position),
                egui::Event::PointerButton {
                    pos: note_position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            pressed,
            vec![AppAction::Piano(crate::PianoAction::Press(
                crate::MidiNote::new(crate::MIDDLE_C).unwrap()
            ))]
        );
        let released = frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(note_position),
                egui::Event::PointerButton {
                    pos: note_position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            released,
            vec![AppAction::Piano(crate::PianoAction::Release(
                crate::MidiNote::new(crate::MIDDLE_C).unwrap()
            ))]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn piano_destination_summary_uses_monitored_track_midi_input_roles() {
        let first_id = crate::TrackId::from_raw(1);
        let muted_id = crate::TrackId::from_raw(2);
        let midi_port = crate::PortId::from_raw(10);
        let output_port = crate::PortId::from_raw(11);
        let state = AppState {
            tracks: vec![
                TrackState {
                    id: first_id,
                    name: "Listening".to_owned(),
                    port_ids: Arc::from([midi_port]),
                    controls: crate::TrackControlState {
                        input_monitoring: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                TrackState {
                    id: muted_id,
                    name: "Muted".to_owned(),
                    port_ids: Arc::from([output_port]),
                    controls: crate::TrackControlState {
                        input_monitoring: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ],
            connections: Arc::new(crate::ConnectionViewState {
                application_ports: Arc::from([
                    crate::ApplicationPortState {
                        id: midi_port,
                        owner: crate::ApplicationPortOwner::Track {
                            track_id: first_id,
                            kind: crate::TrackPortOwnerKind::Main,
                        },
                        name: "in".to_owned(),
                        data_type: crate::PortDataType::Midi,
                        direction: crate::PortDirection::Input,
                        role: crate::PortRole::MidiInput,
                        connection_policy: crate::ConnectionPolicy::UserManaged,
                    },
                    crate::ApplicationPortState {
                        id: output_port,
                        owner: crate::ApplicationPortOwner::Track {
                            track_id: muted_id,
                            kind: crate::TrackPortOwnerKind::Main,
                        },
                        name: "out".to_owned(),
                        data_type: crate::PortDataType::Midi,
                        direction: crate::PortDirection::Output,
                        role: crate::PortRole::MidiOutput,
                        connection_policy: crate::ConnectionPolicy::UserManaged,
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(piano_destinations(&state), [first_id]);
    }

    fn settings_frame(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        settings: &SettingsViewState,
        paths: &BTreeMap<crate::ScriptId, String>,
        events: Vec<egui::Event>,
    ) -> AppWidgetResponse {
        let mut response = None;
        let mut ignored_output_2 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| response = Some(widget.show(ui, state, settings, Some(paths))),
        );
        ignored_output_2.textures_delta.clear();
        response.unwrap()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn audio_settings_keep_independent_driver_configs_and_validate_mapping() {
        let mut builder = SettingsRegistryBuilder::default();
        register_audio_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let snapshot = registry.defaults(7);
        assert_eq!(loop_edge_smoothing_ms(&snapshot).unwrap(), 3);
        let smoothing = registry.definition(LOOP_EDGE_SMOOTHING_MS.id()).unwrap();
        assert_eq!(
            smoothing.editor(),
            &SettingEditor::UnsignedInteger { min: 0, max: 100 }
        );
        assert!(smoothing.help().contains("0 disables"));
        assert_eq!(
            selected_audio_driver(&snapshot).unwrap(),
            AudioDriverKind::Dummy
        );
        let dummy = audio_driver_config_from_snapshot(&snapshot, AudioDriverKind::Dummy).unwrap();
        assert_eq!(
            dummy,
            AudioDriverConfig::Dummy(DummyAudioDriverConfig::default())
        );

        let mut draft = SettingsDraft::from_snapshot(&snapshot);
        draft.set(CPAL_HOST, "test-host".to_owned());
        draft.set(CPAL_OUTPUT_DEVICE, "speakers".to_owned());
        draft.set(CPAL_INPUT_DEVICE, "microphone".to_owned());
        draft.set(CPAL_SAMPLE_RATE, 44_100);
        set_selected_audio_driver(&mut draft, AudioDriverKind::Cpal);
        let cpal = audio_driver_config_from_draft(&draft, AudioDriverKind::Cpal).unwrap();
        assert_eq!(cpal.kind(), AudioDriverKind::Cpal);
        let AudioDriverConfig::Cpal(cpal) = cpal else {
            unreachable!();
        };
        assert_eq!(cpal.host, "test-host");
        assert_eq!(cpal.sample_rate, 44_100);
        assert_eq!(
            audio_driver_config_from_draft(&draft, AudioDriverKind::Dummy).unwrap(),
            dummy
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn add_track_accept_emits_validated_spec() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "New Track".to_owned();
        widget.add_track_audio_channels = 4;
        widget.add_track_midi = true;
        widget.add_track_recording_adjustment = RecordingOffsetAdjustmentState::AutomaticPlusTrim;
        widget.add_track_recording_frames = -12;
        widget.add_track_processor_adjustment = ProcessorLatencyAdjustmentState::ManualOverride;
        widget.add_track_processor_frames = 256;
        frame(&context, &mut widget, &state, Vec::new());
        assert!(widget.add_track_accept_rect.is_some());
        assert!(widget.add_track_recording_frames_rect.is_some());
        assert!(widget.add_track_processor_frames_rect.is_some());
        assert!(widget.add_track_make_default_rect.is_some());
        assert_eq!(
            widget.accept_add_track(&[]),
            Some(AppAction::AddTrackWithTopology(TrackSpec {
                name: "New Track".to_owned(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 4,
                    midi: true,
                },
                latency: TrackLatencySpec {
                    adjustment: RecordingOffsetAdjustmentState::AutomaticPlusTrim,
                    manual_frames: -12,
                    processor_adjustment: ProcessorLatencyAdjustmentState::ManualOverride,
                    processor_manual_frames: 256,
                },
                creation_request_id: None,
            }))
        );
        assert!(!widget.add_track_open);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn invalid_add_track_latency_keeps_the_dialog_draft_open() {
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Invalid latency".to_owned();
        widget.add_track_processor_adjustment = ProcessorLatencyAdjustmentState::ManualOverride;
        widget.add_track_processor_frames = -1;
        assert_eq!(widget.accept_add_track(&[]), None);
        assert!(widget.add_track_open);
        assert_eq!(widget.add_track_processor_frames, -1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn add_track_midi_widget_keeps_its_id_across_track_types() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Track".to_owned();

        frame(&context, &mut widget, &state, Vec::new());
        let id = widget.add_track_midi_id.unwrap();
        for mode in [AddTrackMode::Trigger, AddTrackMode::DryWet] {
            widget.add_track_mode = mode;
            frame(&context, &mut widget, &state, Vec::new());
            assert_eq!(widget.add_track_midi_id, Some(id));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn trigger_track_has_no_audio_or_midi() {
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Trigger".to_owned();
        widget.add_track_mode = AddTrackMode::Trigger;
        widget.add_track_audio_channels = 4;
        widget.add_track_midi = true;

        assert_eq!(
            widget.accept_add_track(&[]),
            Some(AppAction::AddTrackWithTopology(TrackSpec {
                name: "Trigger".to_owned(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 0,
                    midi: false,
                },
                latency: TrackLatencySpec::default(),
                creation_request_id: None,
            }))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dry_wet_dialog_uses_matching_audio_and_processor_catalogs() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Processed".to_owned();
        widget.add_track_mode = AddTrackMode::DryWet;
        widget.add_track_audio_channels = 2;
        widget.add_track_dry_midi = true;
        frame(&context, &mut widget, &AppState::default(), Vec::new());
        assert!(widget.add_track_processor.is_none());
        assert_eq!(widget.accept_add_track(&[]), None);
        assert!(widget.add_track_open);

        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new("future_browser_fx"),
            label: "Future browser FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: crate::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: None,
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: crate::TrackProcessorMidiPolicy::Optional,
            },
            features: crate::TrackProcessorFeatures {
                state: true,
                external_ui: true,
                embedded_ui: false,
                recovery: true,
                logs: true,
            },
            editor: None,
        };
        let state = AppState {
            track_processors: Arc::from([processor.clone()]),
            ..Default::default()
        };
        frame(&context, &mut widget, &state, Vec::new());
        assert_eq!(widget.add_track_processor, Some(processor.id.clone()));
        assert_eq!(
            widget.accept_add_track(&[processor.clone()]),
            Some(AppAction::AddTrackWithTopology(TrackSpec {
                name: "Processed".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                    processor_type: processor.id,
                },
                latency: TrackLatencySpec::default(),
                creation_request_id: None,
            }))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cancelling_add_track_has_no_action() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Cancelled".to_owned();
        frame(&context, &mut widget, &state, Vec::new());
        assert!(widget.add_track_cancel_rect.is_some());
        widget.cancel_add_track();
        assert!(!widget.add_track_open);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bus_blocks_stack_immediately_above_logo_without_overlapping_sync() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let bus = |id| crate::BusState {
            id: crate::BusId::from_raw(id),
            name: if id == 1 {
                "Master".to_owned()
            } else {
                format!("Bus {id}")
            },
            channels: Arc::from([
                crate::BusChannelState {
                    id: crate::BusChannelId::from_raw(id * 2 - 1),
                    label: "Left".to_owned(),
                    output_port_id: crate::PortId::from_raw(id * 2 - 1),
                },
                crate::BusChannelState {
                    id: crate::BusChannelId::from_raw(id * 2),
                    label: "Right".to_owned(),
                    output_port_id: crate::PortId::from_raw(id * 2),
                },
            ]),
            gain_db: 0.0,
            balance: 0.0,
            muted: false,
            output_peaks_db: Arc::from([-20.0, -10.0]),
            control_pending: false,
            control_error: None,
        };
        let mut state = AppState {
            tracks: vec![TrackState {
                id: crate::TrackId::from_raw(1),
                name: "Sync".to_owned(),
                is_sync: true,
                ..Default::default()
            }],
            buses: Arc::from([bus(1)]),
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        frame(&context, &mut widget, &state, Vec::new());
        let bus_rect = widget.bus_area_rect.unwrap();
        let logo_rect = widget.logo_area_rect.unwrap();
        let sync_rect = widget.sync_area_rect.unwrap();
        assert!((bus_rect.bottom() - (logo_rect.top() - SIDEBAR_SECTION_GAP)).abs() < 0.01);
        assert!(sync_rect.bottom() <= bus_rect.top());
        assert_eq!(widget.bus_controls.len(), 1);
        let mute = widget.bus_controls[&crate::BusId::from_raw(1)]
            .mute_rect()
            .unwrap()
            .center();
        frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(mute),
                egui::Event::PointerButton {
                    pos: mute,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let actions = frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(mute),
                egui::Event::PointerButton {
                    pos: mute,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            AppAction::Bus {
                bus_id,
                action: crate::BusAction::MuteChanged(true)
            } if *bus_id == crate::BusId::from_raw(1)
        )));

        state.buses = Arc::from([bus(1), bus(2), bus(3)]);
        frame(&context, &mut widget, &state, Vec::new());
        assert_eq!(widget.bus_controls.len(), 3);
        assert!(widget.bus_area_rect.unwrap().bottom() <= widget.logo_area_rect.unwrap().top());
        assert!(widget.sync_area_rect.unwrap().bottom() <= widget.bus_area_rect.unwrap().top());

        state.buses = Arc::from([bus(1)]);
        frame(&context, &mut widget, &state, Vec::new());
        assert_eq!(widget.bus_controls.len(), 1);

        let settings = settings_state();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 380.0),
                )),
                ..Default::default()
            },
            |ui| {
                widget.show(ui, &state, &settings, None);
            },
        );
        output.textures_delta.clear();
        let short_bus = widget.bus_area_rect.unwrap();
        let short_logo = widget.logo_area_rect.unwrap();
        assert!(short_bus.bottom() <= short_logo.top());
        assert!(widget.sync_area_rect.unwrap().bottom() <= short_bus.top());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ordered_audio_export_selection_emits_the_task_scoped_confirmation() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let task_id = crate::TaskId::from_raw(19);
        let state = AppState {
            io_task: Some(crate::IoTaskState {
                id: task_id,
                kind: crate::IoTaskKind::ExportLoopAudio,
                status: crate::IoTaskStatus::AwaitingChannelSelection,
                progress: 0.2,
                message: "Select channels".to_owned(),
                sample_rate_warning: None,
                audio_channel_mapping: None,
                audio_channel_selection: Some(crate::AudioChannelSelectionState {
                    available_channels: vec!["Direct 1".to_owned(), "Direct 2".to_owned()],
                    default_selection: vec![1, 0],
                }),
            }),
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        let settings = settings_state();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                widget.show(ui, &state, &settings, None);
            },
        );
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert_eq!(
            audio_channel_selection_action(task_id, &[1, 0]),
            AppAction::ConfirmAudioChannelSelection {
                task_id,
                channels: vec![1, 0],
            }
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn scripts_tab_renders_grouped_controls_for_runtime_diagnostics() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let settings = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut widget = AppWidget::new(registry);
        widget.settings.open(&settings);
        widget.settings.select_category("Scripts");
        let script_id = crate::ScriptId::from_raw(1);
        let state = AppState {
            scripting: crate::ScriptingState {
                supported: true,
                scripts: Arc::from([crate::ScriptState {
                    id: script_id,
                    name: "controller.lua".to_owned(),
                    identity: None,
                    kind: crate::ScriptKind::User,
                    enabled: true,
                    lifecycle: crate::ScriptLifecycle::Error,
                    documentation: Some("Controller help".to_owned()),
                    resource_base_uri: None,
                    latest_error: Some("bad callback".to_owned()),
                    activity: crate::ScriptActivityDiagnostics {
                        loop_callbacks: 1,
                        global_callbacks: 2,
                        keyboard_callbacks: 3,
                        timers: 4,
                    },
                    midi: crate::ScriptMidiDiagnostics {
                        rules: 2,
                        connections: 1,
                        dropped_messages: 3,
                        errors: 4,
                        rule_states: Arc::from([crate::ScriptMidiRuleDiagnostics {
                            direction: crate::ScriptMidiRuleDirection::Output,
                            pattern: "APC Mini".to_owned(),
                            matched_endpoints: Arc::from(["APC Mini [sink]".to_owned()]),
                            connected_endpoints: Arc::from(["APC Mini [sink]".to_owned()]),
                            endpoints: Arc::from([crate::ScriptMidiEndpointDiagnostics {
                                id: "sink".to_owned(),
                                name: "APC Mini".to_owned(),
                                connected: true,
                            }]),
                            latest_error: Some("permission denied".to_owned()),
                        }]),
                    },
                    logs: Arc::from([crate::ScriptLogState {
                        level: crate::ScriptLogLevel::Warning,
                        message: "warning log".to_owned(),
                    }]),
                }]),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        };
        let paths = BTreeMap::from([(script_id, "/tmp/controller.lua".to_owned())]);
        settings_frame(&context, &mut widget, &state, &settings, &paths, Vec::new());
        assert!(widget.settings.is_open());

        assert!(widget.settings.restart_rect(script_id).is_some());
        assert!(widget.settings.log_rect(script_id).is_some());
        assert!(widget.settings.documentation_rect(script_id).is_some());
        assert!(widget.settings.status_rect(script_id).is_some());
        assert!(widget.settings.reload_rect(script_id).is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn complete_application_state_produces_paint_commands() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = AppWidget::default();
        let state = AppState {
            tracks: vec![TrackState {
                id: crate::TrackId::from_raw(1),
                name: "Track".to_owned(),
                ..Default::default()
            }],
            details: Some(LoopDetailsState {
                title: "Loop".to_owned(),
                channels: vec![WaveformChannelState {
                    id: crate::ChannelId::from_raw(1),
                    label: "audio".to_owned(),
                    samples: Arc::from([-0.5, 0.25, 0.75, -0.1]),
                    ..Default::default()
                }],
                midi_channels: vec![MidiSequenceChannelState {
                    id: crate::ChannelId::from_raw(2),
                    label: "MIDI 1".to_owned(),
                    events: Arc::from([
                        MidiEventState {
                            frame: 1,
                            data: Arc::from([0x90, 60, 100]),
                        },
                        MidiEventState {
                            frame: 8,
                            data: Arc::from([0x80, 60, 0]),
                        },
                    ]),
                    loop_length: 16,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let settings = settings_state();
        let mut uploaded_logo = false;
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, &state, &settings, None);
                },
            );
            assert!(output.shapes.len() > 10);
            uploaded_logo |= !output.textures_delta.set.is_empty();
            output.textures_delta.clear();
        }
        assert!(uploaded_logo);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn source_tree_marker_overrides_only_the_executable_sibling_location() {
        let directory = tempfile::tempdir().unwrap();
        let executable_directory = directory.path().join("target/debug");
        std::fs::create_dir_all(&executable_directory).unwrap();
        let executable = executable_directory.join("shoopdaloop");
        let packaged = packaged_builtins_location(&executable);

        std::fs::write(executable_directory.join(SOURCE_TREE_MARKER), "../..\n").unwrap();
        assert_eq!(
            builtins_location_for_executable(&executable),
            executable_directory.join("../../resources/builtins")
        );

        std::fs::remove_file(executable_directory.join(SOURCE_TREE_MARKER)).unwrap();
        std::fs::write(
            executable_directory
                .parent()
                .unwrap()
                .join(SOURCE_TREE_MARKER),
            "..\n",
        )
        .unwrap();
        assert_eq!(builtins_location_for_executable(&executable), packaged);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn source_tree_marker_accepts_only_bounded_relative_utf8_paths() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("shoopdaloop");
        let marker = directory.path().join(SOURCE_TREE_MARKER);

        std::fs::write(&marker, directory.path().to_string_lossy().as_bytes()).unwrap();
        assert!(marked_source_builtins_location(&executable).is_none());
        std::fs::write(
            &marker,
            vec![b'x'; MAX_SOURCE_TREE_MARKER_BYTES as usize + 1],
        )
        .unwrap();
        assert!(marked_source_builtins_location(&executable).is_none());
        std::fs::write(&marker, [0xff]).unwrap();
        assert!(marked_source_builtins_location(&executable).is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bundled_script_registry_excludes_native_user_path_workflow() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_bundled_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        assert!(registry.definition(BUILTINS_LOCATION.id()).is_some());
        assert!(registry.definition(BUILTIN_SCRIPTS.id()).is_some());
        assert!(registry.definition(KEYBOARD_SCRIPT_ENABLED.id()).is_none());
        assert!(registry.definition(APC_MINI_SCRIPT_ENABLED.id()).is_none());
        assert!(registry.definition(USER_SCRIPTS.id()).is_none());
        let defaults = registry.defaults(1);
        assert!(!defaults.get(BUILTINS_LOCATION).unwrap().is_empty());
        assert!(defaults.get(BUILTIN_SCRIPTS).unwrap().0.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn complete_track_defaults_round_trip_into_a_new_dialog_draft() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let initial = registry.defaults(4);
        let mut draft = SettingsDraft::from_snapshot(&initial);
        draft.set(DEFAULT_NEW_TRACK_MODE, "dry_wet".to_owned());
        draft.set(DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 6);
        draft.set(DEFAULT_NEW_TRACK_MIDI, true);
        draft.set(DEFAULT_NEW_TRACK_DRY_MIDI, true);
        draft.set(DEFAULT_NEW_TRACK_PROCESSOR, "processor".to_owned());
        draft.set(
            DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT,
            "automatic_plus_trim".to_owned(),
        );
        draft.set(DEFAULT_NEW_TRACK_RECORDING_FRAMES, -24);
        draft.set(
            DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT,
            "automatic".to_owned(),
        );
        draft.set(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES, 480);
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let state = SettingsViewState {
            active: Arc::new(registry.resolve(&document, 5).snapshot),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Saved,
        };
        let mut widget = AppWidget::default();
        let active_defaults = SettingsDraft::from_snapshot(&state.active);
        widget.open_add_track_dialog(2, &active_defaults);
        assert_eq!(widget.add_track_name, "Track 3");
        assert_eq!(widget.add_track_mode, AddTrackMode::DryWet);
        assert_eq!(widget.add_track_audio_channels, 6);
        assert!(widget.add_track_midi);
        assert!(widget.add_track_dry_midi);
        assert_eq!(
            widget
                .add_track_processor
                .as_ref()
                .map(|value| value.as_str()),
            Some("processor")
        );
        assert_eq!(
            widget.add_track_recording_adjustment,
            RecordingOffsetAdjustmentState::AutomaticPlusTrim
        );
        assert_eq!(widget.add_track_recording_frames, -24);
        assert_eq!(
            widget.add_track_processor_adjustment,
            ProcessorLatencyAdjustmentState::Automatic
        );
        assert_eq!(widget.add_track_processor_frames, 480);
        assert!(!widget.add_track_make_default);

        let replacement = registry.defaults(6);
        assert_eq!(
            replacement.get(DEFAULT_NEW_TRACK_AUDIO_CHANNELS).unwrap(),
            2
        );
        assert_eq!(widget.add_track_audio_channels, 6);
        assert!(widget.add_track_midi);
        assert_eq!(widget.add_track_recording_frames, -24);
        assert_eq!(widget.add_track_processor_frames, 480);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn add_track_make_default_saves_all_track_defaults_after_creation_confirmation() {
        let settings = settings_state();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Saved defaults".to_owned();
        widget.add_track_mode = AddTrackMode::DryWet;
        widget.add_track_audio_channels = 3;
        widget.add_track_dry_midi = true;
        widget.add_track_processor = Some(TrackProcessorTypeId::new("processor"));
        widget.add_track_recording_adjustment = RecordingOffsetAdjustmentState::Automatic;
        widget.add_track_recording_frames = -9;
        widget.add_track_processor_adjustment = ProcessorLatencyAdjustmentState::AutomaticPlusTrim;
        widget.add_track_processor_frames = 128;
        widget.add_track_make_default = true;
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new("processor"),
            label: "Processor".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: crate::TrackProcessorConstraints {
                midi: crate::TrackProcessorMidiPolicy::Optional,
                ..Default::default()
            },
            features: Default::default(),
            editor: None,
        };

        let AppAction::AddTrackWithTopology(spec) = widget
            .accept_add_track_with_defaults(&[processor], &AppState::default(), &settings)
            .unwrap()
        else {
            panic!("expected track creation");
        };
        let request_id = spec.creation_request_id.unwrap();
        let mut settings_actions = Vec::new();
        widget.resolve_track_default_saves(&AppState::default(), &settings, &mut settings_actions);
        assert!(settings_actions.is_empty());

        let confirmed = AppState {
            track_creation_results: Arc::from([crate::TrackCreationResult {
                request_id,
                success: true,
            }]),
            ..Default::default()
        };
        let mut busy_settings = settings.clone();
        busy_settings.persistence = SettingsPersistenceState::Saving;
        widget.resolve_track_default_saves(&confirmed, &busy_settings, &mut settings_actions);
        assert!(settings_actions.is_empty());
        assert!(widget.pending_track_defaults.contains_key(&request_id));

        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let rebased_settings = SettingsViewState {
            active: Arc::new(registry.defaults(2)),
            persistence: SettingsPersistenceState::Idle,
            ..settings.clone()
        };
        widget.resolve_track_default_saves(
            &AppState::default(),
            &rebased_settings,
            &mut settings_actions,
        );
        let [SettingsAction::SaveTrackDefaults {
            request_id: saved_request_id,
            draft,
        }] = settings_actions.as_slice()
        else {
            panic!("expected correlated default save after confirmation");
        };
        assert_eq!(*saved_request_id, request_id);
        assert_eq!(draft.base_revision(), 2);
        assert_eq!(draft.get(DEFAULT_NEW_TRACK_MODE).unwrap(), "dry_wet");
        assert_eq!(draft.get(DEFAULT_NEW_TRACK_AUDIO_CHANNELS).unwrap(), 3);
        assert!(!draft.get(DEFAULT_NEW_TRACK_MIDI).unwrap());
        assert!(draft.get(DEFAULT_NEW_TRACK_DRY_MIDI).unwrap());
        assert_eq!(draft.get(DEFAULT_NEW_TRACK_PROCESSOR).unwrap(), "processor");
        assert_eq!(
            draft.get(DEFAULT_NEW_TRACK_RECORDING_ADJUSTMENT).unwrap(),
            "automatic"
        );
        assert_eq!(draft.get(DEFAULT_NEW_TRACK_RECORDING_FRAMES).unwrap(), -9);
        assert_eq!(
            draft.get(DEFAULT_NEW_TRACK_PROCESSOR_ADJUSTMENT).unwrap(),
            "automatic_plus_trim"
        );
        assert_eq!(draft.get(DEFAULT_NEW_TRACK_PROCESSOR_FRAMES).unwrap(), 128);
        let desired = draft.clone();

        widget.notify_track_default_save_result(request_id, TrackDefaultSaveResult::Retry);
        assert!(widget.pending_track_defaults.contains_key(&request_id));
        settings_actions.clear();
        let latest_settings = SettingsViewState {
            active: Arc::new(registry.defaults(3)),
            ..rebased_settings
        };
        widget.resolve_track_default_saves(
            &AppState::default(),
            &latest_settings,
            &mut settings_actions,
        );
        let [SettingsAction::SaveTrackDefaults { draft, .. }] = settings_actions.as_slice() else {
            panic!("expected rejected default save to retry");
        };
        assert_eq!(draft.base_revision(), 3);

        widget.notify_track_default_save_result(request_id, TrackDefaultSaveResult::Accepted);
        assert!(widget.pending_track_defaults.is_empty());
        assert!(widget.confirmed_track_defaults.is_empty());
        assert!(widget.accepted_track_defaults.contains_key(&request_id));

        let effective = widget.effective_track_defaults(&latest_settings);
        widget.open_add_track_dialog(1, &effective);
        assert_eq!(widget.add_track_mode, AddTrackMode::DryWet);
        assert_eq!(widget.add_track_audio_channels, 3);
        assert!(widget.add_track_dry_midi);
        assert_eq!(
            widget
                .add_track_processor
                .as_ref()
                .map(|value| value.as_str()),
            Some("processor")
        );
        assert_eq!(
            widget.add_track_recording_adjustment,
            RecordingOffsetAdjustmentState::Automatic
        );
        assert_eq!(
            widget.add_track_processor_adjustment,
            ProcessorLatencyAdjustmentState::AutomaticPlusTrim
        );
        assert_eq!(widget.add_track_processor_frames, 128);

        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &desired,
                "test",
            )
            .unwrap();
        let persisted_settings = SettingsViewState {
            active: Arc::new(registry.resolve(&document, 4).snapshot),
            persistence: SettingsPersistenceState::Saved,
            ..latest_settings
        };
        widget.reconcile_accepted_track_defaults(&persisted_settings);
        assert!(widget.accepted_track_defaults.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejected_track_creation_discards_pending_defaults_without_saving() {
        let settings = settings_state();
        let mut widget = AppWidget::default();
        widget
            .pending_track_defaults
            .insert(7, SettingsDraft::from_snapshot(&settings.active));
        let rejected = AppState {
            track_creation_results: Arc::from([crate::TrackCreationResult {
                request_id: 7,
                success: false,
            }]),
            ..Default::default()
        };
        let mut settings_actions = Vec::new();
        widget.resolve_track_default_saves(&rejected, &settings, &mut settings_actions);
        assert!(settings_actions.is_empty());
        assert!(widget.pending_track_defaults.is_empty());
        assert!(widget.confirmed_track_defaults.is_empty());
    }
}
