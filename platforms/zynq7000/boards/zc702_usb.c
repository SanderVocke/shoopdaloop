#include "FreeRTOS.h"
#include "task.h"
#include <stdint.h>

static volatile uint32_t *board_reg32(uintptr_t address) {
    return (volatile uint32_t *)address;
}

void shoop_board_usb_reset(void) {
    *board_reg32(0xf8000008) = 0xdf0d;
    *board_reg32(0xf800012c) |= (1u << 2) | (1u << 22);
    *board_reg32(0xf800071c) = 0x200;
    for (unsigned pin = 28; pin <= 39; ++pin)
        *board_reg32(0xf8000700 + 4 * pin) = pin == 29 || pin == 31 || pin == 36 ? 0x205 : 0x204;
    *board_reg32(0xf8000210) |= 1;
    *board_reg32(0xe000a000) = 0xff7f0000;
    *board_reg32(0xe000a204) |= 1u << 7;
    *board_reg32(0xe000a208) |= 1u << 7;
    __asm__ volatile("dsb sy" ::: "memory");
    vTaskDelay(pdMS_TO_TICKS(2));
    *board_reg32(0xe000a000) = 0xff7f0080;
    *board_reg32(0xf8000210) &= ~1u;
    *board_reg32(0xf8000004) = 0x767b;
    __asm__ volatile("dsb sy" ::: "memory");
    vTaskDelay(pdMS_TO_TICKS(10));
}
