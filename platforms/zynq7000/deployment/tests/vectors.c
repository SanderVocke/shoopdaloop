/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "shoop_boot_state.h"
#include <stdio.h>
#include <string.h>
int main(void) {
    const uint8_t hash[32] = {0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,
        0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad};
    struct shoop_config config = { .mac = {2,0,0,0,0,1}, .ip = 0x0f02000a,
        .netmask = 0x00ffffff, .gateway = 0x0202000a, .server = 0x0202000a };
    for (unsigned i = 0; i < 32; ++i) config.credential[i] = i;
    memcpy(config.profile, SHOOP_PROFILE, sizeof(SHOOP_PROFILE));
    shoop_record_seal(&config, 1);
    struct shoop_state state = { .generation = 1, .confirmed = 0, .pending = SHOOP_NO_SLOT, .fallback = SHOOP_NO_SLOT };
    memcpy(state.confirmed_hash, hash, 32); shoop_record_seal(&state, 2);
    struct shoop_counter counter = { .value = 42 }; shoop_record_seal(&counter, 3);
    struct shoop_slot slot = { .bundle_bytes = 3, .build = "firmware-a" };
    memcpy(slot.bundle_hash, hash, 32); shoop_record_seal(&slot, 4);
    return fwrite(&config, 512, 1, stdout) != 1 || fwrite(&state, 512, 1, stdout) != 1 ||
        fwrite(&counter, 512, 1, stdout) != 1 || fwrite(&slot, 512, 1, stdout) != 1;
}
