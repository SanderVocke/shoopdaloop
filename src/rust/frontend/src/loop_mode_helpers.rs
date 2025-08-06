use backend_bindings::LoopMode;

pub fn is_playing_mode(mode: LoopMode) -> bool {
    matches!(mode, LoopMode::Playing | LoopMode::PlayingDryThroughWet)
}

pub fn is_recording_mode(mode: LoopMode) -> bool {
    matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet)
}

pub fn is_running_mode(mode: LoopMode) -> bool {
    matches!(
        mode,
        LoopMode::Playing
            | LoopMode::Replacing
            | LoopMode::Recording
            | LoopMode::RecordingDryIntoWet
            | LoopMode::PlayingDryThroughWet
    )
}
