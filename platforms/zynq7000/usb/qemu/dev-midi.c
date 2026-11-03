/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "qemu/osdep.h"
#include "qapi/error.h"
#include "qemu/module.h"
#include "hw/qdev-properties.h"
#include "hw/qdev-properties-system.h"
#include "hw/usb.h"
#include "chardev/char-fe.h"
#include "desc.h"

#define TYPE_SHOOP_MIDI "usb-shoop-midi"
OBJECT_DECLARE_SIMPLE_TYPE(ShoopMidi, SHOOP_MIDI)
#define MIDI_RING 16384

struct ShoopMidi {
    USBDevice parent;
    CharBackend chardev;
    USBEndpoint *input;
    uint8_t rx[MIDI_RING], tx[MIDI_RING];
    unsigned rx_read, rx_write, tx_read, tx_write;
    guint watch;
    uint8_t cables;
    bool highspeed, stall_in, stall_out, nak_out;
    USBDesc desc;
    USBDescDevice device[2];
    USBDescConfig config[2];
    USBDescIface interfaces[2][2];
    USBDescEndpoint endpoints[2][2];
    USBDescOther control_extra, midi_extra[17];
    uint8_t ac[9], ms[7], jacks[16][30], association[2][20];
};

static const USBDescStrings strings = {
    [1] = "Shoop test fixture", [2] = "Class-compliant MIDI", [3] = "shoop-midi",
};

static void drain_output(ShoopMidi *s);

static gboolean output_ready(void *channel, GIOCondition condition, void *opaque)
{
    ShoopMidi *s = opaque;
    s->watch = 0;
    drain_output(s);
    return G_SOURCE_REMOVE;
}

static void drain_output(ShoopMidi *s)
{
    while (s->tx_read != s->tx_write) {
        unsigned position = s->tx_read % MIDI_RING;
        unsigned count = MIN(s->tx_write - s->tx_read, MIDI_RING - position);
        int written = qemu_chr_fe_write(&s->chardev, s->tx + position, count);
        if (written <= 0) {
            if (!s->watch) {
                s->watch = qemu_chr_fe_add_watch(&s->chardev, G_IO_OUT | G_IO_HUP,
                                               output_ready, s);
            }
            return;
        }
        s->tx_read += written;
    }
}

static int input_space(void *opaque)
{
    ShoopMidi *s = opaque;
    return MIDI_RING - (s->rx_write - s->rx_read);
}

static void input_bytes(void *opaque, const uint8_t *bytes, int count)
{
    ShoopMidi *s = opaque;
    for (int i = 0; i < count; ++i) {
        s->rx[s->rx_write++ % MIDI_RING] = bytes[i];
    }
    usb_wakeup(s->input, 0);
}

static void handle_control(USBDevice *dev, USBPacket *packet, int request,
                           int value, int index, int length, uint8_t *data)
{
    if (usb_desc_handle_control(dev, packet, request, value, index, length, data) >= 0) {
        return;
    }
    packet->status = USB_RET_STALL;
}

static void handle_data(USBDevice *dev, USBPacket *packet)
{
    ShoopMidi *s = SHOOP_MIDI(dev);
    uint8_t bytes[512];
    if (packet->ep->nr != 1) {
        packet->status = USB_RET_STALL;
    } else if (packet->pid == USB_TOKEN_IN) {
        unsigned count = MIN((s->rx_write - s->rx_read) & ~3u,
                             MIN(packet->iov.size, sizeof(bytes)));
        if (s->stall_in) {
            packet->status = USB_RET_STALL;
        } else if (!count) {
            packet->status = USB_RET_NAK;
        } else {
            for (unsigned i = 0; i < count; ++i) {
                bytes[i] = s->rx[s->rx_read++ % MIDI_RING];
            }
            usb_packet_copy(packet, bytes, count);
            qemu_chr_fe_accept_input(&s->chardev);
        }
    } else if (packet->pid == USB_TOKEN_OUT) {
        unsigned count = packet->iov.size;
        if (s->nak_out) {
            packet->status = USB_RET_NAK;
        } else if (s->stall_out || count > sizeof(bytes) || (count & 3)) {
            packet->status = USB_RET_STALL;
        } else if (count > MIDI_RING - (s->tx_write - s->tx_read)) {
            packet->status = USB_RET_NAK;
        } else {
            usb_packet_copy(packet, bytes, count);
            for (unsigned i = 0; i < count; ++i) {
                s->tx[s->tx_write++ % MIDI_RING] = bytes[i];
            }
            drain_output(s);
        }
    } else {
        packet->status = USB_RET_STALL;
    }
}

static void reset(USBDevice *dev)
{
    ShoopMidi *s = SHOOP_MIDI(dev);
    s->rx_read = s->rx_write = s->tx_read = s->tx_write = 0;
}

