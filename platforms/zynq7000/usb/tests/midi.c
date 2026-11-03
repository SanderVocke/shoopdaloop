#include "midi.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

struct sink {
    uint8_t bytes[SHOOP_MIDI_BYTES];
    size_t len;
    unsigned calls;
    uint64_t at;
    bool full;
};

static bool emit(void *context, uint64_t at, const uint8_t *bytes, size_t len) {
    struct sink *sink = context;
    if (sink->full) return false;
    memcpy(sink->bytes, bytes, len);
    sink->len = len;
    sink->at = at;
    ++sink->calls;
    return true;
}

static void decode(struct shoop_midi_decoder *state, struct sink *sink,
                   uint8_t cin, uint8_t a, uint8_t b, uint8_t c) {
    shoop_midi_decode(state, (uint8_t[]){cin, a, b, c}, 100, emit, sink);
}

int main(void) {
    struct shoop_midi_decoder state = {0}, other = {0};
    struct sink sink = {0}, sink2 = {0};
    static const uint8_t messages[][3] = {
        {0x80, 60, 0}, {0x90, 64, 100}, {0xa0, 60, 1}, {0xb0, 7, 100},
        {0xc0, 5, 0}, {0xd0, 2, 0}, {0xe0, 0, 64}, {0xf1, 1, 0},
        {0xf2, 0, 1}, {0xf3, 1, 0}, {0xf6, 0, 0}, {0xf8, 0, 0},
        {0xfa, 0, 0}, {0xfb, 0, 0}, {0xfc, 0, 0}, {0xfe, 0, 0}, {0xff, 0, 0},
    };
    static const uint8_t lengths[] = {3,3,3,3,2,2,3,2,3,2,1,1,1,1,1,1,1};
    static const uint8_t cins[] = {8,9,10,11,12,13,14,2,3,2,5,15,15,15,15,15,15};
    for (unsigned cable = 0; cable < 16; ++cable) {
        for (unsigned i = 0; i < sizeof(lengths); ++i) {
            uint8_t packet[4];
            assert(shoop_midi_valid(messages[i], lengths[i]));
            assert(shoop_midi_packet(messages[i], lengths[i], 0, cable, packet) == lengths[i]);
            assert(packet[0] == (cable << 4 | cins[i]));
            shoop_midi_decode(&state, packet, 100, emit, &sink);
            assert(sink.len == lengths[i] && memcmp(sink.bytes, messages[i], sink.len) == 0);
        }
    }
    decode(&state, &sink, 4, 0xf0, 1, 2);
    decode(&other, &sink2, 4, 0xf0, 3, 4);
    decode(&state, &sink, 15, 0xf8, 0, 0);
    assert(sink.len == 1 && sink.bytes[0] == 0xf8 && state.used == 3);
    decode(&other, &sink2, 5, 0xf7, 0, 0);
    assert(sink2.len == 4 && sink2.bytes[1] == 3);
    decode(&state, &sink, 7, 5, 6, 0xf7);
    assert(sink.len == 6 && sink.bytes[1] == 1 && sink.bytes[5] == 0xf7);

    uint8_t sysex[SHOOP_MIDI_BYTES + 1], packet[4];
    memset(sysex, 42, sizeof(sysex)); sysex[0] = 0xf0;
    for (size_t len = 2; len <= sizeof(sysex); ++len) {
        memset(&state, 0, sizeof(state)); memset(&sink, 0, sizeof(sink));
        sysex[len - 1] = 0xf7;
        for (size_t pos = 0; pos < len;) {
            size_t count = len - pos > 3 ? 3 : len - pos;
            packet[0] = len - pos > 3 ? 4 : 4 + count;
            memset(packet + 1, 0, 3); memcpy(packet + 1, sysex + pos, count);
            shoop_midi_decode(&state, packet, 200, emit, &sink);
            if (len <= SHOOP_MIDI_BYTES) {
                uint8_t encoded[4];
                assert(shoop_midi_packet(sysex, len, pos, 0, encoded) == count);
                assert(memcmp(packet, encoded, 4) == 0);
            }
            pos += count;
        }
        if (len <= SHOOP_MIDI_BYTES) {
            assert(sink.calls == 1 && sink.len == len && memcmp(sink.bytes, sysex, len) == 0);
        } else assert(!sink.calls && state.refused);
        sysex[len - 1] = 42;
    }
    memset(&state, 0, sizeof(state)); memset(&sink, 0, sizeof(sink));
    decode(&state, &sink, 4, 0xf0, 1, 2);
    shoop_midi_expire(&state, 100 + SHOOP_MIDI_ASSEMBLY_US);
    assert(!state.used && state.refused == 1);
    decode(&state, &sink, 8, 0x90, 60, 1);
    assert(state.refused == 2 && !sink.calls);
    decode(&state, &sink, 9, 0x90, 60, 1);
    assert(sink.calls == 1);
    decode(&state, &sink, 15, 61, 0, 0);
    decode(&state, &sink, 15, 2, 0, 0);
    assert(sink.calls == 2 && sink.bytes[0] == 0x90 && sink.bytes[1] == 61);
    decode(&state, &sink, 4, 0x90, 60, 100);
    decode(&state, &sink, 7, 0xc0, 1, 0xf7);
    assert(sink.calls == 2 && state.refused == 4);
    sink.full = true;
    decode(&state, &sink, 15, 0xf8, 0, 0);
    assert(state.dropped == 1);
    assert(!shoop_midi_valid((uint8_t[]){0xf0, 0xf8, 0xf7}, 3));
    assert(!shoop_midi_valid((uint8_t[]){0x90, 0x80, 1}, 3));
    assert(!shoop_midi_packet(messages[0], 3, 0, 16, packet));
    assert(shoop_midi_offset(1000, 1000, 256, 48000) == 255);
    assert(shoop_midi_offset(0, 1000, 256, 48000) == 207);
    assert(shoop_midi_offset(0, 6000, 256, 48000) == 0);
    assert(shoop_midi_offset(0, UINT64_MAX, 256, 48000) == 0);
    assert(shoop_midi_offset(0, 0, 0, 48000) == 0);
    puts("USB MIDI golden vectors, all cable indices, SysEx bounds, isolation and timing passed");
    return 0;
}
