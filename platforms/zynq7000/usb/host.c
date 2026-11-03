#include "host.h"
#include "bridge.h"
#include "descriptors.h"
#include "tusb.h"
#include "FreeRTOS.h"
#include "task.h"
#include "xil_printf.h"
#include "xtime_l.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>

#define USB_INTERFACES 8u
#define USB_PENDING_PACKETS 512u

struct interface {
    bool active;
    bool identifying, have_serial;
    uint8_t identity_step;
    uint16_t language;
    uint32_t identity_token;
    uint64_t serial_hash;
    char product_name[64];
    tuh_midi_mount_cb_t mount;
    tusb_desc_device_t device;
    uint16_t head, tail;
    struct shoop_usb_packet_owner owners[USB_PENDING_PACKETS];
};

static struct interface interfaces[USB_INTERFACES];
_Atomic uint32_t shoop_usb_pending_packets[USB_INTERFACES];
static TaskHandle_t usb_task;
static _Atomic int stopping, stopped;
static unsigned cursor;
static uint32_t lost_packets, transfer_errors;
static int identity_index = -1;
static uint32_t identity_token;
CFG_TUH_MEM_SECTION CFG_TUH_MEM_ALIGN static uint16_t identity_buffer[128];

static void publish_mount(uint8_t index);

void tuh_hub_descriptor_failed_cb(uint8_t address) {
    xil_printf("SHOOP_USB_HUB_REFUSED addr=%u descriptor_or_ports\r\n", address);
}

bool tuh_enum_descriptor_configuration_cb(uint8_t address, uint8_t config, const tusb_desc_configuration_t *descriptor) {
    (void)config;
    bool valid = shoop_usb_configuration_valid((const uint8_t *)descriptor, descriptor->wTotalLength);
    if (valid) {
        const uint8_t *bytes = (const uint8_t *)descriptor;
        unsigned needed = 0, mounted = 0;
        for (unsigned i = 0; i < USB_INTERFACES; ++i) mounted += tuh_midi_mounted(i);
        for (unsigned offset = 9; offset < descriptor->wTotalLength; offset += bytes[offset]) {
            const uint8_t *interface = bytes + offset;
            needed += interface[1] == 4 && interface[0] >= 9 && interface[3] == 0 &&
                interface[5] == 1 && interface[6] == 3 && interface[7] == 0;
        }
        valid = mounted + needed <= USB_INTERFACES;
    }
    if (!valid) xil_printf("SHOOP_USB_MIDI_REFUSED addr=%u descriptor\r\n", address);
    return valid;
}

static void identity_complete(tuh_xfer_t *transfer) {
    if (identity_index < 0 || transfer->user_data != identity_token) return;
    struct interface *interface = &interfaces[identity_index];
    identity_index = -1;
    if (!interface->identifying || interface->identity_token != transfer->user_data) return;
    const uint8_t *bytes = (const uint8_t *)identity_buffer;
    unsigned length = bytes[0];
    bool valid = transfer->result == XFER_RESULT_SUCCESS && transfer->actual_len >= 2 &&
        bytes[1] == TUSB_DESC_STRING && length >= 2 && !(length & 1) && length <= transfer->actual_len;
    if (valid && interface->identity_step == 0 && length >= 4) {
        interface->language = (uint16_t)bytes[2] | (uint16_t)bytes[3] << 8;
    } else if (valid && interface->identity_step == 1) {
        unsigned used = 0;
        for (unsigned i = 2; i + 1 < length && used + 1 < sizeof(interface->product_name); i += 2) {
            unsigned value = bytes[i] | (unsigned)bytes[i + 1] << 8;
            interface->product_name[used++] = value >= 32 && value < 127 ? value : '?';
        }
        interface->product_name[used] = 0;
    } else if (valid && interface->identity_step == 2 && length > 2) {
        uint64_t hash = UINT64_C(14695981039346656037);
        for (unsigned i = 2; i < length; ++i) hash = (hash ^ bytes[i]) * UINT64_C(1099511628211);
        interface->serial_hash = hash;
        interface->have_serial = true;
    }
    ++interface->identity_step;
}

