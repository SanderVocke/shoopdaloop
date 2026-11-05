/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef SHOOP_BOOT_STATE_H
#define SHOOP_BOOT_STATE_H
#ifdef __UBOOT__
#include <linux/types.h>
#ifndef UINT32_MAX
#define UINT32_MAX 0xffffffffu
#endif
#ifndef UINT64_MAX
#define UINT64_MAX 0xffffffffffffffffull
#endif
#else
#include <stdint.h>
#endif
#include <stddef.h>
#include "shoop_layout.h"

#define SHOOP_RECORD_MAGIC 0x53484450u
#define SHOOP_HANDOFF_MAGIC 0x53484f46u
#define SHOOP_NO_SLOT UINT32_MAX
#define SHOOP_BOOT_NORMAL 0u
#define SHOOP_BOOT_UPDATE 1u
#define SHOOP_BOOT_NETWORK 2u
#define SHOOP_TRIAL_ATTEMPTS 2u
#define SHOOP_HANDOFF_ADDRESS 0x8000u
#define SHOOP_STAGE_ADDRESS 0x38000000u
#define SHOOP_RECORD_BYTES 512u

struct shoop_record_header { uint32_t magic, version, kind, reserved; };
struct shoop_config {
    struct shoop_record_header header;
    uint8_t mac[6], pad[2];
    uint32_t ip, netmask, gateway, server;
    uint8_t credential[32];
    char profile[32];
    uint8_t reserved[404];
    uint32_t crc;
};
struct shoop_state {
    struct shoop_record_header header;
    uint64_t generation;
    uint32_t confirmed, pending, attempts, mode;
    uint8_t confirmed_hash[32], pending_hash[32], operation_client[16];
    uint64_t operation_sequence;
    uint32_t boot_failures, fallback;
    uint8_t fallback_hash[32];
    uint8_t reserved[340];
    uint32_t crc;
};
struct shoop_counter {
    struct shoop_record_header header;
    uint64_t value;
    uint8_t reserved[484];
    uint32_t crc;
};
struct shoop_slot {
    struct shoop_record_header header;
    uint32_t bundle_bytes;
    uint8_t bundle_hash[32];
    char build[64];
    uint8_t reserved[392];
    uint32_t crc;
};
struct shoop_handoff {
    uint64_t nonce;
    uint32_t magic, version, slot, trial;
    uint8_t bundle_hash[32];
    char build[64];
    uint8_t operation_client[16];
    uint64_t operation_sequence;
    uint8_t reserved[364];
    uint32_t crc;
    struct shoop_config config;
};
_Static_assert(sizeof(struct shoop_config) == 512, "config ABI");
_Static_assert(sizeof(struct shoop_state) == 512, "state ABI");
_Static_assert(sizeof(struct shoop_counter) == 512, "counter ABI");
_Static_assert(sizeof(struct shoop_slot) == 512, "slot ABI");
_Static_assert(offsetof(struct shoop_handoff, config) == 512, "handoff ABI");
_Static_assert(sizeof(struct shoop_handoff) == 1024, "handoff size");

struct shoop_io {
    void *context;
    int (*read)(void *, uint32_t, void *);
    int (*write)(void *, uint32_t, const void *);
};
enum shoop_error {
    SHOOP_OK = 0, SHOOP_IO = -1, SHOOP_INVALID = -2,
    SHOOP_EXHAUSTED = -3, SHOOP_CONFLICT = -4,
};
uint32_t shoop_crc32(const void *, size_t);
void shoop_record_seal(void *, uint32_t);
int shoop_record_valid(const void *, uint32_t);
int shoop_config_load(const struct shoop_io *, struct shoop_config *);
int shoop_state_load(const struct shoop_io *, struct shoop_state *);
int shoop_state_store(const struct shoop_io *, struct shoop_state *);
int shoop_nonce_reserve(const struct shoop_io *, uint64_t *);
int shoop_candidate(const struct shoop_io *, struct shoop_state *, uint32_t, const uint8_t *);
int shoop_select(const struct shoop_io *, struct shoop_state *, uint32_t *);
int shoop_confirm(const struct shoop_io *, uint32_t, const uint8_t *);
int shoop_handoff_valid(const struct shoop_handoff *);
uint32_t shoop_slot_lba(uint32_t);
#endif
