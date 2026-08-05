//! Controller-independent egui elements.

mod app_widget;
mod details_pane;
mod global_controls;
mod loop_widget;
mod track_controls;
mod track_widget;
mod tracks_widget;
mod waveform;
mod waveform_widget;

pub use app_widget::AppWidget;
pub use details_pane::DetailsPane;
pub use global_controls::GlobalControls;
pub use loop_widget::{initialize, LoopWidget, LoopWidgetResponse};
pub use shoop_app_api::*;
pub use track_controls::TrackControls;
pub use track_widget::{TrackWidget, TrackWidgetResponse};
pub use tracks_widget::{TracksWidget, TracksWidgetResponse};
pub use waveform::{waveform_bins, WaveformBin};
pub use waveform_widget::WaveformWidget;
