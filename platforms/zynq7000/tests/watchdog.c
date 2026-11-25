#include <assert.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "../watchdog.c"

static uint32_t now, mode, clock_control, reboot_status, lock_state;
static uint32_t feeds, resets, deadline, logs, locked_writes;
static jmp_buf fault;
static unsigned injected;
uint64_t shoop_millis(void) { return now; }
uint32_t Xil_In32(uint32_t address) {
    if (address == 0xf800000cu) return lock_state;
    assert(address == 0xf8000258u);
    return reboot_status;
}
void Xil_Out32(uint32_t address, uint32_t value) {
    switch (address) {
    case 0xf8000008u: assert(value == 0xdf0d); lock_state = 0; break;
    case 0xf8000004u: assert(value == 0x767b); lock_state = 1; ++locked_writes; break;
    case 0xf8000258u: assert(!lock_state); reboot_status = value; break;
    case 0xf8005000u: mode = value; break;
    case 0xf8005004u: clock_control = value; break;
    case 0xf8005008u:
        assert(value == 0x1999 && mode == 0xabc003);
        deadline = now + (uint64_t)((clock_control >> 2) & 0xfff) * 4096 * 512 * 1000 / SHOOP_WATCHDOG_HZ;
        ++feeds; break;
    default: assert(!"unexpected register write");
    }
}
int xil_printf(const char *format, ...) { assert(format); ++logs; return 0; }
void vTaskSuspend(void *task) { assert(!task); injected = SHOOP_WATCHDOG_FAULT; longjmp(fault, 1); }
void shoop_watchdog_inject(unsigned kind) { injected = kind; longjmp(fault, 1); }
static void advance(uint32_t millis) {
    now += millis;
    if (mode == 0xabc003 && (int32_t)(now - deadline) >= 0) { ++resets; mode = 0; }
}
static void reset(void) {
    atomic_store(&control, 0); atomic_store(&broker, 0); atomic_store(&render, 0);
    atomic_store(&ready, 0); atomic_store(&require_render, 0);
    atomic_store(&render_baseline, 0); atomic_store(&render_started_at, 0);
    last_control = last_broker = last_render = last_feed = ready_at = 0;
    now = feeds = resets = logs = locked_writes = 0;
    mode = clock_control = injected = 0;
    reboot_status = 0x0f480000; lock_state = 1;
    shoop_watchdog_start();
    assert(reboot_status == 0x00480000 && lock_state == 1 && locked_writes == 1);
    assert(mode == 0xabc003 && deadline >= 30000 && deadline < 30100);
    assert(feeds == 1 && logs == 1);
}
static void progress(unsigned missing) {
    if (missing != 1) shoop_watchdog_control();
    if (missing != 2) shoop_watchdog_broker();
    if (missing != 3) shoop_watchdog_render();
    shoop_watchdog_tick();
}
int main(void) {
#if !SHOOP_WATCHDOG_FAULT
    reset();
    advance(29999); shoop_watchdog_tick(); assert(feeds == 1 && !resets);
    advance(101); assert(resets == 1);
    for (unsigned missing = 0; missing <= 3; ++missing) {
        reset(); assert(shoop_watchdog_ready() == 1);
        for (unsigned i = 0; i < 110; ++i) { advance(100); progress(missing); }
        assert(resets == (missing ? 1u : 0u));
    }
    reset(); shoop_watchdog_ready();
    progress(0); assert(feeds == 2);
    advance(99); progress(0); assert(feeds == 2);
    advance(1); progress(0); assert(feeds == 3);
    reset(); shoop_watchdog_ready(); shoop_watchdog_render_stopped();
    for (unsigned i = 0; i < 120; ++i) { advance(100); progress(3); }
    assert(!resets);
    shoop_watchdog_render_started();
    for (unsigned i = 0; i < 249; ++i) { advance(100); progress(3); }
    assert(!resets && atomic_load(&require_render) == 2);
    for (unsigned i = 0; i < 102; ++i) { advance(100); progress(3); }
    assert(resets == 1);
    reset(); shoop_watchdog_ready(); shoop_watchdog_render_started();
    advance(100); progress(0); assert(atomic_load(&require_render) == 1);
    for (unsigned i = 0; i < 102; ++i) { advance(100); progress(3); }
    assert(resets == 1);
    reset(); shoop_watchdog_ready(); shoop_watchdog_render_stopped();
    for (unsigned i = 0; i < 102; ++i) { advance(100); progress(1); }
    assert(resets == 1);
    reset(); shoop_watchdog_ready(); shoop_watchdog_stopped();
    advance(60000); progress(0); assert(mode == 0xabc000 && !resets);
    reset(); now = UINT32_MAX - 50; shoop_watchdog_ready();
    advance(100); progress(0); assert(feeds == 3 && !resets);
    reset(); lock_state = 0; shoop_watchdog_start(); assert(!lock_state && locked_writes == 1);
#else
    reset(); shoop_watchdog_ready();
    for (unsigned i = 0; i < 319; ++i) { advance(100); progress(0); }
    advance(99);
    if (!setjmp(fault)) {
        progress(0); assert(!injected);
        advance(1); progress(0); assert(!"test fault did not fire");
    }
    assert(injected == SHOOP_WATCHDOG_FAULT);
#endif
    printf("watchdog policy/fault variant=%u: passed\n", SHOOP_WATCHDOG_FAULT);
    return 0;
}
