/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "boot_state.h"
#include <string.h>

uint32_t shoop_crc32(const void *bytes, size_t size) {
    const uint8_t *p = bytes;
    uint32_t crc = UINT32_MAX;
    for (size_t i = 0; i < size; ++i) {
        crc ^= p[i];
        for (unsigned bit = 0; bit < 8; ++bit)
            crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
    }
    return ~crc;
}
void shoop_record_seal(void *record, uint32_t kind) {
    struct shoop_record_header *h = record;
    h->magic = SHOOP_RECORD_MAGIC; h->version = SHOOP_VERSION;
    h->kind = kind; h->reserved = 0;
    uint32_t crc = shoop_crc32(record, 508);
    memcpy((uint8_t *)record + 508, &crc, 4);
}
int shoop_record_valid(const void *record, uint32_t kind) {
    const struct shoop_record_header *h = record;
    uint32_t crc;
    memcpy(&crc, (const uint8_t *)record + 508, 4);
    return h->magic == SHOOP_RECORD_MAGIC && h->version == SHOOP_VERSION &&
        h->kind == kind && !h->reserved && crc == shoop_crc32(record, 508);
}
static int write_checked(const struct shoop_io *io, uint32_t lba, const void *record) {
    uint8_t check[512];
    return io->write(io->context, lba, record) || io->read(io->context, lba, check) ||
        memcmp(record, check, 512) ? SHOOP_IO : SHOOP_OK;
}
int shoop_config_load(const struct shoop_io *io, struct shoop_config *out) {
    struct shoop_config records[2];
    const uint32_t sectors[] = { SHOOP_CONFIG_0_LBA, SHOOP_CONFIG_1_LBA };
    int selected = -1;
    for (unsigned i = 0; i < 2; ++i) {
        if (io->read(io->context, sectors[i], &records[i]) || !shoop_record_valid(&records[i], 1)) continue;
        if (selected >= 0 && memcmp(&records[i], &records[selected], 512)) return SHOOP_CONFLICT;
        selected = (int)i;
    }
    if (selected < 0) return SHOOP_INVALID;
    *out = records[selected];
    uint8_t any = 0;
    for (unsigned i = 0; i < 32; ++i) any |= out->credential[i];
    if (!any || !out->ip || !out->netmask || (out->mac[0] & 1) ||
        memcmp(out->profile, SHOOP_PROFILE, sizeof(SHOOP_PROFILE))) return SHOOP_INVALID;
    return SHOOP_OK;
}
static int state_valid(const struct shoop_state *state) {
    return shoop_record_valid(state, 2) && state->generation &&
        (state->confirmed < 2 || state->confirmed == SHOOP_NO_SLOT) &&
        (state->pending < 2 || state->pending == SHOOP_NO_SLOT) &&
        (state->pending == SHOOP_NO_SLOT || state->pending != state->confirmed) &&
        state->attempts <= SHOOP_TRIAL_ATTEMPTS && state->mode <= SHOOP_BOOT_NETWORK;
}
int shoop_state_load(const struct shoop_io *io, struct shoop_state *out) {
    struct shoop_state records[2];
    const uint32_t sectors[] = { SHOOP_STATE_0_LBA, SHOOP_STATE_1_LBA };
    int selected = -1;
    for (unsigned i = 0; i < 2; ++i) {
        if (io->read(io->context, sectors[i], &records[i]) || !state_valid(&records[i])) continue;
        if (selected >= 0 && records[i].generation == records[selected].generation &&
            memcmp(&records[i], &records[selected], 512)) return SHOOP_CONFLICT;
        if (selected < 0 || records[i].generation > records[selected].generation) selected = (int)i;
    }
    if (selected < 0) return SHOOP_INVALID;
    *out = records[selected];
    return SHOOP_OK;
}
int shoop_state_store(const struct shoop_io *io, struct shoop_state *state) {
    if (state->generation == UINT64_MAX) return SHOOP_EXHAUSTED;
    ++state->generation;
    shoop_record_seal(state, 2);
    if (!state_valid(state)) return SHOOP_INVALID;
    int result = write_checked(io, SHOOP_STATE_0_LBA, state);
    if (!result) result = write_checked(io, SHOOP_STATE_1_LBA, state);
    return result;
}
int shoop_nonce_reserve(const struct shoop_io *io, uint64_t *out) {
    struct shoop_counter record, next = {0};
    const uint32_t sectors[] = { SHOOP_COUNTER_0_LBA, SHOOP_COUNTER_1_LBA };
    unsigned valid = 0;
    for (unsigned i = 0; i < 2; ++i) {
        if (io->read(io->context, sectors[i], &record) || !shoop_record_valid(&record, 3)) continue;
        ++valid;
        if (record.value > next.value) next.value = record.value;
    }
    if (!valid) return SHOOP_INVALID;
    if (next.value == UINT64_MAX) return SHOOP_EXHAUSTED;
    ++next.value;
    shoop_record_seal(&next, 3);
    for (unsigned i = 0; i < 2; ++i) {
        int result = write_checked(io, sectors[i], &next);
        if (result) return result;
    }
    *out = next.value;
    return SHOOP_OK;
}
int shoop_candidate(const struct shoop_io *io, struct shoop_state *state, uint32_t slot, const uint8_t *hash) {
    if (slot >= 2 || slot == state->confirmed || state->pending != SHOOP_NO_SLOT) return SHOOP_CONFLICT;
    state->pending = slot; state->attempts = SHOOP_TRIAL_ATTEMPTS;
    state->mode = SHOOP_BOOT_NORMAL;
    memcpy(state->pending_hash, hash, 32);
    return shoop_state_store(io, state);
}
int shoop_select(const struct shoop_io *io, struct shoop_state *state, uint32_t *slot) {
    if (state->pending != SHOOP_NO_SLOT) {
        if (state->attempts) {
            --state->attempts;
            int result = shoop_state_store(io, state);
            if (result) return result;
            *slot = state->pending;
            return SHOOP_OK;
        }
        state->pending = SHOOP_NO_SLOT;
        memset(state->pending_hash, 0, 32);
        int result = shoop_state_store(io, state);
        if (result) return result;
    }
    if (state->confirmed >= 2) return SHOOP_INVALID;
    *slot = state->confirmed;
    return SHOOP_OK;
}
int shoop_confirm(const struct shoop_io *io, uint32_t slot, const uint8_t *hash) {
    struct shoop_state state;
    int result = shoop_state_load(io, &state);
    if (result) return result;
    if (state.confirmed == slot && !memcmp(state.confirmed_hash, hash, 32))
        return shoop_state_store(io, &state);
    if (state.pending != slot || memcmp(state.pending_hash, hash, 32)) return SHOOP_CONFLICT;
    state.confirmed = slot; state.pending = SHOOP_NO_SLOT; state.attempts = 0;
    memcpy(state.confirmed_hash, hash, 32); memset(state.pending_hash, 0, 32);
    return shoop_state_store(io, &state);
}
int shoop_handoff_valid(const struct shoop_handoff *h) {
    return h->nonce && h->magic == SHOOP_HANDOFF_MAGIC && h->version == SHOOP_VERSION &&
        (h->slot < 2 || h->slot == SHOOP_NO_SLOT) && h->trial <= 1 &&
        h->crc == shoop_crc32(h, 508) && shoop_record_valid(&h->config, 1);
}
uint32_t shoop_slot_lba(uint32_t slot) {
    return slot == 0 ? SHOOP_SLOT_A_LBA : slot == 1 ? SHOOP_SLOT_B_LBA : 0;
}
