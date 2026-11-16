#ifndef SHOOP_USB_BRIDGE_H
#define SHOOP_USB_BRIDGE_H

#include "midi.h"

struct shoop_usb_port_info {
    uint32_t generation, connected, direction;
    uint16_t vendor, product;
    uint8_t slot, index, address, interface_number, cable, speed, reserved[2];
    char key[128], name[128];
};

struct shoop_usb_packet_owner {
    uint32_t generation;
    uint8_t slot, recovery, last, reserved;
};

struct shoop_usb_counters {
    uint32_t received, completed, refused, dropped, malformed, assembly_dropped, recovery_requests, packets, queued;
};

bool shoop_usb_mount(uint8_t index, uint8_t address, uint8_t interface_number,
                     uint8_t rx, uint8_t tx, uint16_t vendor, uint16_t product,
                     uint8_t speed, const char *key, const char *name);
void shoop_usb_unmount(uint8_t index);
void shoop_usb_receive_packet(uint8_t index, const uint8_t packet[4], uint64_t now);
uint8_t shoop_usb_output_index(unsigned slot);
bool shoop_usb_prepare_packet(unsigned slot, uint64_t now, uint8_t packet[4],
                              struct shoop_usb_packet_owner *owner);
void shoop_usb_accept_packet(struct shoop_usb_packet_owner owner);
void shoop_usb_complete_packet(struct shoop_usb_packet_owner owner);
bool shoop_usb_port_snapshot(unsigned direction, unsigned slot, struct shoop_usb_port_info *);
size_t shoop_usb_read(unsigned slot, uint8_t *bytes, size_t capacity, uint64_t *arrival);
uint32_t shoop_usb_input_epoch(unsigned slot);
uint32_t shoop_usb_begin_input(unsigned slot);
void shoop_usb_refuse_input(unsigned slot);
size_t shoop_usb_output_capacity(unsigned slot);
bool shoop_usb_write(unsigned slot, const uint8_t *bytes, size_t len, uint64_t due);
void shoop_usb_recover_all(void);
void shoop_usb_recover_port(unsigned slot);
bool shoop_usb_drained(void);
void shoop_usb_expire(uint64_t now);
void shoop_usb_release(unsigned direction, unsigned slot, uint32_t generation);
bool shoop_usb_counters(unsigned direction, unsigned slot, struct shoop_usb_counters *);

#endif
