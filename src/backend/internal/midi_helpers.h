#pragma once

#include <cstddef>
#include <optional>
#include <stdint.h>
#include <vector>

uint32_t channel(const uint8_t *msg_data);

uint32_t note(const uint8_t *msg_data);

uint32_t velocity(const uint8_t *msg_data);

bool is_noteOn(const uint8_t *msg_data);

bool is_noteOff(const uint8_t *msg_data);

bool is_cc(const uint8_t *msg_data);

bool is_program(const uint8_t *msg_data);

bool is_channel_pressure(const uint8_t *msg_data);

bool is_pitch_wheel(const uint8_t *msg_data);

std::optional<uint32_t> is_all_notes_off_for_channel(const uint8_t *msg_data);

std::optional<uint32_t> is_all_sound_off_for_channel(const uint8_t *msg_data);

std::vector<uint8_t> noteOn(uint8_t channel, uint8_t note, uint8_t velocity);

std::vector<uint8_t> noteOff(uint8_t channel, uint8_t note, uint8_t velocity);

std::vector<uint8_t> cc(uint8_t channel, uint8_t controller, uint8_t value);

std::vector<uint8_t> programChange(uint8_t channel, uint8_t program);

std::vector<uint8_t> pitchWheelChange(uint8_t channel, uint16_t value);

std::vector<uint8_t> channelPressure(uint8_t channel, uint8_t value);