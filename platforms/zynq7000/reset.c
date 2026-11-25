#include "FreeRTOS.h"
#include "xil_io.h"

extern void shoop_halt(void) __attribute__((noreturn));

void __attribute__((noreturn)) shoop_system_reset(void) {
    portDISABLE_INTERRUPTS();
    Xil_Out32(0xf8000008u, 0xdf0du);
    /* Ensure the factory FSBL reloads PL after a warm reset. */
    uint32_t status = Xil_In32(0xf8000258u);
    Xil_Out32(0xf8000258u, status & ~0x0f000000u);
    DATA_SYNC;
    Xil_Out32(0xf8000200u, 1u);
    DATA_SYNC;
    shoop_halt();
}
