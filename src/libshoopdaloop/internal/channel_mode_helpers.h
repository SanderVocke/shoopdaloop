#pragma once
#include "types.h"
#include <cstddef>
#include <optional>
#include <stdint.h>


typedef enum {
    ChannelPlayback =  0b00000001,
    ChannelRecord =    0b00000010,
    ChannelPreRecord = 0b00000100,
    ChannelReplace =   0b00001000,
} channel_process_flags_t;

struct channel_process_params {
    unsigned process_flags = 0;
    int position = 0;
};

unsigned loop_mode_to_channel_process_flags(
    shoop_loop_mode_t loop_mode,
    shoop_channel_mode_t channel_mode);

channel_process_params get_channel_process_params(
    shoop_loop_mode_t loop_mode,
    std::optional<shoop_loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta,
    int position,
    uint32_t start_offset,
    shoop_channel_mode_t channel_mode
    );