//! Controller-independent egui elements and their input/output data models.

mod loop_widget;
mod tracks_widget;

pub use loop_widget::{
    initialize, CompositeKind, LoopMode, LoopState, LoopWidget, LoopWidgetAction,
    LoopWidgetResponse,
};
pub use tracks_widget::{IndexedLoopAction, TrackState, TracksWidget};
