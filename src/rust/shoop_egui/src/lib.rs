//! Controller-independent egui elements.

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod app_widget;
mod click_track_dialog;
pub mod colors;
mod composite_loop_widget;
mod connection_dialog;
mod details_pane;
mod dial;
mod fonts;
mod global_controls;
mod key_input;
mod loop_widget;
mod meter_ballistics;
mod midi_sequence_widget;
mod optimistic_value;
mod piano_pane;
mod script_dialogs;
mod settings_dialog;
mod tiny_synth_fx_editor;
mod track_controls;
mod track_widget;
mod tracks_widget;
mod waveform;
mod waveform_widget;

pub use app_widget::{
    audio_driver_config_from_draft, audio_driver_config_from_snapshot,
    carla_hosting_mode_from_snapshot, register_audio_settings, register_bundled_script_settings,
    register_carla_settings, register_script_settings, register_settings,
    register_settings_with_ui_scale_default, selected_audio_driver, set_selected_audio_driver,
    AppWidget, AppWidgetResponse, APC_MINI_SCRIPT_ENABLED, CARLA_HOSTING_MODE, CPAL_BUFFER_SIZE,
    CPAL_CAPTURE_RING_FRAMES, CPAL_CLIENT_NAME, CPAL_HOST, CPAL_INPUT_CHANNELS, CPAL_INPUT_DEVICE,
    CPAL_MIDI_INPUTS, CPAL_MIDI_OUTPUTS, CPAL_OUTPUT_CHANNELS, CPAL_OUTPUT_DEVICE,
    CPAL_SAMPLE_RATE, DEFAULT_NEW_TRACK_AUDIO_CHANNELS, DEFAULT_NEW_TRACK_MIDI, DUMMY_BUFFER_SIZE,
    DUMMY_SAMPLE_RATE, JACK_CLIENT_NAME, KEYBOARD_SCRIPT_ENABLED, SELECTED_AUDIO_DRIVER,
    UI_SCALE_FACTOR, USER_SCRIPTS,
};
pub use composite_loop_widget::CompositeLoopWidget;
pub use connection_dialog::{ConnectionDialog, ConnectionScope};
pub use details_pane::DetailsPane;
pub use global_controls::GlobalControls;
pub use loop_widget::{LoopWidget, LoopWidgetResponse};
pub use midi_sequence_widget::MidiSequenceWidget;
pub use piano_pane::{c_label, is_black, PianoLayout, PianoPane, MIDDLE_C, MIDI_NOTE_COUNT};
pub use settings_dialog::{
    SettingsAction, SettingsDialog, SettingsDialogResponse, TracingStatus, TracingStopped,
};
pub use shoop_app_api::*;
pub use shoop_settings::*;
pub use track_controls::TrackControls;
pub use track_widget::{TrackWidget, TrackWidgetResponse};
pub use tracks_widget::{TracksWidget, TracksWidgetResponse};
pub use waveform::{waveform_bins, WaveformBin};
pub use waveform_widget::WaveformWidget;

pub fn initialize(context: &egui::Context) {
    fonts::initialize(context);
    context.all_styles_mut(|style| {
        style.visuals.widgets.hovered.bg_fill = colors::HOVER_BACKGROUND;
        style.visuals.widgets.hovered.weak_bg_fill = colors::HOVER_BACKGROUND;
    });
}

fn control_safe_scroll_source() -> egui::scroll_area::ScrollSource {
    egui::scroll_area::ScrollSource {
        drag: egui::scroll_area::DragScroll::Never,
        ..Default::default()
    }
}
