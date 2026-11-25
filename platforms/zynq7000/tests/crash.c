#include "crash.h"
#include <assert.h>
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xil_exception.h"

extern volatile struct shoop_crash_record shoop_crash_record;
uint32_t UndefinedExceptionAddr = 0x00123450, PrefetchAbortAddr = 0x00134560, DataAbortAddr = 0x00145670;
const char shoop_build_identity[] = "crash-test";
static jmp_buf halted;
static unsigned flushes, reads, writes, disabled, logs, captures;
static int blocked, nested, interrupt_flush;
static char output[1024];
static void (*handlers[5])(void *);
static void *arguments[5];

void shoop_exception_mask(void) { ++disabled; }
void *xTaskGetCurrentTaskHandle(void) { return (void *)0x12340000; }
void shoop_exception_registers(struct shoop_crash_registers *regs) {
    ++captures;
    *regs = (struct shoop_crash_registers){0x197, 0x60000153, 0x1008, 0x43c00078, 3, 4};
}
void Xil_ExceptionRegisterHandler(uint32_t id, void (*handler)(void *), void *arg) {
    assert(id == 1 || id == 3 || id == 4);
    handlers[id] = handler; arguments[id] = arg;
}
void Xil_DCacheFlushRange(intptr_t address, uint32_t bytes) {
    assert(address == (intptr_t)&shoop_crash_record && bytes == 64);
    ++flushes;
    if (flushes & 1) {
        assert(shoop_crash_record.word[0] == 0);
        if (interrupt_flush) longjmp(halted, 1);
    } else assert(shoop_crash_valid(&shoop_crash_record));
}
void mock_barrier(void) {}
uint32_t mock_uart_read(uintptr_t address, unsigned offset) {
    assert(address == 0xe0001000 && offset == 0x2c);
    assert(flushes == 2);
    ++reads;
    if (nested) { nested = 0; handlers[3](arguments[3]); assert(0); }
    return blocked ? 16 : 0;
}
void mock_uart_write(uintptr_t address, unsigned offset, uint32_t byte) {
    assert(address == 0xe0001000 && offset == 0x30 && writes < sizeof(output)-1);
    output[writes++] = (char)byte;
}
int xil_printf(const char *format, ...) {
    assert(strstr(format, "SHOOP_CRASH_RECORD")); ++logs; return 0;
}
void shoop_exception_halt(void) { longjmp(halted, 1); }

static int test_ready(void *context) { (void)context; return mock_uart_read(0xe0001000, 0x2c) == 0; }
static void test_write(void *context, unsigned char byte) { (void)context; mock_uart_write(0xe0001000, 0x30, byte); }
static void test_codec(void) {
    for (unsigned word = 0; word < SHOOP_CRASH_WORDS; ++word) {
        for (unsigned bit = 0; bit < 32; ++bit) {
            shoop_crash_record.word[word] ^= 1u << bit;
            assert(!shoop_crash_valid(&shoop_crash_record));
            shoop_crash_record.word[word] ^= 1u << bit;
            assert(shoop_crash_valid(&shoop_crash_record));
        }
    }
    const struct shoop_crash_io io = {NULL, NULL, test_ready, test_write};
    reads = writes = 0; blocked = 0;
    assert(shoop_crash_emit(&shoop_crash_record, &io, 0) == 0 && reads == 0);
    assert(shoop_crash_emit(&shoop_crash_record, &io, 7) == 7 && reads == 7 && writes == 7);
    reads = writes = 0; blocked = 1;
    assert(shoop_crash_emit(&shoop_crash_record, &io, 9) == 0 && reads == 9 && writes == 0);
    shoop_crash_record.word[SHOOP_CRASH_CHECKSUM] ^= 1;
    reads = 0;
    assert(shoop_crash_emit(&shoop_crash_record, &io, 9) == 0 && reads == 0);
}

int main(int argc, char **argv) {
    assert(argc == 3);
    unsigned kind = (unsigned)atoi(argv[1]);
    blocked = !strcmp(argv[2], "blocked");
    nested = !strcmp(argv[2], "nested");
    shoop_exceptions_init();
    assert(logs == 1 && handlers[1] && handlers[3] && handlers[4]);
    assert(flushes == 1 && shoop_crash_record.word[0] == 0);
    flushes = 0;
    interrupt_flush = !strcmp(argv[2], "torn");
    if (!setjmp(halted)) handlers[kind](arguments[kind]);
    assert(disabled && captures == 1 && logs == 1);
    if (interrupt_flush) {
        assert(flushes == 1 && !shoop_crash_valid(&shoop_crash_record));
        assert(!writes && !reads);
    } else {
        assert(flushes == 2 && shoop_crash_valid(&shoop_crash_record));
        assert(shoop_crash_record.word[SHOOP_CRASH_KIND] == kind);
        assert(shoop_crash_record.word[SHOOP_CRASH_PC] == (kind == 4 ? DataAbortAddr : kind == 3 ? PrefetchAbortAddr : UndefinedExceptionAddr));
        assert(shoop_crash_record.word[SHOOP_CRASH_DFSR] == 0x1008);
        assert(shoop_crash_record.word[SHOOP_CRASH_DFAR] == 0x43c00078);
        assert(shoop_crash_record.word[SHOOP_CRASH_TASK] == 0x12340000);
        assert(shoop_crash_record.word[SHOOP_CRASH_SPSR] == 0x60000153);
        uint32_t tag = 2166136261u;
        for (unsigned i=0; shoop_build_identity[i]; ++i) tag = (tag ^ (unsigned char)shoop_build_identity[i]) * 16777619u;
        assert(shoop_crash_record.word[SHOOP_CRASH_BUILD] == tag);
        if (blocked) { assert(reads == 1000000 && writes == 0); }
        else if (disabled == 2) { assert(reads == 1 && writes == 0); }
        else { assert(reads == writes && strstr(output, "SHOOP_ARM_EXCEPTION 53444352 00000001 00000010")); }
        printf("MOCK_CRASH_WORDS");
        for (unsigned i = 0; i < SHOOP_CRASH_WORDS; ++i) printf(" %08x", shoop_crash_record.word[i]);
        putchar('\n');
        test_codec();
    }
    printf("ARM exception kind=%u mode=%s: ordered cache-flush record, CRC, bounded UART and fail-closed halt passed\n", kind, argv[2]);
}
