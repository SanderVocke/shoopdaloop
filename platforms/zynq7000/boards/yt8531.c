#include "yt8531.h"

enum {
    CONTROL = 0, STATUS = 1, ID_HIGH = 2, ID_LOW = 3,
    ADVERTISE = 4, ADVERTISE_GIGABIT = 9, PHY_STATUS = 0x11,
    EXT_ADDRESS = 0x1e, EXT_DATA = 0x1f,
    CHIP_CONFIG = 0xa001, RGMII_CONFIG = 0xa003,
    CONTROL_AN_ENABLE = 1 << 12, CONTROL_AN_RESTART = 1 << 9,
    CONTROL_POWER_DOWN = 1 << 11, CONTROL_ISOLATE = 1 << 10,
    STATUS_AN_COMPLETE = 1 << 5, STATUS_LINK = 1 << 2,
    ADV_10_FULL = 1 << 6, ADV_100_FULL = 1 << 8,
    ADV_HALF_OR_100T4 = (1 << 5) | (1 << 7) | (1 << 9),
    ADV_1000_HALF = 1 << 8, ADV_1000_FULL = 1 << 9,
    PHY_FULL_DUPLEX = 1 << 13, PHY_SPEED = (3 << 14) | (1 << 9),
    RGMII_RX_DELAY_BASE = 1 << 8,
    RGMII_RX_DELAY = 15 << 10, RGMII_TX_GE_DELAY = 15,
    DELAY_1950_PS = 13,
    POLL_MS = 20, LINK_TIMEOUT_MS = 5000,
};

static bool update(const struct shoop_mdio *bus, unsigned reg, uint16_t mask, uint16_t value)
{
    uint16_t current;
    return bus->read(bus->context, reg, &current) &&
           bus->write(bus->context, reg, (uint16_t)((current & ~mask) | value));
}

static bool update_extended(const struct shoop_mdio *bus, unsigned reg, uint16_t mask, uint16_t value)
{
    return bus->write(bus->context, EXT_ADDRESS, (uint16_t)reg) &&
           update(bus, EXT_DATA, mask, value);
}

unsigned shoop_yt8531_start(const struct shoop_mdio *bus)
{
    uint16_t high, low;
    if (!bus->read(bus->context, ID_HIGH, &high) || high != SHOOP_YT8531_ID_HIGH ||
        !bus->read(bus->context, ID_LOW, &low) || low != 0xe91bu)
        return 0;

    /* RGMII-ID uses 1.95 ns on RX and gigabit TX, in 150 ps steps. */
    if (!update_extended(bus, CHIP_CONFIG, RGMII_RX_DELAY_BASE, 0) ||
        !update_extended(bus, RGMII_CONFIG, RGMII_RX_DELAY | RGMII_TX_GE_DELAY,
                         (DELAY_1950_PS << 10) | DELAY_1950_PS) ||
        !update(bus, ADVERTISE, ADV_HALF_OR_100T4, ADV_10_FULL | ADV_100_FULL) ||
        !update(bus, ADVERTISE_GIGABIT, ADV_1000_HALF, ADV_1000_FULL) ||
        !update(bus, CONTROL, CONTROL_POWER_DOWN | CONTROL_ISOLATE,
                CONTROL_AN_ENABLE | CONTROL_AN_RESTART))
        return 0;

    for (unsigned elapsed = 0; elapsed <= LINK_TIMEOUT_MS; elapsed += POLL_MS) {
        uint16_t status;
        /* Link status latches low on the first read. */
        if (!bus->read(bus->context, STATUS, &status) || !bus->read(bus->context, STATUS, &status))
            return 0;
        if ((status & (STATUS_AN_COMPLETE | STATUS_LINK)) == (STATUS_AN_COMPLETE | STATUS_LINK)) {
            if (!bus->read(bus->context, PHY_STATUS, &status) || !(status & PHY_FULL_DUPLEX))
                return 0;
            switch (status & PHY_SPEED) {
                case 0: return 10;
                case 1 << 14: return 100;
                case 2 << 14: return 1000;
                default: return 0;
            }
        }
        if (elapsed < LINK_TIMEOUT_MS)
            bus->delay_ms(bus->context, POLL_MS);
    }
    return 0;
}
