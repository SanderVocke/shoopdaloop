#include "yt8531.h"
#include <assert.h>

struct fixture {
    uint16_t registers[32];
    uint16_t extended[3];
    unsigned selected;
    unsigned writes;
    unsigned elapsed_ms;
    unsigned status_reads;
    unsigned reads;
    unsigned fail_read;
    unsigned fail_write;
    bool link_partner;
    bool broken;
};

static bool read_register(void *context, unsigned reg, uint16_t *value)
{
    struct fixture *f = context;
    f->reads++;
    if (f->broken || f->reads == f->fail_read)
        return false;
    if (f->registers[3] == 0xe91a && (reg == 0 || reg == 1 || reg == 4 || reg == 9))
        assert(!(f->extended[2] & 2));
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
    if (f->writes == f->fail_write)
        return false;
    if (reg == 0x1e) {
        assert(value == 0xa000 || value == 0xa001 || value == 0xa003);
        f->selected = value == 0xa000 ? 2 : value == 0xa003;
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
    for (unsigned variant = 0; variant < 2; ++variant) {
        for (unsigned mode = 0; mode < 3; ++mode) {
            f = ready;
            if (variant) {
                f.registers[3] = 0xe91a;
                f.extended[0] &= (uint16_t)~7;
                f.extended[2] = 0xffff;
            }
            f.registers[0x11] = (uint16_t)((mode << 14) | 0x2000);
            assert(shoop_yt8531_start(&bus) == speeds[mode]);
            assert(f.extended[0] == (variant ? 0xfef8 : 0xfeff) && f.extended[1] == 0xf7fd);
            assert(f.extended[2] == (variant ? 0xfffd : 0));
            assert(f.registers[0] == 0x1200 && f.registers[4] == 0x0941 && f.registers[9] == 0x0200);
            assert(f.elapsed_ms == 40);
        }
    }
    for (unsigned mode = 1; mode <= 7; ++mode) {
        f = ready;
        f.registers[3] = 0xe91a;
        f.extended[0] = (uint16_t)(0xfff8 | mode);
        assert(shoop_yt8531_start(&bus) == 0 && f.writes == 1);
        assert(f.extended[0] == (0xfff8 | mode));
        assert(f.registers[0] == ready.registers[0]);
    }
    f = ready;
    f.registers[3] = 0xe91a;
    f.registers[0x11] = 0xa000;
    f.extended[0] &= (uint16_t)~7;
    f.extended[2] = 0xffff;
    const struct fixture copper = f;
    assert(shoop_yt8531_start(&bus) == 1000);
    const unsigned read_count = f.reads, write_count = f.writes;
    for (unsigned failure = 1; failure <= read_count; ++failure) {
        f = copper;
        f.fail_read = failure;
        assert(shoop_yt8531_start(&bus) == 0 && f.reads == failure);
    }
    for (unsigned failure = 1; failure <= write_count; ++failure) {
        f = copper;
        f.fail_write = failure;
        assert(shoop_yt8531_start(&bus) == 0 && f.writes == failure);
    }
    const uint16_t unsupported[] = {0xe918, 0xe919, 0xe91c, 0xffff};
    for (unsigned id = 0; id < sizeof unsupported / sizeof unsupported[0]; ++id) {
        f = ready;
        f.registers[3] = unsupported[id];
        assert(shoop_yt8531_start(&bus) == 0 && f.writes == 0);
    }
    f = ready;
    f.registers[2] ^= 1;
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
