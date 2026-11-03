#include "host.h"
#include "tusb.h"
#include "FreeRTOS.h"
#include "task.h"
#include "xil_printf.h"
#include <stdatomic.h>

static TaskHandle_t usb_task;
static _Atomic int stopping;
static _Atomic int stopped;

static void usb_run(void *unused) {
    (void)unused;
    tusb_rhport_init_t init = { .role = TUSB_ROLE_HOST, .speed = TUSB_SPEED_HIGH };
    if (!tusb_init(0, &init)) {
        xil_printf("SHOOP_USB_FAILED init\r\n");
        atomic_store(&stopped, -1);
        vTaskSuspend(NULL);
    }
    xil_printf("SHOOP_USB_READY TinyUSB host\r\n");
    while (!atomic_load(&stopping)) {
        tuh_task_ext(0, false);
        vTaskDelay(1);
    }
    atomic_store(&stopped, tuh_deinit(0) ? 1 : -1);
    vTaskSuspend(NULL);
}

void shoop_usb_start(void) {
    configASSERT(!usb_task);
    configASSERT(xTaskCreate(usb_run, "usb-host", 8192, NULL, 5, &usb_task) == pdPASS);
}

int shoop_usb_stop(void) {
    if (!usb_task) return 1;
    atomic_store(&stopping, 1);
    TickType_t start = xTaskGetTickCount();
    while (!atomic_load(&stopped)) {
        if ((TickType_t)(xTaskGetTickCount() - start) >= pdMS_TO_TICKS(5000)) return 0;
        vTaskDelay(1);
    }
    if (atomic_load(&stopped) != 1) return 0;
    vTaskDelete(usb_task);
    usb_task = NULL;
    return 1;
}

void tuh_midi_mount_cb(uint8_t index, const tuh_midi_mount_cb_t *info) {
    xil_printf("SHOOP_USB_MIDI_MOUNT index=%u addr=%u interface=%u rx=%u tx=%u\r\n",
        index, info->daddr, info->bInterfaceNumber, info->rx_cable_count, info->tx_cable_count);
}

void tuh_midi_umount_cb(uint8_t index) {
    xil_printf("SHOOP_USB_MIDI_UNMOUNT index=%u\r\n", index);
}

void tuh_midi_rx_cb(uint8_t index, uint32_t transferred) {
    (void)transferred;
    uint8_t packet[4];
    for (unsigned i = 0; i < 128 && tuh_midi_packet_read(index, packet); ++i) {}
}
