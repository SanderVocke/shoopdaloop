#ifndef SHOOP_USB_MIDI_H
#define SHOOP_USB_MIDI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define SHOOP_MIDI_PORTS 16u
#define SHOOP_MIDI_BYTES 2048u
#define SHOOP_MIDI_QUEUE 256u
#define SHOOP_MIDI_ASSEMBLY_US 1000000u

typedef bool (*shoop_midi_emit)(void *, uint64_t, const uint8_t *, size_t);

struct shoop_midi_decoder {
    uint8_t bytes[SHOOP_MIDI_BYTES];
    uint16_t used;
    uint8_t running, needed;
    bool discarding;
    uint64_t started;
    uint32_t refused, dropped;
};

bool shoop_midi_valid(const uint8_t *bytes, size_t len);
void shoop_midi_decode(struct shoop_midi_decoder *, const uint8_t packet[4],
                      uint64_t now, shoop_midi_emit, void *context);
void shoop_midi_expire(struct shoop_midi_decoder *, uint64_t now);
size_t shoop_midi_packet(const uint8_t *bytes, size_t len, size_t offset,
                        uint8_t cable, uint8_t packet[4]);
uint32_t shoop_midi_offset(uint64_t arrival, uint64_t now, uint32_t frames, uint32_t rate);

#endif