static void realize(USBDevice *dev, Error **errp)
{
    ShoopMidi *s = SHOOP_MIDI(dev);
    if (!s->cables || s->cables > 16 || !qemu_chr_fe_backend_connected(&s->chardev)) {
        error_setg(errp, "A MIDI chardev and 1..16 cables are required");
        return;
    }
    memcpy(s->ac, (uint8_t[]){9, 0x24, 1, 0, 1, 9, 0, 1, 1}, 9);
    unsigned total = 33 + 32 * s->cables;
    memcpy(s->ms, (uint8_t[]){7, 0x24, 1, 0, 1, total & 255, total >> 8}, 7);
    s->control_extra = (USBDescOther){ .length = 9, .data = s->ac };
    s->midi_extra[0] = (USBDescOther){ .length = 7, .data = s->ms };
    for (unsigned i = 0; i < s->cables; ++i) {
        unsigned id = 4 * i + 1;
        memcpy(s->jacks[i], (uint8_t[]){
            6, 0x24, 2, 1, id, 0,
            6, 0x24, 2, 2, id + 1, 0,
            9, 0x24, 3, 1, id + 2, 1, id + 1, 1, 0,
            9, 0x24, 3, 2, id + 3, 1, id, 1, 0}, 30);
        s->midi_extra[i + 1] = (USBDescOther){ .length = 30, .data = s->jacks[i] };
        s->association[0][4 + i] = id;
        s->association[1][4 + i] = id + 2;
    }
    for (unsigned direction = 0; direction < 2; ++direction) {
        memcpy(s->association[direction], (uint8_t[]){4 + s->cables, 0x25, 1, s->cables}, 4);
    }
    for (unsigned speed = 0; speed < 2; ++speed) {
        for (unsigned direction = 0; direction < 2; ++direction) {
            s->endpoints[speed][direction] = (USBDescEndpoint){
                .bEndpointAddress = 1 | (direction ? USB_DIR_IN : 0),
                .bmAttributes = USB_ENDPOINT_XFER_BULK,
                .wMaxPacketSize = speed ? 512 : 64,
                .is_audio = 1, .extra = s->association[direction],
            };
        }
        s->interfaces[speed][0] = (USBDescIface){
            .bInterfaceNumber = 0, .bInterfaceClass = 1, .bInterfaceSubClass = 1,
            .ndesc = 1, .descs = &s->control_extra,
        };
        s->interfaces[speed][1] = (USBDescIface){
            .bInterfaceNumber = 1, .bInterfaceClass = 1, .bInterfaceSubClass = 3,
            .bNumEndpoints = 2, .ndesc = 1 + s->cables,
            .descs = s->midi_extra, .eps = s->endpoints[speed],
        };
        s->config[speed] = (USBDescConfig){
            .bNumInterfaces = 2, .bConfigurationValue = 1,
            .bmAttributes = USB_CFG_ATT_ONE | USB_CFG_ATT_SELFPOWER,
            .nif = 2, .ifs = s->interfaces[speed],
        };
        s->device[speed] = (USBDescDevice){
            .bcdUSB = 0x0200, .bMaxPacketSize0 = 64,
            .bNumConfigurations = 1, .confs = &s->config[speed],
        };
    }
    s->desc = (USBDesc){
        .id = {.idVendor = 0xcafe, .idProduct = 0x4010, .bcdDevice = 0x0100,
               .iManufacturer = 1, .iProduct = 2, .iSerialNumber = 3},
        .full = &s->device[0], .high = s->highspeed ? &s->device[1] : NULL, .str = strings,
    };
    dev->usb_desc = &s->desc;
    usb_desc_create_serial(dev);
    usb_desc_init(dev);
    s->input = usb_ep_get(dev, USB_TOKEN_IN, 1);
    qemu_chr_fe_set_handlers(&s->chardev, input_space, input_bytes, NULL, NULL, s, NULL, true);
}

static void unrealize(USBDevice *dev)
{
    ShoopMidi *s = SHOOP_MIDI(dev);
    if (s->watch) {
        g_source_remove(s->watch);
        s->watch = 0;
    }
    qemu_chr_fe_deinit(&s->chardev, false);
}

static const Property properties[] = {
    DEFINE_PROP_CHR("chardev", ShoopMidi, chardev),
    DEFINE_PROP_UINT8("cables", ShoopMidi, cables, 1),
    DEFINE_PROP_BOOL("high-speed", ShoopMidi, highspeed, true),
    DEFINE_PROP_BOOL("stall-in", ShoopMidi, stall_in, false),
    DEFINE_PROP_BOOL("stall-out", ShoopMidi, stall_out, false),
    DEFINE_PROP_BOOL("nak-out", ShoopMidi, nak_out, false),
};

static void class_init(ObjectClass *klass, void *data)
{
    DeviceClass *dc = DEVICE_CLASS(klass);
    USBDeviceClass *uc = USB_DEVICE_CLASS(klass);
    dc->desc = "Deterministic USB MIDI 1.0 fixture";
    uc->product_desc = "Shoop USB MIDI";
    uc->realize = realize;
    uc->unrealize = unrealize;
    uc->handle_reset = reset;
    uc->handle_control = handle_control;
    uc->handle_data = handle_data;
    device_class_set_props(dc, properties);
}

static const TypeInfo info = {
    .name = TYPE_SHOOP_MIDI, .parent = TYPE_USB_DEVICE,
    .instance_size = sizeof(ShoopMidi), .class_init = class_init,
};

static void register_types(void)
{
    type_register_static(&info);
}
type_init(register_types)