static void identify_ports(void) {
    if (identity_index >= 0 || atomic_load(&stopping)) return;
    for (unsigned index = 0; index < USB_INTERFACES; ++index) {
        struct interface *interface = &interfaces[index];
        if (!interface->identifying) continue;
        if (interface->identity_step == 1 && !interface->device.iProduct) ++interface->identity_step;
        if (interface->identity_step == 2 && !interface->device.iSerialNumber) ++interface->identity_step;
        if (interface->identity_step >= 3) {
            interface->identifying = false;
            publish_mount(index);
            continue;
        }
        uint8_t string_index = interface->identity_step == 0 ? 0 :
            interface->identity_step == 1 ? interface->device.iProduct : interface->device.iSerialNumber;
        if (++identity_token == 0) ++identity_token;
        if (tuh_descriptor_get_string(interface->mount.daddr, string_index,
            string_index ? interface->language : 0, identity_buffer, sizeof(identity_buffer),
            identity_complete, identity_token)) {
            interface->identity_token = identity_token;
            identity_index = index;
            return;
        }
    }
}

uint64_t shoop_usb_micros(void) {
    XTime ticks; XTime_GetTime(&ticks);
    const uint64_t frequency = XPAR_CPU_CORTEXA9_0_CPU_CLK_FREQ_HZ / 2;
    return ticks / frequency * 1000000 + ticks % frequency * 1000000 / frequency;
}

static bool pending(void) {
    if (identity_index >= 0) return true;
    for (unsigned i = 0; i < USB_INTERFACES; ++i) {
        if (interfaces[i].head != interfaces[i].tail) return true;
    }
    return false;
}

static void receive_packets(uint64_t now) {
    uint8_t packet[4];
    for (unsigned index = 0; index < USB_INTERFACES; ++index) {
        if (!interfaces[index].active) continue;
        for (unsigned count = 0; count < 128 && tuh_midi_packet_read(index, packet); ++count) {
            if (!atomic_load(&stopping)) shoop_usb_receive_packet(index, packet, now);
        }
    }
}

static bool transmit_port(unsigned slot, unsigned index, uint64_t now) {
    struct interface *interface = &interfaces[index];
    if (!interface->active || (uint16_t)(interface->head - interface->tail) >= USB_PENDING_PACKETS) return false;
    uint8_t packet[4];
    struct shoop_usb_packet_owner owner;
    if (!shoop_usb_prepare_packet(slot, now, packet, &owner) || !tuh_midi_packet_write(index, packet)) return false;
    interface->owners[interface->head++ % USB_PENDING_PACKETS] = owner;
    atomic_store(&shoop_usb_pending_packets[index], (uint16_t)(interface->head - interface->tail));
    shoop_usb_accept_packet(owner);
    return true;
}

static void transmit_packets(uint64_t now) {
    uint8_t routes[SHOOP_MIDI_PORTS];
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        routes[i] = shoop_usb_output_index(i);
    }
    unsigned written = 0, idle = 0;
    for (unsigned budget = 0; budget < 128 * SHOOP_MIDI_PORTS && written < 128; ++budget) {
        unsigned slot = cursor++ % SHOOP_MIDI_PORTS;
        if (routes[slot] < USB_INTERFACES && transmit_port(slot, routes[slot], now)) {
            ++written;
            idle = 0;
        } else if (++idle == SHOOP_MIDI_PORTS) break;
    }
    for (unsigned i = 0; i < USB_INTERFACES; ++i) {
        if (interfaces[i].active) tuh_midi_write_flush(i);
    }
}

