#ifndef SHOOP_DIAGNOSTICS_H
#define SHOOP_DIAGNOSTICS_H

#include "FreeRTOS.h"
#include "task.h"
#include "xil_printf.h"
#include <stddef.h>

void shoop_log(const unsigned char *bytes, size_t len);

/* Task-context diagnostics keep each message intact without masking interrupts. */
#define shoop_logf(...) do { \
    vTaskSuspendAll(); \
    xil_printf(__VA_ARGS__); \
    (void)xTaskResumeAll(); \
} while (0)

#endif
