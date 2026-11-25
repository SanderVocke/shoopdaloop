#include "FreeRTOS.h"
#include "task.h"
#include "crash.h"
#include "xil_cache.h"
#include "xil_exception.h"
#include "xil_io.h"
#include "xuartps.h"
#include "xil_printf.h"

extern uint32_t UndefinedExceptionAddr, PrefetchAbortAddr, DataAbortAddr;
extern const char shoop_build_identity[] __attribute__((weak));
volatile struct shoop_crash_record shoop_crash_record
    __attribute__((section(".noinit.crash"), aligned(64), used));
static volatile unsigned entered;
static uint32_t build_tag;

static void flush(void *context, const volatile void *record, size_t bytes) {
    (void)context;
    Xil_DCacheFlushRange((INTPTR)record, (u32)bytes);
    DATA_SYNC;
}
static int ready(void *context) {
    (void)context;
    return !(XUartPs_ReadReg(STDOUT_BASEADDRESS, XUARTPS_SR_OFFSET) & XUARTPS_SR_TXFULL);
}
static void write_byte(void *context, unsigned char byte) {
    (void)context;
    XUartPs_WriteReg(STDOUT_BASEADDRESS, XUARTPS_FIFO_OFFSET, byte);
}
static const struct shoop_crash_io io = {NULL, flush, ready, write_byte};

static void exception(void *data) {
    shoop_exception_mask();
    if (entered) shoop_exception_halt();
    entered = 1;
    struct shoop_crash_info info = {0};
    info.kind = (uint32_t)(uintptr_t)data;
    info.pc = info.kind == XIL_EXCEPTION_ID_DATA_ABORT_INT ? DataAbortAddr :
        info.kind == XIL_EXCEPTION_ID_PREFETCH_ABORT_INT ? PrefetchAbortAddr : UndefinedExceptionAddr;
    shoop_exception_registers(&info.cpu);
    info.task = (uint32_t)(uintptr_t)xTaskGetCurrentTaskHandle();
    info.build = build_tag;
    shoop_crash_store(&shoop_crash_record, &info, &io);
    (void)shoop_crash_emit(&shoop_crash_record, &io, 1000000);
    shoop_exception_halt();
}

void shoop_exceptions_init(void) {
    build_tag = 2166136261u;
    if (shoop_build_identity) {
        for (unsigned i = 0; i < 40 && shoop_build_identity[i]; ++i)
            build_tag = (build_tag ^ (unsigned char)shoop_build_identity[i]) * 16777619u;
    }
    Xil_ExceptionRegisterHandler(XIL_EXCEPTION_ID_UNDEFINED_INT, exception, (void *)(uintptr_t)XIL_EXCEPTION_ID_UNDEFINED_INT);
    Xil_ExceptionRegisterHandler(XIL_EXCEPTION_ID_PREFETCH_ABORT_INT, exception, (void *)(uintptr_t)XIL_EXCEPTION_ID_PREFETCH_ABORT_INT);
    Xil_ExceptionRegisterHandler(XIL_EXCEPTION_ID_DATA_ABORT_INT, exception, (void *)(uintptr_t)XIL_EXCEPTION_ID_DATA_ABORT_INT);
    shoop_crash_record.word[SHOOP_CRASH_SEAL] = 0;
    flush(NULL, &shoop_crash_record, sizeof(shoop_crash_record));
    xil_printf("SHOOP_CRASH_RECORD address=%08lx words=%u\r\n", (unsigned long)&shoop_crash_record, SHOOP_CRASH_WORDS);
}
