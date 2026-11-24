#include "temperature.h"
#include <stdbool.h>

#define CFG 0x00u
#define STATUS 0x0cu
#define COMMAND 0x10u
#define DATA 0x14u
#define CONTROL 0x18u
#define ENABLE (1u << 31)
#define COMMAND_FULL (1u << 11)
#define COMMAND_EMPTY (1u << 10)
#define DATA_EMPTY (1u << 8)
#define INTERFACE_RESET (1u << 4)
#define POLLS 4096u

static bool wait_status(const struct shoop_xadc_bus *bus, uint32_t mask, uint32_t expected) {
    for (unsigned i = 0; i < POLLS; ++i) {
        uint32_t status = bus->read(bus->context, STATUS);
        if (status == UINT32_MAX) return false;
        if ((status & mask) == expected) return true;
    }
    return false;
}

static bool exchange(const struct shoop_xadc_bus *bus, uint32_t command, uint32_t *data) {
    if (!wait_status(bus, COMMAND_FULL, 0)) return false;
    bus->write(bus->context, COMMAND, command);
    if (!wait_status(bus, DATA_EMPTY, 0)) return false;
    *data = bus->read(bus->context, DATA);
    return true;
}

static bool read_adc(const struct shoop_xadc_bus *bus, unsigned address, int32_t *temperature) {
    uint32_t raw;
    if (!exchange(bus, 0x04000000u | (address << 16), &raw)
        || !exchange(bus, 0, &raw)) return false;
    raw &= 0xfff0u;
    if (!raw || raw == 0xfff0u) return false;
    *temperature = (int32_t)(((uint64_t)raw * 503975u + 32768u) / 65536u) - 273150;
    return true;
}

void shoop_xadc_sample(const struct shoop_xadc_bus *bus, struct shoop_temperature *sample) {
    *sample = (struct shoop_temperature){0};
    uint32_t cfg = bus->read(bus->context, CFG);
    uint32_t control = bus->read(bus->context, CONTROL);
    if (cfg == UINT32_MAX || control == UINT32_MAX) return;
    /* Only configure the PS serial interface, never the ADC or its protection registers. */
    uint32_t enabled = cfg | ENABLE | 0x31fu;
    if (enabled != cfg) bus->write(bus->context, CFG, enabled);
    if (control & INTERFACE_RESET) {
        uint32_t status = bus->read(bus->context, STATUS);
        if (status == UINT32_MAX ||
            (status & (COMMAND_EMPTY | DATA_EMPTY)) != (COMMAND_EMPTY | DATA_EMPTY)) return;
        bus->write(bus->context, CONTROL, control & ~INTERFACE_RESET);
    }
    if (!wait_status(bus, COMMAND_EMPTY, COMMAND_EMPTY)) return;
    unsigned drained = 0;
    while (!(bus->read(bus->context, STATUS) & DATA_EMPTY)) {
        if (drained++ == 16) return;
        (void)bus->read(bus->context, DATA);
    }
    if (read_adc(bus, 0x00, &sample->current_millicelsius)) {
        sample->flags |= SHOOP_TEMPERATURE_CURRENT;
        if (read_adc(bus, 0x20, &sample->peak_millicelsius))
            sample->flags |= SHOOP_TEMPERATURE_PEAK;
        uint32_t status = bus->read(bus->context, STATUS);
        if (status != UINT32_MAX) {
            sample->flags |= SHOOP_TEMPERATURE_ALARM_VALID;
            if (status & (1u << 7)) sample->flags |= SHOOP_TEMPERATURE_ALARM;
        }
    }
}
