#include <assert.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>

void shoop_system_reset(void) __attribute__((noreturn));
static jmp_buf halted;
static unsigned step;
static uint32_t original;

void mock_disable(void) { assert(step++ == 0); }
uint32_t Xil_In32(uintptr_t address) {
    assert(step++ == 2 && address == 0xf8000258u);
    return original;
}
void Xil_Out32(uintptr_t address, uint32_t value) {
    switch (step++) {
    case 1: assert(address == 0xf8000008u && value == 0xdf0du); break;
    case 3: assert(address == 0xf8000258u && value == (original & ~0x0f000000u)); break;
    case 5: assert(address == 0xf8000200u && value == 1); break;
    default: assert(0);
    }
}
void mock_barrier(void) { assert(step == 4 || step == 6); ++step; }
void shoop_halt(void) { assert(step++ == 7); longjmp(halted, 1); }

static void run_case(uint32_t value) {
    original = value;
    step = 0;
    if (!setjmp(halted)) shoop_system_reset();
    assert(step == 8);
}
int main(void) {
    run_case(0);
    run_case(UINT32_MAX);
    run_case(0x9e123456u);
    puts("System reset: interrupts disabled, FSBL PL reload flags cleared, other status preserved, reset and barriers ordered: passed");
}
