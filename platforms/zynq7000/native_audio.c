#include "FreeRTOS.h"
#include "task.h"
#include "xil_io.h"
#include "diagnostics.h"
#include "native_audio.h"
#include "native_audio_registers.h"
#include <stdbool.h>
#include <stdint.h>

extern int shoop_fpga_configured(void);
extern int shoop_fpga_irq_start(void);
extern int shoop_fpga_irq_wait(void);
extern void shoop_fpga_irq_stop(void);
extern uint64_t shoop_timer_ticks(void);
extern uint64_t shoop_timer_hz(void);
extern int shoop_native_pcm_step(void *owner, const int32_t *input, int32_t *output);
extern void shoop_render_xruns(void *owner, uint32_t count);

static bool running;
static uint32_t xruns, peak_callback_ticks, irq_wakeups;
static uint64_t last_input_tick;

static uint32_t read_audio(uint32_t offset) {
#ifdef SHOOP_FIXTURE
    return shoop_fixture_native_audio_read(offset);
#else
    return Xil_In32(SHOOP_PL_REG_BASE + offset);
#endif
}

static void write_audio(uint32_t offset, uint32_t value) {
#ifdef SHOOP_FIXTURE
    shoop_fixture_native_audio_write(offset, value);
#else
    Xil_Out32(SHOOP_PL_REG_BASE + offset, value);
#endif
}

static void barrier(void) {
    __asm__ volatile("dmb sy" ::: "memory");
}

int shoop_native_audio_available(void) {
#ifndef SHOOP_FIXTURE
    if (!shoop_fpga_configured()) return 0;
#endif
    return read_audio(SHOOP_AUDIO_REG_ID) == SHOOP_AUDIO_ID &&
        read_audio(SHOOP_AUDIO_REG_VERSION) == SHOOP_AUDIO_VERSION &&
        read_audio(SHOOP_AUDIO_REG_QUANTUM) == SHOOP_AUDIO_QUANTUM &&
        read_audio(SHOOP_AUDIO_REG_SAMPLE_RATE) == SHOOP_AUDIO_SAMPLE_RATE &&
        read_audio(SHOOP_AUDIO_REG_CLOCK_HZ) == SHOOP_PL_CLOCK_HZ;
}

int shoop_native_audio_start(void) {
    if (!shoop_fpga_irq_start()) return 0;
    running = true;
    last_input_tick = shoop_timer_ticks();
    write_audio(SHOOP_AUDIO_REG_ENABLE, 1);
    barrier();
    return 1;
}

static void observe_misses(void *owner, bool fault) {
    uint32_t count = read_audio(SHOOP_AUDIO_REG_XRUNS);
    if (fault && count == 0) count = 1;
    if (count > xruns) {
        shoop_render_xruns(owner, count - xruns);
        xruns = count;
    }
}

static void stop_audio(const char *reason) {
    if (!running) return;
    write_audio(SHOOP_AUDIO_REG_ENABLE, 0);
    barrier();
    shoop_fpga_irq_stop();
    running = false;
    shoop_logf("SHOOP_NATIVE_I2S_RESULT reason=%s xruns=%u peak_callback_ticks=%u callback_hz=%u peak_bank_cycles=%u fabric_hz=%u irq_wakeups=%u\r\n",
        reason, xruns, peak_callback_ticks, (unsigned)shoop_timer_hz(),
        read_audio(SHOOP_AUDIO_REG_PEAK_CYCLES), SHOOP_PL_CLOCK_HZ, irq_wakeups);
}

int shoop_native_audio_wait(void *owner, int stopping) {
    int32_t output[2 * SHOOP_AUDIO_QUANTUM];
    if (stopping) {
        const int32_t silence[SHOOP_AUDIO_QUANTUM] = {0};
        if (running) observe_misses(owner, false);
        stop_audio("shutdown");
        int result = shoop_native_pcm_step(owner, silence, output);
        if (!result) vTaskDelay(1);
        return result;
    }
    if (!running) {
        vTaskDelay(1);
        return 0;
    }

    if (shoop_fpga_irq_wait() && irq_wakeups != UINT32_MAX) ++irq_wakeups;
    barrier();
    uint32_t status = read_audio(SHOOP_AUDIO_REG_STATUS);
    const bool fault = (status & SHOOP_AUDIO_STATUS_FAULT) != 0;
    observe_misses(owner, fault);
    if (fault) {
        stop_audio("bank-fault");
        return 0;
    }
    if (!(status & SHOOP_AUDIO_STATUS_PENDING)) {
        if (shoop_timer_ticks() - last_input_tick > shoop_timer_hz() / 20) {
            if (!xruns) {
                shoop_render_xruns(owner, 1);
                xruns = 1;
            }
            stop_audio("no-input-bank");
        }
        return 0;
    }

    last_input_tick = shoop_timer_ticks();
    uint32_t sequence = read_audio(SHOOP_AUDIO_REG_SEQUENCE);
    int32_t input[SHOOP_AUDIO_QUANTUM];
    for (unsigned frame = 0; frame < SHOOP_AUDIO_QUANTUM; ++frame)
        input[frame] = (int32_t)read_audio(SHOOP_AUDIO_REG_RX_BASE + 4 * frame);
    barrier();
    uint64_t callback_start = shoop_timer_ticks();
    int result = shoop_native_pcm_step(owner, input, output);
    uint64_t elapsed = shoop_timer_ticks() - callback_start;
    if (elapsed > peak_callback_ticks)
        peak_callback_ticks = elapsed > UINT32_MAX ? UINT32_MAX : (uint32_t)elapsed;
    if (!result) {
        for (unsigned word = 0; word < 2 * SHOOP_AUDIO_QUANTUM; ++word)
            write_audio(SHOOP_AUDIO_REG_TX_BASE + 4 * word, (uint32_t)output[word]);
        barrier();
        write_audio(SHOOP_AUDIO_REG_COMPLETE, sequence);
        barrier();
    } else {
        stop_audio(result > 0 ? "shutdown" : "callback-error");
    }
    return result;
}
