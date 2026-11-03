/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "runtime.h"
#include "FreeRTOS.h"
#include "task.h"
#include "xsdps.h"
#include "xil_io.h"
#include "xil_printf.h"
#include <stdatomic.h>
#include <string.h>

extern uint64_t shoop_millis(void);
extern void shoop_halt(void);
extern const char shoop_build_identity[];
static XSdPs sd;
XSdPs_Config XSdPs_ConfigTable[1] = {{
    .DeviceId = 0, .BaseAddress = 0xe0100000, .InputClockHz = 50000000,
    .CardDetect = 1, .WriteProtect = 0, .BusWidth = 4, .BankNumber = 1,
}};
static uint8_t bounce[512] __attribute__((aligned(64)));
static struct shoop_handoff handoff;
static _Atomic uint32_t control_progress, render_progress, broker_progress;
static _Atomic uint32_t ready, ready_at, require_render;
static uint32_t last_control, last_render, last_broker, last_feed;
static int confirmed, stable_recorded;

static void watchdog(unsigned seconds) {
    Xil_Out32(0xf8005000, 0xabc000);
    if (seconds) {
        unsigned count = (uint64_t)seconds * 111111111 / (512 * 4096) + 1;
        Xil_Out32(0xf8005004, 0x920000 | (count << 2) | 2);
        Xil_Out32(0xf8005000, 0xabc003);
        Xil_Out32(0xf8005008, 0x1999);
    }
}
static int read_sector(void *context, uint32_t lba, void *out) {
    (void)context;
    if (lba >= SHOOP_CARD_SECTORS) return SHOOP_INVALID;
    u32 address = sd.HCS ? lba : lba * 512;
    if (XSdPs_ReadPolled(&sd, address, 1, bounce) != XST_SUCCESS) return SHOOP_IO;
    memcpy(out, bounce, 512);
    return 0;
}
static int write_sector(void *context, uint32_t lba, const void *in) {
    (void)context;
    if (lba != SHOOP_STATE_0_LBA && lba != SHOOP_STATE_1_LBA) return SHOOP_INVALID;
    u32 address = sd.HCS ? lba : lba * 512;
    memcpy(bounce, in, 512);
    return XSdPs_WritePolled(&sd, address, 1, bounce) == XST_SUCCESS ? 0 : SHOOP_IO;
}
static const struct shoop_io io = { NULL, read_sector, write_sector };
int shoop_deploy_init(void) {
    memcpy(&handoff, (const void *)SHOOP_HANDOFF_ADDRESS, sizeof handoff);
    if (!shoop_handoff_valid(&handoff) || strcmp(handoff.build, shoop_build_identity)) return 0;
    if (XSdPs_CfgInitialize(&sd, &XSdPs_ConfigTable[0], 0xe0100000) != XST_SUCCESS ||
        XSdPs_CardInitialize(&sd) != XST_SUCCESS) {
        xil_printf("SHOOP_SD_INIT_FAILED\r\n"); return 0;
    }
    struct shoop_config config;
    if (shoop_config_load(&io, &config) || memcmp(&config, &handoff.config, sizeof config)) return 0;
    struct shoop_counter counter;
    unsigned valid = 0;
    const uint32_t sectors[] = { SHOOP_COUNTER_0_LBA, SHOOP_COUNTER_1_LBA };
    for (unsigned i = 0; i < 2; ++i) {
        if (!read_sector(NULL, sectors[i], &counter) && shoop_record_valid(&counter, 3)) {
            if (counter.value != handoff.nonce) return 0;
            ++valid;
        }
    }
    if (!valid) return 0;
    atomic_store(&require_render, 1);
    xil_printf("SHOOP_DEPLOY_HANDOFF slot=%u trial=%u build=%s\r\n", handoff.slot, handoff.trial, handoff.build);
#if SHOOP_DEPLOY_FAULT == 1
    xil_printf("SHOOP_DEPLOY_TEST_BEFORE_READY\r\n"); shoop_halt();
#endif
    return 1;
}
const struct shoop_config *shoop_deploy_config(void) { return &handoff.config; }
void shoop_deploy_tick(void) {
    if (!atomic_load(&ready)) return;
    uint32_t now = (uint32_t)shoop_millis();
    if (now - last_feed < 100) return;
    uint32_t c = atomic_load(&control_progress), r = atomic_load(&render_progress), b = atomic_load(&broker_progress);
    if (c != last_control && b != last_broker && (!atomic_load(&require_render) || r != last_render)) {
        Xil_Out32(0xf8005008, 0x1999);
        last_feed = now; last_control = c; last_render = r; last_broker = b;
    }
}
void shoop_deploy_render(void) {
    atomic_fetch_add(&render_progress, 1);
#if SHOOP_DEPLOY_FAULT == 2
    if (atomic_load(&ready) && (uint32_t)shoop_millis() - atomic_load(&ready_at) > 300) shoop_halt();
#endif
}
void shoop_deploy_broker(void) {
    atomic_fetch_add(&broker_progress, 1);
#if SHOOP_DEPLOY_FAULT == 4
    if (atomic_load(&ready) && (uint32_t)shoop_millis() - atomic_load(&ready_at) > 300) vTaskSuspend(NULL);
#endif
}
static int record_stable(void) {
    if (handoff.slot == SHOOP_NO_SLOT) return 0;
    struct shoop_state state;
    if (shoop_state_load(&io, &state) || state.confirmed != handoff.slot ||
        memcmp(state.confirmed_hash, handoff.bundle_hash, 32)) return SHOOP_CONFLICT;
    state.boot_failures = 0;
    return shoop_state_store(&io, &state);
}
void shoop_deploy_control(void) {
    atomic_fetch_add(&control_progress, 1);
    if (atomic_load(&ready)) {
        uint32_t elapsed = (uint32_t)shoop_millis() - atomic_load(&ready_at);
#if SHOOP_DEPLOY_FAULT == 3
        if (elapsed > 300) shoop_halt();
#endif
        if (!stable_recorded && elapsed >= 60000 && !record_stable()) stable_recorded = 1;
    }
}
void shoop_deploy_render_stopped(void) { atomic_store(&require_render, 0); }
void shoop_deploy_stopped(void) {
    if (atomic_load(&ready) && !stable_recorded && record_stable())
        xil_printf("SHOOP_BOOT_CLEAN_RECORD_FAILED\r\n");
    atomic_store(&ready, 0); watchdog(0);
}
int shoop_deploy_confirm(void) {
    uint32_t start = (uint32_t)shoop_millis();
    uint32_t render = atomic_load(&render_progress), broker = atomic_load(&broker_progress);
    while ((uint32_t)shoop_millis() - start < 2000) vTaskDelay(1);
    if (atomic_load(&render_progress) == render || atomic_load(&broker_progress) == broker) return 0;
    if (handoff.slot < 2 && shoop_confirm(&io, handoff.slot, handoff.bundle_hash)) return 0;
    confirmed = handoff.slot < 2;
    atomic_store(&ready_at, (uint32_t)shoop_millis());
    atomic_store(&ready, 1); watchdog(10);
    xil_printf(confirmed ? "SHOOP_DEPLOY_CONFIRMED slot=%u build=%s\r\n" : "SHOOP_DEPLOY_RAM_READY slot=%u build=%s\r\n",
               handoff.slot, handoff.build);
    return 1;
}
int shoop_deploy_info(struct shoop_deployment_info *out) {
    *out = (struct shoop_deployment_info){ .enabled = 1, .slot = handoff.slot, .confirmed = confirmed,
        .trial = handoff.trial, .nonce = handoff.nonce, .operation_sequence = handoff.operation_sequence };
    memcpy(out->build, handoff.build, 64); memcpy(out->bundle_hash, handoff.bundle_hash, 32);
    memcpy(out->operation_client, handoff.operation_client, 16);
    return 1;
}
int shoop_deploy_request(const uint8_t *client, uint64_t sequence) {
    struct shoop_state state;
    if (shoop_state_load(&io, &state)) return 0;
    state.mode = SHOOP_BOOT_UPDATE; state.boot_failures = 0;
    memcpy(state.operation_client, client, 16); state.operation_sequence = sequence;
    if (shoop_state_store(&io, &state)) return 0;
    return 1;
}
void shoop_deploy_reset(void) {
    portDISABLE_INTERRUPTS();
    Xil_Out32(0xf8000008, 0xdf0d); Xil_Out32(0xf8000200, 1);
    shoop_halt();
}
