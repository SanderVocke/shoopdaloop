//! Controller-independent egui elements and their input/output data models.

mod app_model;
mod loop_widget;
mod tracks_widget;
mod waveform;

pub use app_model::{
    AppAction, AppState, DefaultRecordingAction, GlobalControlAction, GlobalControlState,
    IndexedLoopAction, IndexedTrackAction, LoopDetailsState, StatusState, TrackControlState,
    TrackState, TrackWidgetAction, WaveformChannelState, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB,
};
pub use loop_widget::{
    initialize, CompositeKind, LoopMode, LoopState, LoopWidget, LoopWidgetAction,
    LoopWidgetResponse,
};
pub use tracks_widget::TracksWidget;
pub use waveform::{waveform_bins, WaveformBin};
