#pragma once
#include "types.h"
#include <cstddef>
#include <optional>
#include <algorithm>

typedef enum {
    ChannelPlayback =  0b00000001,
    ChannelRecord =    0b00000010,
    ChannelPreRecord = 0b00000100,
    ChannelReplace =   0b00001000,
} channel_process_flags_t;

struct channel_process_params {
    unsigned process_flags;
    int position;
    size_t length;
};

inline unsigned loop_mode_to_channel_process_flags(
    loop_mode_t loop_mode,
    channel_mode_t channel_mode)
{
    // Map the "advanced modes" of the loop to the simple subset of
    // [Stopped, Playing, Replacing, Recording].
    if (channel_mode == Disabled) {
        return 0;
    } else if (channel_mode == Dry && loop_mode == Playing) {
        return 0;
    } else if (channel_mode == Dry && (
        (loop_mode == PlayingDryThroughWet) ||
        (loop_mode == RecordingDryIntoWet)
    )) {
        return ChannelPlayback;
    } else if (channel_mode == Wet && loop_mode == PlayingDryThroughWet) {
        return 0;
    } else if (channel_mode == Wet && loop_mode == RecordingDryIntoWet) {
        return ChannelReplace;
    } else if (channel_mode == Direct && loop_mode == PlayingDryThroughWet) {
        return ChannelPlayback;
    } else if (channel_mode == Direct && loop_mode == RecordingDryIntoWet) {
        return ChannelReplace;
    }

    return loop_mode;
}

inline channel_process_params modified_process_inputs_for_channel(
    loop_mode_t loop_mode,
    std::optional<std::pair<loop_mode_t, size_t>> maybe_next_mode,
    int position,
    size_t length,
    size_t start_offset,
    channel_mode_t channel_mode
    ) {
    channel_process_params rval {
        loop_mode_to_channel_process_flags(loop_mode, channel_mode),
        position, length };

    // Now, check if we should be pre-playing or pre-recording, in which case
    // we map again to Playing / Recording
    if (rval.process_flags == 0 && // Stopped
        maybe_next_mode &&
        loop_mode_to_channel_process_flags(maybe_next_mode->first, channel_mode) & ChannelPlayback) {
        // Possibly pre-play samples.
        rval.position = (int) start_offset - (int) maybe_next_mode->second;
        rval.process_flags |= ChannelPlayback;
    } else if (!(rval.process_flags & ChannelRecord) && 
               maybe_next_mode &&
               loop_mode_to_channel_process_flags(maybe_next_mode->first, channel_mode) & ChannelRecord) {
        rval.process_flags |= ChannelPreRecord;
        ///?
    }

    return rval;
}