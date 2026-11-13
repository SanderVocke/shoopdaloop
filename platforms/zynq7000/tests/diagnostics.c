#include "diagnostics.h"
#include <assert.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

static unsigned suspended;
static unsigned pending;
static char output[256];
static size_t length;
static const char ready[] = "SHOOP_HOST_READY\n";

static void switch_task(void) {
    unsigned task = pending;
    pending = 0;
    if (task == 1) shoop_logf("SHOOP_USB_MIDI_MOUNT index=%u\n", 3u);
    if (task == 2) shoop_log((const unsigned char *)ready, strlen(ready));
}
void vTaskSuspendAll(void) { ++suspended; }
int xTaskResumeAll(void) {
    assert(suspended);
    if (!--suspended && pending) switch_task();
    return 0;
}
void outbyte(char byte) {
    assert(length < sizeof(output) - 1);
    output[length++] = byte;
    output[length] = 0;
    if (!suspended && pending) switch_task();
}
void xil_printf(const char *format, ...) {
    char buffer[128];
    va_list arguments;
    va_start(arguments, format);
    int count = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    assert(count >= 0 && (size_t)count < sizeof(buffer));
    for (int i = 0; i < count; ++i) outbyte(buffer[i]);
}
int main(void) {
    for (unsigned repetition = 0; repetition < 100; ++repetition) {
        length = 0;
        pending = 1;
        shoop_log((const unsigned char *)ready, strlen(ready));
        assert(strcmp(output, "SHOOP_HOST_READY\nSHOOP_USB_MIDI_MOUNT index=3\n") == 0);
        assert(!suspended && !pending);
        length = 0;
        pending = 2;
        shoop_logf("SHOOP_USB_MIDI_MOUNT index=%u\n", 3u);
        assert(strcmp(output, "SHOOP_USB_MIDI_MOUNT index=3\nSHOOP_HOST_READY\n") == 0);
        assert(!suspended && !pending);
    }
    return 0;
}
