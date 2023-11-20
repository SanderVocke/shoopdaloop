#include "channel_mode_helpers.h"
#include <iostream>
#include <algorithm>

unsigned loop_mode_to_channel_process_flags(
    loop_mode_t loop_mode,
    channel_mode_t channel_mode)
{
    // Map the "advanced modes" of the loop to the simple subset of
    // [Stopped, Playing, Replacing, Recording].
    if (channel_mode == ChannelMode_Disabled) {
        return 0;
    } else if (channel_mode == ChannelMode_Dry && loop_mode == LoopMode_Playing) {
        return 0;
    } else if (channel_mode == ChannelMode_Dry && (
        (loop_mode == LoopMode_PlayingDryThroughWet) ||
        (loop_mode == LoopMode_RecordingDryIntoWet)
    )) {
        return ChannelPlayback;
    } else if (channel_mode == ChannelMode_Wet && loop_mode == LoopMode_PlayingDryThroughWet) {
        return 0;
    } else if (channel_mode == ChannelMode_Wet && loop_mode == LoopMode_RecordingDryIntoWet) {
        return ChannelReplace;
    } else if (channel_mode == ChannelMode_Direct && loop_mode == LoopMode_PlayingDryThroughWet) {
        return ChannelPlayback;
    } else if (channel_mode == ChannelMode_Direct && loop_mode == LoopMode_RecordingDryIntoWet) {
        return ChannelReplace;
    }

    // Special cases handled, now the straightforward ones
    switch(loop_mode) {
        case LoopMode_Stopped:
            return 0;
        case LoopMode_Playing:
            return ChannelPlayback;
        case LoopMode_Recording:
            return ChannelRecord;
        case LoopMode_Replacing:
            return ChannelReplace;
        default:
            break;
    }

    return 0;
}

channel_process_params get_channel_process_params(
    loop_mode_t loop_mode,
    std::optional<loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta,
    int position,
    uint32_t start_offset,
    channel_mode_t channel_mode
    ) {
    channel_process_params rval {
        loop_mode_to_channel_process_flags(loop_mode, channel_mode),
        position + (int) start_offset };

    // Now, check if we should be pre-playing or pre-recording, in which case
    // we map again to Playing / Recording
    if (rval.process_flags == 0 && // Stopped
        maybe_next_mode.has_value() &&
        maybe_next_mode_eta.has_value() &&
        maybe_next_mode_delay_cycles.value_or(999) == 0 &&
        loop_mode_to_channel_process_flags(maybe_next_mode.value(), channel_mode) & ChannelPlayback) {
        // Possibly pre-play samples.
        rval.position = (int) start_offset - (int) maybe_next_mode_eta.value();
        rval.process_flags |= ChannelPlayback;
    } else if (!(rval.process_flags & ChannelRecord) && 
               maybe_next_mode.has_value() &&
               maybe_next_mode_eta.has_value() &&
               maybe_next_mode_delay_cycles.value_or(999) == 0 &&
               loop_mode_to_channel_process_flags(maybe_next_mode.value(), channel_mode) & ChannelRecord) {
        rval.process_flags |= ChannelPreRecord;
    }

    return rval;
}