static void usb_run(void *unused) {
    (void)unused;
    tusb_rhport_init_t init = { .role = TUSB_ROLE_HOST, .speed = TUSB_SPEED_HIGH };
    if (!tusb_init(0, &init)) {
        xil_printf("SHOOP_USB_FAILED init\r\n");
        atomic_store(&stopped, -1);
        vTaskSuspend(NULL);
    }
    xil_printf("SHOOP_USB_READY TinyUSB host\r\n");
    uint64_t stop_at = 0, last_diagnostic = 0;
    bool drained = true;
    for (;;) {
        tuh_task_ext(0, false);
        if (shoop_usb_controller_failed()) {
            ++transfer_errors;
            for (unsigned i = 0; i < USB_INTERFACES; ++i) tuh_midi_umount_cb(i);
            drained = false;
            break;
        }
        identify_ports();
        uint64_t now = shoop_usb_micros();
        shoop_usb_expire(now);
        receive_packets(now);
        transmit_packets(now);
        if (now - last_diagnostic >= 1000000) {
            last_diagnostic = now;
            for (unsigned slot = 0; slot < SHOOP_MIDI_PORTS; ++slot) {
                struct shoop_usb_counters counters;
                shoop_usb_counters(1, slot, &counters);
                if (counters.refused || counters.dropped || counters.recovery_requests)
                    xil_printf("SHOOP_USB_PRESSURE slot=%u refused=%u dropped=%u recovery_requests=%u queued=%u accepted=%u completed=%u packets=%u\r\n",
                        slot, counters.refused, counters.dropped, counters.recovery_requests, counters.queued,
                        counters.received, counters.completed, counters.packets);
            }
        }
        if (atomic_load(&stopping)) {
            if (!stop_at) stop_at = now;
            if (shoop_usb_drained() && !pending()) break;
            if (now - stop_at >= 5000000) { drained = false; break; }
        }
        vTaskDelay(1);
    }
    bool deinitialized = tuh_deinit(0);
    for (unsigned direction = 0; direction < 2; ++direction) {
        for (unsigned slot = 0; slot < SHOOP_MIDI_PORTS; ++slot) {
            struct shoop_usb_counters counters;
            shoop_usb_counters(direction, slot, &counters);
            xil_printf("SHOOP_USB_COUNTER direction=%u slot=%u received=%u completed=%u refused=%u dropped=%u malformed=%u assembly_dropped=%u recovery_requests=%u packets=%u queued=%u\r\n",
                direction, slot, counters.received, counters.completed, counters.refused, counters.dropped,
                counters.malformed, counters.assembly_dropped, counters.recovery_requests, counters.packets, counters.queued);
        }
    }
    xil_printf("SHOOP_USB_STOP drained=%u controller_stopped=%u lost_packets=%u errors=%u stack_free=%u elapsed_us=%u\r\n",
        drained, deinitialized, lost_packets, transfer_errors, (unsigned)uxTaskGetStackHighWaterMark(NULL),
        stop_at ? (unsigned)(shoop_usb_micros() - stop_at) : 0);
    atomic_store(&stopped, drained && deinitialized ? 1 : -1);
    vTaskSuspend(NULL);
}

void shoop_usb_start(void) {
    configASSERT(!usb_task);
    configASSERT(xTaskCreate(usb_run, "usb-host", 8192, NULL, 5, &usb_task) == pdPASS);
}

void shoop_usb_begin_stop(void) {
    if (!atomic_exchange(&stopping, 1)) shoop_usb_recover_all();
}

int shoop_usb_stop(void) {
    if (!usb_task) return 1;
    shoop_usb_begin_stop();
    static uint64_t deadline;
    if (!deadline) deadline = shoop_usb_micros() + 5500000;
    if (!atomic_load(&stopped)) return shoop_usb_micros() < deadline ? 2 : 0;
    if (atomic_load(&stopped) != 1) return 0;
    vTaskDelete(usb_task);
    usb_task = NULL;
    return 1;
}

