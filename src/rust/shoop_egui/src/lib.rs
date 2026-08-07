//! Controller-independent egui elements.

mod app_widget;
mod connection_dialog;
mod details_pane;
mod fonts;
mod global_controls;
mod loop_widget;
mod settings_dialog;
mod track_controls;
mod track_widget;
mod tracks_widget;
mod waveform;
mod waveform_widget;

pub use app_widget::{
    register_settings, AppWidget, AppWidgetResponse, DEFAULT_NEW_TRACK_AUDIO_CHANNELS,
    DEFAULT_NEW_TRACK_MIDI,
};
pub use connection_dialog::{ConnectionDialog, ConnectionScope};
pub use details_pane::DetailsPane;
pub use global_controls::GlobalControls;
pub use loop_widget::{LoopWidget, LoopWidgetResponse};
pub use settings_dialog::{SettingsAction, SettingsDialog};
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
