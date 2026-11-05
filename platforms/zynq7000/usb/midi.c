#include "midi.h"
#include <string.h>

static unsigned message_size(uint8_t status) {
    if (status >= 0x80 && status <= 0xef) return (status & 0xe0) == 0xc0 ? 2 : 3;
    switch (status) {
    case 0xf1: case 0xf3: return 2;
    case 0xf2: return 3;
    case 0xf6: case 0xf8: case 0xfa: case 0xfb: case 0xfc: case 0xfe: case 0xff: return 1;
    default: return 0;
    }
}

bool shoop_midi_valid(const uint8_t *bytes, size_t len) {
    if (!bytes || !len || len > SHOOP_MIDI_BYTES) return false;
    if (bytes[0] == 0xf0) {
        if (len < 2 || bytes[len - 1] != 0xf7) return false;
        for (size_t i = 1; i + 1 < len; ++i) if (bytes[i] & 0x80) return false;
        return true;
    }
    if (message_size(bytes[0]) != len) return false;
    for (size_t i = 1; i < len; ++i) if (bytes[i] & 0x80) return false;
    return true;
}

static void emit_message(struct shoop_midi_decoder *state, uint64_t now,
                         const uint8_t *bytes, size_t len, shoop_midi_emit emit, void *context) {
    if (!emit(context, now, bytes, len)) ++state->dropped;
}

void shoop_midi_expire(struct shoop_midi_decoder *state, uint64_t now) {
    if ((state->used || state->discarding) && now - state->started >= SHOOP_MIDI_ASSEMBLY_US) {
        state->used = state->needed = state->running = 0;
        state->discarding = false;
        ++state->refused;
    }
}

static void decode_byte(struct shoop_midi_decoder *state, uint8_t byte, uint64_t now,
                        shoop_midi_emit emit, void *context) {
    if (byte >= 0xf8) {
        if (message_size(byte)) emit_message(state, now, &byte, 1, emit, context);
        else ++state->refused;
        return;
    }
    if (state->discarding) {
        if (byte == 0xf7) { state->discarding = false; return; }
        if (!(byte & 0x80)) return;
        state->discarding = false;
    }
    if (state->used && state->bytes[0] == 0xf0) {
        if (!(byte & 0x80) || byte == 0xf7) {
            if (state->used == SHOOP_MIDI_BYTES) {
                state->used = 0;
                state->discarding = byte != 0xf7;
                ++state->refused;
                return;
            }
            state->bytes[state->used++] = byte;
            if (byte == 0xf7) {
                emit_message(state, now, state->bytes, state->used, emit, context);
                state->used = 0;
            }
            return;
        }
        state->used = 0;
        ++state->refused;
    }
    if (byte & 0x80) {
        if (state->used) ++state->refused;
        state->used = 0;
        state->running = byte < 0xf0 ? byte : 0;
        state->started = now;
        if (byte == 0xf0) {
            state->bytes[state->used++] = byte;
            state->needed = 0;
            return;
        }
        state->needed = message_size(byte);
        if (!state->needed) { ++state->refused; return; }
        state->bytes[state->used++] = byte;
    } else {
        if (!state->used) {
            if (!state->running) { ++state->refused; return; }
            state->bytes[state->used++] = state->running;
            state->needed = message_size(state->running);
            state->started = now;
        }
        state->bytes[state->used++] = byte;
    }
    if (state->used == state->needed) {
        emit_message(state, now, state->bytes, state->used, emit, context);
        state->used = 0;
    }
}

void shoop_midi_decode(struct shoop_midi_decoder *state, const uint8_t packet[4],
                      uint64_t now, shoop_midi_emit emit, void *context) {
    static const uint8_t lengths[16] = {0, 0, 2, 3, 3, 1, 2, 3, 3, 3, 3, 3, 2, 2, 3, 1};
    shoop_midi_expire(state, now);
    unsigned cin = packet[0] & 15, count = lengths[cin];
    if (!count) {
        if (packet[0] || packet[1] || packet[2] || packet[3]) ++state->refused;
        return;
    }
    bool valid = true;
    if (cin >= 8 && cin <= 14) {
        valid = (packet[1] >> 4) == cin;
        for (unsigned i = 2; i <= count; ++i) valid &= packet[i] < 0x80;
    } else if (cin == 2) {
        valid = (packet[1] == 0xf1 || packet[1] == 0xf3) && packet[2] < 0x80;
    } else if (cin == 3) {
        valid = packet[1] == 0xf2 && packet[2] < 0x80 && packet[3] < 0x80;
    } else if (cin >= 4 && cin <= 7) {
        bool assembling = (state->used && state->bytes[0] == 0xf0) || state->discarding;
        if (cin == 5 && packet[1] == 0xf6) valid = true;
        else {
            valid = packet[1] == 0xf0 || assembling;
            unsigned data_end = cin == 4 ? count : count - 1;
            for (unsigned i = 1; i <= data_end; ++i)
                valid &= packet[i] < 0x80 || (i == 1 && packet[i] == 0xf0);
            if (cin != 4) valid &= packet[count] == 0xf7;
        }
    }
    if (!valid) {
        state->used = state->needed = state->running = 0;
        state->discarding = false;
        ++state->refused;
        return;
    }
    for (unsigned i = 1; i <= count; ++i) decode_byte(state, packet[i], now, emit, context);
}

size_t shoop_midi_packet(const uint8_t *bytes, size_t len, size_t offset,
                        uint8_t cable, uint8_t packet[4]) {
    if (!bytes || !len || len > SHOOP_MIDI_BYTES || offset >= len || cable > 15) return 0;
    if (!offset && !shoop_midi_valid(bytes, len)) return 0;
    size_t remaining = len - offset, count = remaining < 3 ? remaining : 3;
    uint8_t cin;
    if (bytes[0] == 0xf0) cin = remaining > 3 ? 4 : (uint8_t)(4 + count);
    else {
        if (offset || !shoop_midi_valid(bytes, len)) return 0;
        uint8_t status = bytes[0];
        cin = status < 0xf0 ? status >> 4 : status >= 0xf8 ? 15 :
              status == 0xf6 ? 5 : len == 2 ? 2 : 3;
    }
    packet[0] = (uint8_t)((cable << 4) | cin);
    memset(packet + 1, 0, 3);
    memcpy(packet + 1, bytes + offset, count);
    return count;
}

uint32_t shoop_midi_offset(uint64_t arrival, uint64_t now, uint32_t frames, uint32_t rate) {
    if (!frames || !rate || (now > arrival && now - arrival >= 1000000)) return 0;
    uint64_t age = now > arrival ? (now - arrival) * rate / 1000000 : 0;
    return age >= frames ? 0 : frames - 1 - (uint32_t)age;
}
