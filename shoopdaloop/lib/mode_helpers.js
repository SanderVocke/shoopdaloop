.import 'backend/frontend_interface/types.js' as Types

function is_playing_mode(mode) {
    return [Types.LoopMode.Playing, Types.LoopMode.PlayingDryThroughWet].includes(mode)
}

function is_recording_mode(mode) {
    return [Types.LoopMode.Recording, Types.LoopMode.RecordingDryIntoWet].includes(mode)
}

function mode_name(mode) {
    return {
        [Types.LoopMode.Unknown]: "Unknown",
        [Types.LoopMode.Stopped]: "Stopped",
        [Types.LoopMode.Playing]: "Playing",
        [Types.LoopMode.Recording]: "Recording",
        [Types.LoopMode.PlayingDryThroughWet]: "Playing Dry Through Wet",
        [Types.LoopMode.RecordingDryIntoWet]: "Recording Dry Into Wet"
    }[mode];
}