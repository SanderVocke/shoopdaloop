.import ShoopDaLoop.Rust 1.0 as SC

function is_playing_mode(mode) {
    return [SC.ShoopRustConstants.LoopMode.Playing, SC.ShoopRustConstants.LoopMode.PlayingDryThroughWet].includes(mode)
}

function is_recording_mode(mode) {
    return [SC.ShoopRustConstants.LoopMode.Recording, SC.ShoopRustConstants.LoopMode.RecordingDryIntoWet].includes(mode)
}

function is_recording_mode_for(loop_mode, channel_mode) {
    return (loop_mode == SC.ShoopRustConstants.LoopMode.Recording && channel_mode != SC.ShoopRustConstants.ChannelMode.Disabled) ||
        (loop_mode == SC.ShoopRustConstants.LoopMode.RecordingDryIntoWet && channel_mode == SC.ShoopRustConstants.ChannelMode.Wet)
}

function is_mode_with_predictable_end(mode) {
    return [SC.ShoopRustConstants.LoopMode.Playing, SC.ShoopRustConstants.LoopMode.PlayingDryThroughWet, SC.ShoopRustConstants.LoopMode.RecordingDryIntoWet].includes(mode)
}

function is_running_mode(mode) {
    return [
        SC.ShoopRustConstants.LoopMode.Playing,
        SC.ShoopRustConstants.LoopMode.Replacing,
        SC.ShoopRustConstants.LoopMode.Recording,
        SC.ShoopRustConstants.LoopMode.RecordingDryIntoWet,
        SC.ShoopRustConstants.LoopMode.PlayingDryThroughWet
    ].includes(mode)
}

function mode_name(mode) {
    return {
        [SC.ShoopRustConstants.LoopMode.Unknown]: "Unknown",
        [SC.ShoopRustConstants.LoopMode.Stopped]: "Stopped",
        [SC.ShoopRustConstants.LoopMode.Playing]: "Playing",
        [SC.ShoopRustConstants.LoopMode.Recording]: "Recording",
        [SC.ShoopRustConstants.LoopMode.PlayingDryThroughWet]: "Playing Dry Through Wet",
        [SC.ShoopRustConstants.LoopMode.RecordingDryIntoWet]: "Recording Dry Into Wet"
    }[mode];
}