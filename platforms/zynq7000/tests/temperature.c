#include "temperature.h"
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

struct mock {
    uint32_t cfg, control, response, pipeline, status;
    uint32_t current, peak;
    unsigned commands, reads, writes, data_reads, interface_releases;
    bool ready, stalled, busy, fail_peak;
};
static uint32_t read_register(void *context, unsigned offset) {
    struct mock *mock = context;
    ++mock->reads;
    assert(mock->reads < 20000);
    switch (offset) {
    case 0: return mock->cfg;
    case 0x18: return mock->control;
    case 0x0c: return mock->status | (mock->busy ? 0 : 1u << 10)
                            | (mock->ready ? 0 : 1u << 8);
    case 0x14:
        assert(mock->ready);
        mock->ready = false;
        ++mock->data_reads;
        return mock->response;
    default: assert(false); return 0;
    }
}
static void write_register(void *context, unsigned offset, uint32_t value) {
    struct mock *mock = context;
    ++mock->writes;
    if (offset == 0) {
        assert(value == (mock->cfg | 0x8000031fu));
        mock->cfg = value;
        return;
    }
    if (offset == 0x18) {
        assert(mock->control & (1u << 4));
        assert(value == (mock->control & ~(1u << 4)));
        assert(!(value & (1u << 4))); /* Release only; never assert interface or ADC reset. */
        mock->control = value;
        ++mock->interface_releases;
        return;
    }
    assert(offset == 0x10); /* No interrupt acknowledgement or ADC register writes. */
    assert(value == 0 || value == 0x04000000u || value == 0x04200000u);
    assert(!mock->ready);
    ++mock->commands;
    mock->response = mock->pipeline;
    mock->ready = !mock->stalled && !(mock->fail_peak && value == 0x04200000u);
    if (value == 0x04000000u) mock->pipeline = mock->current;
    if (value == 0x04200000u) mock->pipeline = mock->peak;
}
static struct shoop_temperature sample(struct mock *mock) {
    struct shoop_temperature output;
    memset(&output, 0xff, sizeof output);
    const struct shoop_xadc_bus bus = {mock, read_register, write_register};
    shoop_xadc_sample(&bus, &output);
    return output;
}
static struct mock normal(void) {
    return (struct mock){.cfg = 0x00402000u, .current = 0xa000, .peak = 0xb000,
                         .pipeline = 0x08000000u};
}
int main(void) {
    _Static_assert(sizeof(struct shoop_temperature) == 12, "Rust/C temperature ABI");
    struct mock mock = normal();
    struct shoop_temperature reading = sample(&mock);
    assert(reading.current_millicelsius == 41834);
    assert(reading.peak_millicelsius == 73333);
    assert(reading.flags == 7);
    assert(mock.commands == 4 && mock.data_reads == 4);
    assert(mock.cfg == 0x8040231fu);

    mock = normal();
    mock.ready = true;
    mock.response = 0xffff; /* A stale FIFO entry must not become a temperature. */
    mock.status = 1u << 7;
    reading = sample(&mock);
    assert(reading.current_millicelsius == 41834 && reading.flags == 15);
    assert(mock.data_reads == 5);
    unsigned writes = mock.writes;
    mock.reads = 0;
    reading = sample(&mock);
    assert(reading.flags == 15 && mock.writes == writes + 4);

    mock = normal(); mock.current = 0x8000;
    reading = sample(&mock);
    assert(reading.current_millicelsius == -21162);
    mock = normal(); mock.current = 0;
    assert(sample(&mock).flags == 0);
    mock = normal(); mock.current = 0xffff;
    assert(sample(&mock).flags == 0);
    mock = normal(); mock.peak = 0;
    assert(sample(&mock).flags == 5);
    mock = normal(); mock.fail_peak = true;
    assert(sample(&mock).flags == 5);

    mock = normal(); mock.stalled = true;
    assert(sample(&mock).flags == 0 && mock.commands == 1);
    mock = normal(); mock.busy = true;
    assert(sample(&mock).flags == 0 && mock.commands == 0);
    mock = normal(); mock.cfg = UINT32_MAX;
    assert(sample(&mock).flags == 0 && mock.writes == 0);
    mock = normal(); mock.control = 1u << 4;
    assert(sample(&mock).flags == 7 && mock.interface_releases == 1 && mock.control == 0);
    mock.reads = 0;
    assert(sample(&mock).flags == 7 && mock.interface_releases == 1);
    mock = normal(); mock.control = 1u << 4; mock.ready = true;
    assert(sample(&mock).flags == 0 && mock.interface_releases == 0 && mock.commands == 0);
    mock = normal(); mock.control = 1u << 4; mock.busy = true;
    assert(sample(&mock).flags == 0 && mock.interface_releases == 0 && mock.commands == 0);
    mock = normal(); mock.status = UINT32_MAX;
    assert(sample(&mock).flags == 0 && mock.commands == 0);
    puts("XADC temperature conversion, FIFO alignment, bounded failures and read-only ADC access: passed");
}
