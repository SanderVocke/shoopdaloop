.import ShoopConstants 1.0 as SC

function is_playing_mode(mode) {
    return [SC.ShoopConstants.LoopMode.Playing, SC.ShoopConstants.LoopMode.PlayingDryThroughWet].includes(mode)
}

function is_recording_mode(mode) {
    return [SC.ShoopConstants.LoopMode.Recording, SC.ShoopConstants.LoopMode.RecordingDryIntoWet].includes(mode)
}

function is_recording_mode_for(loop_mode, channel_mode) {
    return (loop_mode == SC.ShoopConstants.LoopMode.Recording && channel_mode != SC.ShoopConstants.ChannelMode.Disabled) ||
        (loop_mode == SC.ShoopConstants.LoopMode.RecordingDryIntoWet && channel_mode == SC.ShoopConstants.ChannelMode.Wet)
}

function is_mode_with_predictable_end(mode) {
    return [SC.ShoopConstants.LoopMode.Playing, SC.ShoopConstants.LoopMode.PlayingDryThroughWet, SC.ShoopConstants.LoopMode.RecordingDryIntoWet].includes(mode)
}

function is_running_mode(mode) {
    return [
        SC.ShoopConstants.LoopMode.Playing,
        SC.ShoopConstants.LoopMode.Replacing,
        SC.ShoopConstants.LoopMode.Recording,
        SC.ShoopConstants.LoopMode.RecordingDryIntoWet,
        SC.ShoopConstants.LoopMode.PlayingDryThroughWet
    ].includes(mode)
}

function mode_name(mode) {
    return {
        [SC.ShoopConstants.LoopMode.Unknown]: "Unknown",
        [SC.ShoopConstants.LoopMode.Stopped]: "Stopped",
        [SC.ShoopConstants.LoopMode.Playing]: "Playing",
        [SC.ShoopConstants.LoopMode.Recording]: "Recording",
        [SC.ShoopConstants.LoopMode.PlayingDryThroughWet]: "Playing Dry Through Wet",
        [SC.ShoopConstants.LoopMode.RecordingDryIntoWet]: "Recording Dry Into Wet"
    }[mode];
}