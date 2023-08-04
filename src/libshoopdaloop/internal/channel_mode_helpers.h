#pragma once
#include "types.h"
#include <cstddef>
#include <optional>

typedef enum {
    ChannelPlayback =  0b00000001,
    ChannelRecord =    0b00000010,
    ChannelPreRecord = 0b00000100,
    ChannelReplace =   0b00001000,
} channel_process_flags_t;

struct channel_process_params {
    unsigned process_flags;
    int position;
};

unsigned loop_mode_to_channel_process_flags(
    loop_mode_t loop_mode,
    channel_mode_t channel_mode);

channel_process_params get_channel_process_params(
    loop_mode_t loop_mode,
    std::optional<loop_mode_t> maybe_next_mode,
    std::optional<size_t> maybe_next_mode_delay_cycles,
    std::optional<size_t> maybe_next_mode_eta,
    int position,
    size_t start_offset,
    channel_mode_t channel_mode
    );