#include "yt8531.h"
#include <assert.h>

struct fixture {
    uint16_t registers[32];
    uint16_t extended[2];
    unsigned selected;
    unsigned writes;
    unsigned elapsed_ms;
    unsigned status_reads;
    bool link_partner;
    bool broken;
};

static bool read_register(void *context, unsigned reg, uint16_t *value)
{
    struct fixture *f = context;
    if (f->broken)
        return false;
    if (reg == 1) {
        f->status_reads++;
        *value = f->link_partner && f->elapsed_ms >= 40 && f->status_reads % 2 == 0 ? 0x24 : 0;
    } else if (reg == 0x1f) {
        *value = f->extended[f->selected];
    } else {
        *value = f->registers[reg];
    }
    return true;
}

static bool write_register(void *context, unsigned reg, uint16_t value)
{
    struct fixture *f = context;
    f->writes++;
    if (reg == 0x1e) {
        assert(value == 0xa001 || value == 0xa003);
        f->selected = value == 0xa003;
    } else if (reg == 0x1f) {
        f->extended[f->selected] = value;
    } else {
        f->registers[reg] = value;
    }
    return true;
}

static void delay(void *context, unsigned milliseconds)
{
    struct fixture *f = context;
    f->elapsed_ms += milliseconds;
}

int main(void)
{
    struct fixture f;
    const struct shoop_mdio bus = {&f, read_register, write_register, delay};
    const struct fixture ready = {
        .registers = {[0] = 0x0c00, [2] = 0x4f51, [3] = 0xe91b, [4] = 0x0aa1, [9] = 0x100},
        .extended = {0xffff, 0xffff},
        .link_partner = true,
    };
    const unsigned speeds[] = {10, 100, 1000};
    for (unsigned mode = 0; mode < 3; ++mode) {
        f = ready;
        f.registers[0x11] = (uint16_t)((mode << 14) | 0x2000);
        assert(shoop_yt8531_start(&bus) == speeds[mode]);
        assert(f.extended[0] == 0xfeff && f.extended[1] == 0xf7fd);
        assert(f.registers[0] == 0x1200 && f.registers[4] == 0x0941 && f.registers[9] == 0x0200);
        assert(f.elapsed_ms == 40);
    }
    f = ready;
    f.registers[3] ^= 1;
    assert(shoop_yt8531_start(&bus) == 0 && f.writes == 0);
    f = ready;
    f.broken = true;
    assert(shoop_yt8531_start(&bus) == 0 && f.writes == 0);
    f = ready;
    f.link_partner = false;
    assert(shoop_yt8531_start(&bus) == 0 && f.elapsed_ms == 5000);
    f = ready;
    assert(shoop_yt8531_start(&bus) == 0 && f.elapsed_ms == 40);
    return 0;
}
