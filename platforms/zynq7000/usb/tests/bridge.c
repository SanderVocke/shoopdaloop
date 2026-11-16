#include "bridge.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static void input_recovery(void) {
    uint8_t bytes[3]; uint64_t arrival;
    for (unsigned i = 0; i < 32; ++i) {
        assert(shoop_usb_read(0, bytes, sizeof(bytes), &arrival) == 3);
        assert(!memcmp(bytes, (uint8_t[]){0xb0 | (i / 2),i % 2 ? 123 : 120,0}, 3));
    }
}

int main(void) {
    assert(shoop_usb_mount(0, 1, 1, 1, 1, 0xcafe, 0x4010, 2, "usb0/1:fixture", "Synth"));
    assert(shoop_usb_input_epoch(0) == 0 && shoop_usb_begin_input(0) == 0);
    assert(shoop_usb_output_capacity(0) == SHOOP_MIDI_QUEUE);
    struct shoop_usb_port_info info;
    assert(shoop_usb_port_snapshot(0, 0, &info));
    assert(info.connected && info.generation == 1 && !strcmp(info.name, "Synth in 1"));
    uint8_t packet[4], bytes[SHOOP_MIDI_BYTES];
    uint64_t arrival;
    shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,60,100}, 1234);
    assert(shoop_usb_read(0, bytes, sizeof(bytes), &arrival) == 3);
    assert(arrival == 1234 && !memcmp(bytes, (uint8_t[]){0x90,60,100}, 3));
    struct shoop_usb_packet_owner owner, old;
    assert(shoop_usb_write(0, (uint8_t[]){0x80,60,0}, 3, 2000));
    assert(shoop_usb_output_capacity(0) == SHOOP_MIDI_QUEUE - 1);
    assert(!shoop_usb_prepare_packet(0, 1999, packet, &owner));
    assert(shoop_usb_prepare_packet(0, 2000, packet, &owner));
    assert(!memcmp(packet, (uint8_t[]){8,0x80,60,0}, 4));
    assert(owner.last && !owner.recovery);
    shoop_usb_accept_packet(owner);
    shoop_usb_complete_packet(owner);
    assert(shoop_usb_drained());
    assert(shoop_usb_output_capacity(0) == SHOOP_MIDI_QUEUE);

    shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,61,100}, 1235);
    assert(shoop_usb_write(0, (uint8_t[]){0x90,61,100}, 3, 2000));
    assert(shoop_usb_prepare_packet(0, 2000, packet, &old));
    shoop_usb_accept_packet(old);
    shoop_usb_unmount(0);
    input_recovery();
    assert(!shoop_usb_read(0, bytes, sizeof(bytes), &arrival));
    assert(!shoop_usb_write(0, (uint8_t[]){0x80,61,0}, 3, 0));
    assert(shoop_usb_mount(0, 2, 1, 1, 1, 0xcafe, 0x4010, 2, "usb0/1:fixture", "Synth"));
    assert(shoop_usb_port_snapshot(1, 0, &info) && info.generation == 2);
    shoop_usb_complete_packet(old);
    shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,62,100}, 3000);
    assert(shoop_usb_read(0, bytes, sizeof(bytes), &arrival) == 3 && bytes[1] == 62);
    for (unsigned i = 0; i < SHOOP_MIDI_QUEUE + 1; ++i)
        shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,63,100}, 3000);
    input_recovery();
    assert(!shoop_usb_read(0, bytes, sizeof(bytes), &arrival));
    shoop_usb_receive_packet(0, (uint8_t[]){8,0x80,63,0}, 3001);
    assert(shoop_usb_read(0, bytes, sizeof(bytes), &arrival) == 3 && bytes[0] == 0x80);

    memset(bytes, 42, sizeof(bytes)); bytes[0] = 0xf0; bytes[sizeof(bytes) - 1] = 0xf7;
    assert(shoop_usb_write(0, bytes, sizeof(bytes), 0));
    unsigned count = 0;
    while (shoop_usb_prepare_packet(0, 3000, packet, &owner)) {
        shoop_usb_accept_packet(owner); shoop_usb_complete_packet(owner);
        ++count;
    }
    assert(count == (SHOOP_MIDI_BYTES + 2) / 3);
    for (unsigned i = 0; i < SHOOP_MIDI_QUEUE; ++i) {
        assert(shoop_usb_write(0, (uint8_t[]){0x90,63,100}, 3, 0));
    }
    assert(!shoop_usb_write(0, (uint8_t[]){0x90,64,100}, 3, 0));
    assert(shoop_usb_output_capacity(0) == 0);
    struct shoop_usb_packet_owner resets[32];
    for (unsigned i = 0; i < 32; ++i) {
        assert(!shoop_usb_drained());
        assert(shoop_usb_prepare_packet(0, 3000, packet, &resets[i]));
        assert(resets[i].recovery);
        assert(!memcmp(packet, (uint8_t[]){11,0xb0 | (i / 2),i % 2 ? 123 : 120,0}, 4));
        shoop_usb_accept_packet(resets[i]);
    }
    assert(!shoop_usb_prepare_packet(0, 3000, packet, &owner));
    assert(!shoop_usb_drained());
    for (unsigned i = 0; i < 31; ++i) shoop_usb_complete_packet(resets[i]);
    assert(!shoop_usb_drained());
    shoop_usb_complete_packet(resets[31]);
    assert(shoop_usb_drained());
    assert(shoop_usb_write(0, (uint8_t[]){0x90,65,100}, 3, 0));
    shoop_usb_recover_all();
    assert(!shoop_usb_drained());
    shoop_usb_unmount(0);
    assert(shoop_usb_drained());
    struct shoop_usb_counters counters;
    assert(shoop_usb_counters(0,0,&counters) && counters.dropped==SHOOP_MIDI_QUEUE+2 && !counters.queued);
    assert(shoop_usb_counters(1,0,&counters) && counters.dropped==SHOOP_MIDI_QUEUE+2 && !counters.queued);
    assert(shoop_usb_mount(1, 3, 1, 15, 15, 0xcafe, 0x4010, 0, "usb0/2:fixture", "Interface"));
    assert(!shoop_usb_mount(2, 4, 1, 1, 1, 0xcafe, 0x4010, 0, "usb0/3:excess", "Excess"));
    assert(!shoop_usb_mount(2, 4, 1, 17, 1, 0xcafe, 0x4010, 0, "usb0/3:excess", "Excess"));
    assert(shoop_usb_mount(0, 5, 1, 1, 1, 0xcafe, 0x4010, 2, "usb0/1:fixture", "Synth"));
    uint32_t baseline = shoop_usb_begin_input(0);
    assert(baseline == shoop_usb_input_epoch(0));
    assert(!shoop_usb_read(0, bytes, sizeof(bytes), &arrival));
    shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,64,100}, 4000);
    assert(shoop_usb_read(0, bytes, sizeof(bytes), &arrival) == 3 && bytes[1] == 64);
    for (unsigned i = 0; i < SHOOP_MIDI_QUEUE + 1; ++i)
        shoop_usb_receive_packet(0, (uint8_t[]){9,0x90,64,100}, 4000);
    assert(shoop_usb_input_epoch(0) != baseline);
    assert(shoop_usb_counters(0, 0, &counters));
    uint32_t refused = counters.refused;
    shoop_usb_refuse_input(0);
    assert(shoop_usb_counters(0, 0, &counters) && counters.refused == refused + 1);
    puts("USB MIDI admission, queue bounds, generations, scheduling, counted loss and completed recovery passed");
    return 0;
}
