#ifndef SHOOP_YT8531_H
#define SHOOP_YT8531_H

#include <stdbool.h>
#include <stdint.h>

#define SHOOP_YT8531_ID_HIGH 0x4f51u

struct shoop_mdio {
    void *context;
    bool (*read)(void *context, unsigned reg, uint16_t *value);
    bool (*write)(void *context, unsigned reg, uint16_t value);
    void (*delay_ms)(void *context, unsigned milliseconds);
};

/* Returns the full-duplex link speed in Mbps, or zero on failure. */
unsigned shoop_yt8531_start(const struct shoop_mdio *bus);

#endif
