#pragma once
#include "types.h"

inline loop_mode_t modified_loop_mode_for_channel(loop_mode_t loop_mode, channel_mode_t channel_mode) {
    loop_mode_t modified_mode = loop_mode;
    if (channel_mode == Disabled) {
        modified_mode = Stopped;
    } else if (channel_mode == Dry && modified_mode == Playing) {
        modified_mode = Stopped;
    } else if (channel_mode == Dry && (
        (modified_mode == PlayingDryThroughWet) ||
        (modified_mode == RecordingDryIntoWet)
    )) {
        modified_mode = Playing;
    } else if (channel_mode == Wet && modified_mode == PlayingDryThroughWet) {
        modified_mode = Stopped;
    } else if (channel_mode == Wet && modified_mode == RecordingDryIntoWet) {
        modified_mode = Replacing;
    } else if (channel_mode == Direct && modified_mode == PlayingDryThroughWet) {
        modified_mode = Playing;
    } else if (channel_mode == Direct && modified_mode == RecordingDryIntoWet) {
        modified_mode = Replacing;
    }
    return modified_mode;
}