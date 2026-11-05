#ifndef SHOOP_USB_HOST_H
#define SHOOP_USB_HOST_H
#include <stdint.h>
#include <stdbool.h>

void shoop_usb_start(void);
int shoop_usb_stop(void);
void shoop_usb_begin_stop(void);
uint64_t shoop_usb_micros(void);
bool shoop_usb_controller_failed(void);

#endif
