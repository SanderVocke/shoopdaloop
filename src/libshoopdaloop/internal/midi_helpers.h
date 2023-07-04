#pragma once

#include <cstddef>
#include <stdint.h>
#include <optional>

inline size_t channel(const uint8_t *msg_data) {
    return msg_data[0] & 0x0F;
}

inline size_t note (const uint8_t *msg_data) {
    return msg_data[1];
}

inline size_t velocity (const uint8_t *msg_data) {
    return msg_data[2];
}

inline bool is_noteOn(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0x90;
}

inline bool is_noteOff(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0x80;
}

inline bool is_cc(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xB0;
}

inline bool is_program(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xC0;
}

inline bool is_channel_pressure(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xD0;
}

inline bool is_pitch_wheel(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xE0;
}

inline std::optional<size_t> is_all_notes_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 123) {
        return channel(msg_data);
    }
    return std::nullopt;
}

inline std::optional<size_t> is_all_sound_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 120) {
        return channel(msg_data);
    }
    return std::nullopt;
}