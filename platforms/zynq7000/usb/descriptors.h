#ifndef SHOOP_USB_DESCRIPTORS_H
#define SHOOP_USB_DESCRIPTORS_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
bool shoop_usb_configuration_valid(const uint8_t *bytes, size_t length);
#endif
