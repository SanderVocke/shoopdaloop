#include "tusb.h"
#include "host/hcd.h"
#include "portable/chipidea/ci_hs/ci_hs_type.h"
#include "portable/ehci/ehci_api.h"
#include "FreeRTOS.h"
#include "task.h"
#include "xscugic.h"
#include "xil_mmu.h"
#include "xil_cache.h"
#include <string.h>

#define USB0_BASE 0xe0002000u
#define USB0_IRQ 53u

extern XScuGic xInterruptController;
extern unsigned char __usb_dma_start[], __usb_dma_end[];

static ci_hs_regs_t *const controller = (ci_hs_regs_t *)USB0_BASE;

static void usb_irq(void *unused) {
    (void)unused;
    hcd_int_handler(0, true);
}

static bool wait_clear(volatile uint32_t *reg, uint32_t mask) {
    TickType_t start = xTaskGetTickCount();
    while (*reg & mask) {
        if ((TickType_t)(xTaskGetTickCount() - start) >= pdMS_TO_TICKS(100)) return false;
        vTaskDelay(1);
    }
    return true;
}

bool hcd_init(uint8_t rhport, const tusb_rhport_init_t *init) {
    if (rhport != 0 || init->role != TUSB_ROLE_HOST) return false;
    size_t size = (size_t)(__usb_dma_end - __usb_dma_start);
    if (!size || size > 0x100000 || ((uintptr_t)__usb_dma_start & 0xfffff)) return false;
    Xil_SetTlbAttributes((INTPTR)__usb_dma_start, NORM_NONCACHE);
    memset(__usb_dma_start, 0, size);
    __asm__ volatile("dsb sy" ::: "memory");
    controller->USBCMD = USBCMD_RESET;
    if (!wait_clear(&controller->USBCMD, USBCMD_RESET)) return false;
    controller->USBMODE = USBMODE_CM_HOST;
    controller->PORTSC1 = (controller->PORTSC1 & ~((3u << 30) | (1u << 28))) | (2u << 30);
    XScuGic_SetPriorityTriggerType(&xInterruptController, USB0_IRQ,
        (configMAX_API_CALL_INTERRUPT_PRIORITY + 1) << portPRIORITY_SHIFT, 1);
    if (XScuGic_Connect(&xInterruptController, USB0_IRQ, usb_irq, NULL) != XST_SUCCESS) return false;
    return ehci_init(0, USB0_BASE + 0x100, USB0_BASE + 0x140);
}

bool hcd_deinit(uint8_t rhport) {
    if (rhport != 0) return false;
    controller->USBINTR = 0;
    controller->USBCMD &= ~1u;
    TickType_t start = xTaskGetTickCount();
    while (!(controller->USBSTS & (1u << 12))) {
        if ((TickType_t)(xTaskGetTickCount() - start) >= pdMS_TO_TICKS(100)) return false;
        vTaskDelay(1);
    }
    XScuGic_Disable(&xInterruptController, USB0_IRQ);
    XScuGic_Disconnect(&xInterruptController, USB0_IRQ);
    __asm__ volatile("dsb sy" ::: "memory");
    return ehci_deinit(0);
}

void hcd_int_enable(uint8_t rhport) {
    if (rhport == 0) XScuGic_Enable(&xInterruptController, USB0_IRQ);
}

void hcd_int_disable(uint8_t rhport) {
    if (rhport == 0) XScuGic_Disable(&xInterruptController, USB0_IRQ);
}

bool hcd_dcache_clean(const void *address, uint32_t bytes) {
    Xil_DCacheFlushRange((INTPTR)address, bytes);
    __asm__ volatile("dsb sy" ::: "memory");
    return true;
}

bool hcd_dcache_invalidate(const void *address, uint32_t bytes) {
    Xil_DCacheInvalidateRange((INTPTR)address, bytes);
    __asm__ volatile("dsb sy" ::: "memory");
    return true;
}

bool hcd_dcache_clean_invalidate(const void *address, uint32_t bytes) {
    return hcd_dcache_clean(address, bytes);
}
