/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "shoop_boot_state.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static uint8_t disk[6][512];
static unsigned writes, fail_at;
static const uint32_t sectors[] = {
    SHOOP_CONFIG_0_LBA, SHOOP_CONFIG_1_LBA, SHOOP_STATE_0_LBA,
    SHOOP_STATE_1_LBA, SHOOP_COUNTER_0_LBA, SHOOP_COUNTER_1_LBA,
};
static int index_of(uint32_t lba) {
    for (int i = 0; i < 6; ++i) if (sectors[i] == lba) return i;
    return -1;
}
static int read_sector(void *context, uint32_t lba, void *out) {
    (void)context;
    int index = index_of(lba);
    if (index < 0) return -1;
    memcpy(out, disk[index], 512);
    return 0;
}
static int write_sector(void *context, uint32_t lba, const void *in) {
    (void)context;
    int index = index_of(lba);
    if (index < 0) return -1;
    if (++writes == fail_at) {
        memcpy(disk[index], in, 199);
        return -1;
    }
    memcpy(disk[index], in, 512);
    return 0;
}
static struct shoop_io io = { NULL, read_sector, write_sector };
static const uint8_t a[32] = { 0xaa }, b[32] = { 0xbb };
static void initialize(void) {
    memset(disk, 0, sizeof disk); writes = fail_at = 0;
    struct shoop_state state = { .generation = 1, .confirmed = 0, .pending = SHOOP_NO_SLOT };
    memcpy(state.confirmed_hash, a, 32);
    shoop_record_seal(&state, 2);
    memcpy(disk[2], &state, 512); memcpy(disk[3], &state, 512);
    struct shoop_counter counter = { .value = 41 };
    shoop_record_seal(&counter, 3);
    memcpy(disk[4], &counter, 512); memcpy(disk[5], &counter, 512);
}
static void nonce_faults(void) {
    for (unsigned cut = 1; cut <= 2; ++cut) {
        initialize(); fail_at = cut;
        uint64_t nonce = 0;
        assert(shoop_nonce_reserve(&io, &nonce) == SHOOP_IO && nonce == 0);
        fail_at = 0;
        assert(!shoop_nonce_reserve(&io, &nonce) && nonce >= 42);
        uint64_t used = nonce;
        memset(disk[4], 0xff, 512);
        assert(!shoop_nonce_reserve(&io, &nonce) && nonce > used);
        used = nonce;
        memset(disk[5], 0xff, 512);
        assert(!shoop_nonce_reserve(&io, &nonce) && nonce > used);
    }
    memset(disk[4], 0, 512); memset(disk[5], 0, 512);
    uint64_t nonce = 0;
    assert(shoop_nonce_reserve(&io, &nonce) == SHOOP_INVALID);
    struct shoop_counter counter = { .value = UINT64_MAX };
    shoop_record_seal(&counter, 3); memcpy(disk[4], &counter, 512);
    assert(shoop_nonce_reserve(&io, &nonce) == SHOOP_EXHAUSTED);
}
static void trials_and_confirmation(void) {
    initialize();
    struct shoop_state state;
    assert(!shoop_state_load(&io, &state));
    assert(!shoop_candidate(&io, &state, 1, b));
    assert(shoop_candidate(&io, &state, 0, a) == SHOOP_CONFLICT);
    uint64_t previous = 0;
    for (unsigned boot = 0; boot < 3; ++boot) {
        uint64_t nonce; uint32_t slot;
        assert(!shoop_state_load(&io, &state));
        assert(!shoop_select(&io, &state, &slot));
        assert(slot == (boot < 2 ? 1u : 0u));
        assert(!shoop_nonce_reserve(&io, &nonce) && nonce > previous);
        previous = nonce;
    }
    assert(!memcmp(state.confirmed_hash, a, 32));
    assert(!shoop_candidate(&io, &state, 1, b));
    assert(shoop_confirm(&io, 1, a) == SHOOP_CONFLICT);
    assert(!shoop_confirm(&io, 1, b));
    assert(!shoop_confirm(&io, 1, b));
    memset(disk[2], 0, 512);
    assert(!shoop_state_load(&io, &state) && state.confirmed == 1 && state.pending == SHOOP_NO_SLOT);
}
static void publication_faults(void) {
    for (unsigned cut = 1; cut <= 2; ++cut) {
        initialize();
        struct shoop_state state;
        assert(!shoop_state_load(&io, &state));
        fail_at = cut;
        assert(shoop_candidate(&io, &state, 1, b) == SHOOP_IO);
        assert(!shoop_state_load(&io, &state));
        assert(state.confirmed == 0 && !memcmp(state.confirmed_hash, a, 32));
        assert(state.pending == SHOOP_NO_SLOT ||
               (state.pending == 1 && !memcmp(state.pending_hash, b, 32)));
    }
    initialize();
    struct shoop_state state;
    assert(!shoop_state_load(&io, &state));
    state.header.version++;
    memcpy(disk[2], &state, 512); memset(disk[3], 0xff, 512);
    assert(shoop_state_load(&io, &state) == SHOOP_INVALID);
}
int main(void) {
    assert(shoop_crc32("123456789", 9) == 0xcbf43926);
    nonce_faults(); trials_and_confirmation(); publication_faults();
    puts("Boot state: nonce durability, trial rollback, confirmation and torn writes passed");
}
