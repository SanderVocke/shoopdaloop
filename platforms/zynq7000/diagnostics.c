#include "diagnostics.h"

void shoop_log(const unsigned char *bytes, size_t len) {
    extern void outbyte(char);
    vTaskSuspendAll();
    for (size_t i = 0; i < len; ++i) outbyte(bytes[i]);
    (void)xTaskResumeAll();
}
