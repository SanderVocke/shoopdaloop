#include "FreeRTOS.h"
#include "task.h"
#include "xuartps.h"
#include "xil_printf.h"
#include <stdatomic.h>
#include <stdlib.h>
#include <stdio.h>

static _Atomic unsigned paused;
static _Atomic unsigned pending;
static _Atomic uint32_t input_bits;
static _Atomic uint32_t midi_packet;
static _Atomic unsigned fpu_passes;
static _Atomic unsigned fail_cleanup;
extern void shoop_render_wake(void);
extern void shoop_fixture_report(uint32_t *, uint32_t *, uint32_t *, uint32_t *, uint32_t *, uint32_t *);
extern void shoop_heap_stats(size_t *, size_t *);
extern unsigned shoop_socket_count(void);
extern uint64_t shoop_millis(void);
int shoop_fixture_fail_cleanup(void) { return atomic_exchange(&fail_cleanup, 0); }

int shoop_fixture_paused(void) { return atomic_load(&paused); }
uint32_t shoop_fixture_input(void) { return atomic_load(&input_bits); }
uint32_t shoop_fixture_midi(void) { return atomic_exchange(&midi_packet, 0); }
unsigned shoop_fixture_frames(void) {
    if (!atomic_load(&paused)) return 256;
    unsigned count = atomic_load(&pending);
    if (count > 256) count = 256;
    atomic_fetch_sub(&pending, count);
    if (atomic_load(&pending)) shoop_render_wake();
    return count;
}
void shoop_fixture_stopping(void) { atomic_fetch_add(&pending, 256); }
void shoop_fixture_irq(void) {
    uint32_t low = 0x76543210, high = 0xfedcba98;
    __asm__ volatile("vmov d0, %0, %1\nvmov d16, %0, %1" :: "r"(low), "r"(high) : "d0", "d16");
}
static void fpu_task(void *argument) {
    uint32_t low = (uint32_t)argument, high = ~low;
    for (unsigned i = 0; i < 256; ++i) {
        uint32_t a, b, c, d;
        __asm__ volatile(
            "vmov d0, %4, %5\nvmov d16, %4, %5\nwfi\nvmov %0, %1, d0\nvmov %2, %3, d16"
            : "=&r"(a), "=&r"(b), "=&r"(c), "=&r"(d) : "r"(low), "r"(high) : "d0", "d16", "memory");
        configASSERT(a == low && b == high && c == low && d == high);
    }
    if (atomic_fetch_add(&fpu_passes, 1) == 1) xil_printf("SHOOP_FPU_PASS\r\n");
    vTaskDelete(NULL);
}
static void reply(void) {
    uint32_t frame, hash, midi, on, off, bad;
    size_t used, peak;
    shoop_fixture_report(&frame, &hash, &midi, &on, &off, &bad);
    shoop_heap_stats(&used, &peak);
    char line[192];
    int size = snprintf(line, sizeof(line), "D %u %u %u %u %u %u %u %u %u %u %u\n", (unsigned)frame, (unsigned)hash,
        (unsigned)midi, (unsigned)used, (unsigned)peak, (unsigned)xPortGetFreeHeapSize(),
        shoop_socket_count(), (unsigned)shoop_millis(), (unsigned)on, (unsigned)off, (unsigned)bad);
    configASSERT(size > 0 && (size_t)size < sizeof(line));
    for (int i = 0; i < size; ++i) XUartPs_SendByte(0xe0000000, line[i]);
}
static void driver(void *unused) {
    (void)unused;
    char command[64];
    unsigned used = 0;
    XUartPs_WriteReg(0xe0000000, XUARTPS_CR_OFFSET, XUARTPS_CR_TX_EN | XUARTPS_CR_RX_EN);
    for (;;) {
        while (XUartPs_IsReceiveData(0xe0000000)) {
            char byte = XUartPs_RecvByte(0xe0000000);
            if (byte == '\n') {
                command[used] = 0;
                unsigned value = used > 1 ? (unsigned)strtoul(command + 1, NULL, 10) : 0;
                switch (command[0]) {
                case 'P': atomic_store(&paused, value != 0); break;
                case 'I': atomic_store(&input_bits, value); break;
                case 'M': configASSERT(value <= 2); atomic_store(&midi_packet, value); break;
                case 'F': atomic_store(&fail_cleanup, value != 0); break;
                case 'S':
                    configASSERT(value <= 65536 && atomic_load(&paused));
                    atomic_fetch_add(&pending, value); shoop_render_wake();
                    while (atomic_load(&pending)) vTaskDelay(1);
                    break;
                case 'Q': break;
                default: vApplicationAssert("fixture command", 0);
                }
                reply(); used = 0;
            } else {
                configASSERT(used + 1 < sizeof(command));
                command[used++] = byte;
            }
        }
        vTaskDelay(1);
    }
}
void shoop_fixture_start(void) {
    vTaskSuspendAll();
    configASSERT(xTaskCreate(fpu_task, "fpu-a", 2048, (void *)0x12345678, 5, NULL) == pdPASS);
    configASSERT(xTaskCreate(fpu_task, "fpu-b", 2048, (void *)0x87654321, 5, NULL) == pdPASS);
    configASSERT(xTaskCreate(driver, "fixture", 4096, NULL, 3, NULL) == pdPASS);
    xTaskResumeAll();
}
