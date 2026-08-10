//! Controller-independent egui elements.

mod app_widget;
mod click_track_dialog;
pub mod colors;
mod connection_dialog;
mod details_pane;
mod dial;
mod fonts;
mod global_controls;
mod key_input;
mod loop_widget;
mod settings_dialog;
mod track_controls;
mod track_widget;
mod tracks_widget;
mod waveform;
mod waveform_widget;

pub use app_widget::{
    audio_driver_config_from_draft, audio_driver_config_from_snapshot,
    carla_hosting_mode_from_snapshot, register_audio_settings, register_bundled_script_settings,
    register_carla_settings, register_script_settings, register_settings, selected_audio_driver,
    set_selected_audio_driver, AppWidget, AppWidgetResponse, APC_MINI_SCRIPT_ENABLED,
    CARLA_HOSTING_MODE, CPAL_BUFFER_SIZE, CPAL_CAPTURE_RING_FRAMES, CPAL_CLIENT_NAME, CPAL_HOST,
    CPAL_INPUT_CHANNELS, CPAL_INPUT_DEVICE, CPAL_MIDI_INPUTS, CPAL_MIDI_OUTPUTS,
    CPAL_OUTPUT_CHANNELS, CPAL_OUTPUT_DEVICE, CPAL_SAMPLE_RATE, DEFAULT_NEW_TRACK_AUDIO_CHANNELS,
    DEFAULT_NEW_TRACK_MIDI, DUMMY_BUFFER_SIZE, DUMMY_SAMPLE_RATE, JACK_CLIENT_NAME,
    KEYBOARD_SCRIPT_ENABLED, SELECTED_AUDIO_DRIVER, USER_SCRIPTS,
};
pub use connection_dialog::{ConnectionDialog, ConnectionScope};
pub use details_pane::DetailsPane;
pub use global_controls::GlobalControls;
pub use loop_widget::{LoopWidget, LoopWidgetResponse};
pub use settings_dialog::{SettingsAction, SettingsDialog, SettingsDialogResponse};
pub use shoop_app_api::*;
pub use shoop_settings::*;
pub use track_controls::TrackControls;
pub use track_widget::{TrackWidget, TrackWidgetResponse};
pub use tracks_widget::{TracksWidget, TracksWidgetResponse};
pub use waveform::{waveform_bins, WaveformBin};
pub use waveform_widget::WaveformWidget;

pub fn initialize(context: &egui::Context) {
    fonts::initialize(context);
    egui_material_icons::initialize(context);
}
