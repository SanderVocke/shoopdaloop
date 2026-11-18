#include "FreeRTOS.h"
#include "diagnostics.h"
#include "../native_audio.h"
#include "native_audio_registers.h"
#include <stdatomic.h>

extern void shoop_fpga_irq_pend_fixture(void);

static _Atomic uint32_t enabled, pending, sequence;
static uint32_t ticks, writes, played_nonzero;

void shoop_fixture_native_audio_tick(void) {
    if (!atomic_load(&enabled) || atomic_load(&pending) || ++ticks % 2) return;
    atomic_fetch_add(&sequence, 1);
    atomic_store(&pending, 1);
    shoop_fpga_irq_pend_fixture();
}

uint32_t shoop_fixture_native_audio_read(uint32_t offset) {
    switch (offset) {
        case SHOOP_AUDIO_REG_ID: return SHOOP_AUDIO_ID;
        case SHOOP_AUDIO_REG_VERSION: return SHOOP_AUDIO_VERSION;
        case SHOOP_AUDIO_REG_QUANTUM: return SHOOP_AUDIO_QUANTUM;
        case SHOOP_AUDIO_REG_SAMPLE_RATE: return SHOOP_AUDIO_SAMPLE_RATE;
        case SHOOP_AUDIO_REG_CLOCK_HZ: return SHOOP_PL_CLOCK_HZ;
        case SHOOP_AUDIO_REG_STATUS: return atomic_load(&pending) ? SHOOP_AUDIO_STATUS_PENDING : 0;
        case SHOOP_AUDIO_REG_SEQUENCE: return atomic_load(&sequence);
        default:
            if (offset >= SHOOP_AUDIO_REG_RX_BASE &&
                offset < SHOOP_AUDIO_REG_RX_BASE + 4 * SHOOP_AUDIO_QUANTUM) {
                configASSERT(atomic_load(&pending));
                return 0x00400000u;
            }
            return 0;
    }
}

void shoop_fixture_native_audio_write(uint32_t offset, uint32_t value) {
    if (offset == SHOOP_AUDIO_REG_ENABLE) {
        atomic_store(&enabled, value);
        if (!value) shoop_logf("SHOOP_NATIVE_I2S_FIXTURE output_nonzero=%u\r\n", played_nonzero);
    } else if (offset == SHOOP_AUDIO_REG_COMPLETE) {
        configASSERT(atomic_load(&pending) && value == atomic_load(&sequence));
        configASSERT(writes == 2 * SHOOP_AUDIO_QUANTUM);
        writes = 0;
        atomic_store(&pending, 0);
    } else {
        configASSERT(atomic_load(&pending) && writes < 2 * SHOOP_AUDIO_QUANTUM);
        configASSERT(offset == SHOOP_AUDIO_REG_TX_BASE + 4 * writes);
        ++writes;
        if (value) ++played_nonzero;
    }
}
