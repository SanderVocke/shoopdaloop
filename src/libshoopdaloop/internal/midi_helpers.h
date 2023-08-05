#pragma once

#include <cstddef>
#include <optional>
#include <stdint.h>

size_t channel(const uint8_t *msg_data);

size_t note(const uint8_t *msg_data);

size_t velocity(const uint8_t *msg_data);

bool is_noteOn(const uint8_t *msg_data);

bool is_noteOff(const uint8_t *msg_data);

bool is_cc(const uint8_t *msg_data);

bool is_program(const uint8_t *msg_data);

bool is_channel_pressure(const uint8_t *msg_data);

bool is_pitch_wheel(const uint8_t *msg_data);

std::optional<size_t> is_all_notes_off_for_channel(const uint8_t *msg_data);

std::optional<size_t> is_all_sound_off_for_channel(const uint8_t *msg_data);