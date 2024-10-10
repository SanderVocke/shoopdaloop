#include "midi_helpers.h"

uint32_t channel(const uint8_t *msg_data) { return msg_data[0] & 0x0F; }

uint32_t note(const uint8_t *msg_data) { return msg_data[1]; }

uint32_t velocity(const uint8_t *msg_data) { return msg_data[2]; }

bool is_noteOn(const uint8_t *msg_data) { return (msg_data[0] & 0xF0) == 0x90; }

bool is_noteOff(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0x80;
}

bool is_cc(const uint8_t *msg_data) { return (msg_data[0] & 0xF0) == 0xB0; }

bool is_program(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xC0;
}

bool is_channel_pressure(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xD0;
}

bool is_pitch_wheel(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xE0;
}

std::optional<uint32_t> is_all_notes_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 123) {
        return channel(msg_data);
    }
    return std::nullopt;
}

std::optional<uint32_t> is_all_sound_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 120) {
        return channel(msg_data);
    }
    return std::nullopt;
}

std::vector<uint8_t> noteOn(uint8_t channel, uint8_t note, uint8_t velocity) {
     return { static_cast<unsigned char>(0x90 + channel), note, velocity };
}

std::vector<uint8_t> noteOff(uint8_t channel, uint8_t note, uint8_t velocity) {
    return { static_cast<unsigned char>(0x80 + channel), note, velocity };
}

std::vector<uint8_t> cc(uint8_t channel, uint8_t controller, uint8_t value) {
    return { static_cast<unsigned char>(0xB0 + channel), controller, value };
}

std::vector<uint8_t> programChange(uint8_t channel, uint8_t program) {
    return { static_cast<unsigned char>(0xC0 + channel), program };
}

std::vector<uint8_t> pitchWheelChange(uint8_t channel, uint16_t value) {
    uint8_t first = (uint8_t)(value & 0b1111111);
    uint8_t second = (uint8_t)((value >> 8) & 0b1111111);
    return { static_cast<unsigned char>(0xE0 + channel), first, second };
}

std::vector<uint8_t> channelPressure(uint8_t channel, uint8_t value) {
    return { static_cast<unsigned char>(0xD0 + channel), value };
}