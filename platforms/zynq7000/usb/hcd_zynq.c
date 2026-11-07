#include "tusb.h"
#include "host/hcd.h"
#include "portable/chipidea/ci_hs/ci_hs_type.h"
#include "portable/ehci/ehci_api.h"
#include "FreeRTOS.h"
#include "task.h"
#include "xscugic.h"
#include "xil_mmu.h"
#include "xil_cache.h"
#include "xil_printf.h"
#include <string.h>
#include <stdatomic.h>

#define USB0_BASE 0xe0002000u
#define USB0_IRQ 53u

extern XScuGic xInterruptController;
extern unsigned char __usb_dma_start[], __usb_dma_end[];

static ci_hs_regs_t *const controller = (ci_hs_regs_t *)USB0_BASE;
static _Atomic bool controller_failed;

bool shoop_usb_controller_failed(void) { return atomic_load(&controller_failed); }

bool shoop_usb_wait_status(volatile uint32_t *reg, uint32_t mask, uint32_t expected) {
    TickType_t start = xTaskGetTickCount();
    while ((*reg & mask) != expected) {
        if ((TickType_t)(xTaskGetTickCount() - start) >= pdMS_TO_TICKS(100)) {
            controller->USBINTR = 0;
            controller->USBCMD &= ~1u;
            atomic_store(&controller_failed, true);
            xil_printf("SHOOP_USB_FAILED controller-handshake mask=%08x expected=%08x\r\n", mask, expected);
            return false;
        }
        vTaskDelay(1);
    }
    return true;
}

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

static volatile uint32_t *reg32(uintptr_t address) {
    return (volatile uint32_t *)address;
}

static bool ulpi(uint8_t address, uint8_t *value, bool write) {
    volatile uint32_t *viewport = reg32(USB0_BASE + 0x170);
    if (!(*viewport & (1u << 27))) {
        *viewport = 1u << 31;
        if (!wait_clear(viewport, 1u << 31)) return false;
    }
    *viewport = (1u << 30) | (write ? 1u << 29 : 0) | (uint32_t)address << 16 | (write ? *value : 0);
    if (!wait_clear(viewport, 1u << 30)) return false;
    if (!write) *value = *viewport >> 8;
    return true;
}

static bool phy_write(uint8_t address, uint8_t value) {
    return ulpi(address, &value, true);
}

extern void shoop_board_usb_reset(void);

static bool phy_init(void) {
    uint8_t id[4], saved, scratch;
    for (unsigned i = 0; i < sizeof(id); ++i) if (!ulpi(i, &id[i], false)) return false;
    if (!(id[0] | id[1]) || (id[0] == 0xff && id[1] == 0xff)) return false;
    if (!ulpi(0x16, &saved, false)) return false;
    for (unsigned i = 0; i < 2; ++i) {
        uint8_t test = i ? 0xaa : 0x55;
        if (!phy_write(0x16, test) || !ulpi(0x16, &scratch, false) || scratch != test) return false;
    }
    if (!phy_write(0x16, saved) || !phy_write(0x0a, 0x86) ||
        !phy_write(0x04, 0x41) || !phy_write(0x07, 0) || !phy_write(0x0b, 0x60)) return false;
    xil_printf("SHOOP_USB_PHY vendor=%02x%02x product=%02x%02x\r\n", id[1], id[0], id[3], id[2]);
    return true;
}

bool hcd_init(uint8_t rhport, const tusb_rhport_init_t *init) {
    if (rhport != 0 || init->role != TUSB_ROLE_HOST) return false;
    shoop_board_usb_reset();
    size_t size = (size_t)(__usb_dma_end - __usb_dma_start);
    if (!size || size > 0x100000 || ((uintptr_t)__usb_dma_start & 0xfffff)) return false;
    Xil_SetTlbAttributes((INTPTR)__usb_dma_start, NORM_NONCACHE);
    memset(__usb_dma_start, 0, size);
    __asm__ volatile("dsb sy" ::: "memory");
    controller->USBCMD = USBCMD_RESET;
    if (!wait_clear(&controller->USBCMD, USBCMD_RESET)) return false;
    controller->USBMODE = USBMODE_CM_HOST;
    controller->PORTSC1 = (controller->PORTSC1 & ~((3u << 30) | (1u << 28))) | (2u << 30);
    if (!phy_init()) return false;
    XScuGic_SetPriorityTriggerType(&xInterruptController, USB0_IRQ,
        (configMAX_API_CALL_INTERRUPT_PRIORITY + 1) << portPRIORITY_SHIFT, 1);
    if (XScuGic_Connect(&xInterruptController, USB0_IRQ, usb_irq, NULL) != XST_SUCCESS) return false;
    bool initialized = ehci_init(0, USB0_BASE + 0x100, USB0_BASE + 0x140);
    if (initialized && (controller->PORTSC1 & 1)) {
        controller->PORTSC1 = (controller->PORTSC1 & ~0x2au) | 2u;
        controller->USBSTS = 1u << 2;
        hcd_event_device_attach(0, false);
    }
    return initialized;
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
    bool halted = ehci_deinit(0);
    return phy_write(0x0c, 0x60) && halted;
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
