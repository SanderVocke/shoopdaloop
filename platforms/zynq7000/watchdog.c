/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "watchdog.h"
#ifdef SHOOP_HOST_WATCHDOG
#include "FreeRTOS.h"
#include "task.h"
#include "xil_io.h"
#include "xil_printf.h"
#include <stdatomic.h>
#include <stdint.h>

extern uint64_t shoop_millis(void);
static _Atomic uint32_t control, broker, render;
static _Atomic uint32_t ready, require_render, render_baseline, render_started_at;
static uint32_t last_control, last_broker, last_render, last_feed;
static uint32_t ready_at;

static void arm(unsigned seconds) {
    unsigned count = (uint64_t)seconds * SHOOP_WATCHDOG_HZ / (512u * 4096u) + 1;
    configASSERT(count <= 0xfff);
    Xil_Out32(0xf8005000u, 0xabc000u);
    Xil_Out32(0xf8005004u, 0x920000u | (count << 2) | 2u);
    Xil_Out32(0xf8005000u, 0xabc003u);
    Xil_Out32(0xf8005008u, 0x1999u);
    DATA_SYNC;
}
void shoop_watchdog_start(void) {
    uint32_t locked = Xil_In32(0xf800000cu) & 1u;
    Xil_Out32(0xf8000008u, 0xdf0du);
    Xil_Out32(0xf8000258u, Xil_In32(0xf8000258u) & ~0x0f000000u);
    if (locked) Xil_Out32(0xf8000004u, 0x767bu);
    DATA_SYNC;
    atomic_store(&require_render, 1);
    arm(30);
    xil_printf("SHOOP_WATCHDOG_ARMED startup_s=30 ready_s=10\r\n");
}
int shoop_watchdog_ready(void) {
    ready_at = (uint32_t)shoop_millis();
    last_feed = ready_at;
    last_control = atomic_load(&control);
    last_broker = atomic_load(&broker);
    last_render = atomic_load(&render);
    arm(10);
    atomic_store(&ready, 1);
    xil_printf("SHOOP_WATCHDOG_READY\r\n");
    return 1;
}
void shoop_watchdog_tick(void) {
    if (!atomic_load(&ready)) return;
    uint32_t now = (uint32_t)shoop_millis();
    if (now - last_feed < 100) return;
    uint32_t c = atomic_load(&control), b = atomic_load(&broker), r = atomic_load(&render);
    uint32_t required = atomic_load(&require_render);
    int render_ok = !required || r != last_render;
    if (required == 2) {
        if (r != atomic_load(&render_baseline)) atomic_store(&require_render, 1);
        else render_ok = now - atomic_load(&render_started_at) < 25000;
    }
    if (c != last_control && b != last_broker && render_ok) {
        Xil_Out32(0xf8005008u, 0x1999u);
        last_feed = now;
        last_control = c; last_broker = b; last_render = r;
    }
}
#if SHOOP_WATCHDOG_FAULT
extern void shoop_watchdog_inject(unsigned) __attribute__((noreturn));
static void inject(unsigned actor) {
    if (atomic_load(&ready) && (uint32_t)shoop_millis() - ready_at >= 32000 &&
        (SHOOP_WATCHDOG_FAULT == actor || (actor == 1 && SHOOP_WATCHDOG_FAULT >= 4))) {
        xil_printf("SHOOP_WATCHDOG_TEST_FAULT kind=%u\r\n", SHOOP_WATCHDOG_FAULT);
        if (SHOOP_WATCHDOG_FAULT <= 3) vTaskSuspend(NULL);
        else shoop_watchdog_inject(SHOOP_WATCHDOG_FAULT);
    }
}
#else
static void inject(unsigned actor) { (void)actor; }
#endif
void shoop_watchdog_control(void) { atomic_fetch_add(&control, 1); inject(1); }
void shoop_watchdog_broker(void) { atomic_fetch_add(&broker, 1); inject(2); }
void shoop_watchdog_render(void) { atomic_fetch_add(&render, 1); inject(3); }
void shoop_watchdog_render_stopped(void) { atomic_store(&require_render, 0); }
void shoop_watchdog_render_started(void) {
    atomic_store(&render_baseline, atomic_load(&render));
    atomic_store(&render_started_at, (uint32_t)shoop_millis());
    atomic_store(&require_render, 2);
}
void shoop_watchdog_stopped(void) {
    atomic_store(&ready, 0);
    Xil_Out32(0xf8005000u, 0xabc000u);
    DATA_SYNC;
}
#endif
