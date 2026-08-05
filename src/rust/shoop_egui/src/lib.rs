//! Controller-independent egui elements and their input/output data models.

mod app_model;
mod app_widget;
mod global_controls;
mod loop_widget;
mod track_controls;
mod track_widget;
mod tracks_widget;
mod waveform;

pub use app_model::{
    AppAction, AppState, DefaultRecordingAction, GlobalControlAction, GlobalControlState,
    IndexedLoopAction, IndexedTrackAction, LoopDetailsState, StatusState, TrackControlState,
    TrackState, TrackWidgetAction, WaveformChannelState, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB,
};
pub use app_widget::AppWidget;
pub use global_controls::GlobalControls;
pub use loop_widget::{
    initialize, CompositeKind, LoopMode, LoopState, LoopWidget, LoopWidgetAction,
    LoopWidgetResponse,
};
pub use track_controls::TrackControls;
pub use track_widget::{TrackWidget, TrackWidgetResponse};
pub use tracks_widget::{TracksWidget, TracksWidgetResponse};
pub use waveform::{waveform_bins, WaveformBin};
