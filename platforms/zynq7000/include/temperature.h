#ifndef SHOOP_TEMPERATURE_H
#define SHOOP_TEMPERATURE_H
#include <stdint.h>

#define SHOOP_TEMPERATURE_CURRENT 1u
#define SHOOP_TEMPERATURE_PEAK 2u
#define SHOOP_TEMPERATURE_ALARM_VALID 4u
#define SHOOP_TEMPERATURE_ALARM 8u

struct shoop_temperature {
    int32_t current_millicelsius;
    int32_t peak_millicelsius;
    uint32_t flags;
};

struct shoop_xadc_bus {
    void *context;
    uint32_t (*read)(void *, unsigned offset);
    void (*write)(void *, unsigned offset, uint32_t value);
};

void shoop_xadc_sample(const struct shoop_xadc_bus *, struct shoop_temperature *);
int shoop_soc_temperature(struct shoop_temperature *);
#endif