static void publish_mount(uint8_t index) {
    if (index >= USB_INTERFACES || atomic_load(&stopping)) return;
    struct interface *interface = &interfaces[index];
    const tuh_midi_mount_cb_t *info = &interface->mount;
    uint16_t vendor, product;
    if (!tuh_vid_pid_get(info->daddr, &vendor, &product)) return;
    tuh_bus_info_t bus;
    uint8_t hops[3], count = 0, address = info->daddr;
    do {
        if (count == sizeof(hops) || !tuh_bus_info_get(address, &bus)) return;
        hops[count++] = bus.hub_addr ? bus.hub_port : 1;
        address = bus.hub_addr;
    } while (address);
    char path[24] = "usb0", key[96], name[96];
    while (count) {
        size_t used = strlen(path);
        snprintf(path + used, sizeof(path) - used, "/%u", hops[--count]);
    }
    if (interface->have_serial) {
        snprintf(key, sizeof(key), "%s:%04x:%04x:s%016llx:if%u", path, vendor, product,
            (unsigned long long)interface->serial_hash, info->bInterfaceNumber);
    } else snprintf(key, sizeof(key), "%s:%04x:%04x:if%u", path, vendor, product, info->bInterfaceNumber);
    if (interface->product_name[0]) snprintf(name, sizeof(name), "%.48s %s if%u", interface->product_name, path, info->bInterfaceNumber);
    else snprintf(name, sizeof(name), "USB %04x:%04x %s if%u", vendor, product, path, info->bInterfaceNumber);
    if (!shoop_usb_mount(index, info->daddr, info->bInterfaceNumber, info->rx_cable_count,
                         info->tx_cable_count, vendor, product, tuh_speed_get(info->daddr), key, name)) {
        xil_printf("SHOOP_USB_MIDI_REFUSED index=%u capacity\r\n", index);
        return;
    }
    interfaces[index].active = true;
    xil_printf("SHOOP_USB_MIDI_MOUNT index=%u addr=%u interface=%u rx=%u tx=%u\r\n",
        index, info->daddr, info->bInterfaceNumber, info->rx_cable_count, info->tx_cable_count);
}

void tuh_midi_mount_cb(uint8_t index, const tuh_midi_mount_cb_t *info) {
    if (index >= USB_INTERFACES || atomic_load(&stopping)) return;
    struct interface *interface = &interfaces[index];
    memset(interface, 0, sizeof(*interface));
    interface->mount = *info;
    interface->language = 0x0409;
    if (!tuh_descriptor_get_device_local(info->daddr, &interface->device)) return;
    interface->identifying = true;
}

void tuh_midi_umount_cb(uint8_t index) {
    if (index >= USB_INTERFACES) return;
    struct interface *interface = &interfaces[index];
    lost_packets += (uint16_t)(interface->head - interface->tail);
    interface->active = false;
    interface->identifying = false;
    interface->tail = interface->head;
    atomic_store(&shoop_usb_pending_packets[index], 0);
    shoop_usb_unmount(index);
    xil_printf("SHOOP_USB_MIDI_UNMOUNT index=%u\r\n", index);
}

void tuh_midi_tx_cb(uint8_t index, uint32_t transferred) {
    if (index >= USB_INTERFACES) return;
    struct interface *interface = &interfaces[index];
    if (transferred % 4 || transferred / 4 > (uint16_t)(interface->head - interface->tail)) {
        ++transfer_errors;
        tuh_midi_umount_cb(index);
        return;
    }
    for (unsigned count = 0; count < transferred / 4; ++count) {
        shoop_usb_complete_packet(interface->owners[interface->tail++ % USB_PENDING_PACKETS]);
    }
    atomic_store(&shoop_usb_pending_packets[index], (uint16_t)(interface->head - interface->tail));
}

void tuh_midi_rx_cb(uint8_t index, uint32_t transferred) {
    if (transferred % 4) {
        ++transfer_errors;
        tuh_midi_umount_cb(index);
    }
}

void tuh_midi_error_cb(uint8_t index, uint8_t endpoint, xfer_result_t result) {
    ++transfer_errors;
    xil_printf("SHOOP_USB_MIDI_ERROR index=%u endpoint=%u result=%u\r\n", index, endpoint, result);
    tuh_midi_umount_cb(index);
}
