#include "FreeRTOS.h"
#include "task.h"
#include "lwip/init.h"
#include "lwip/sockets.h"
#include "lwip/stats.h"
#include "lwip/memp.h"
#include "netif/xadapter.h"
#include "xil_printf.h"
#include "xtime_l.h"
#include <stdatomic.h>
#include <errno.h>
#include <string.h>
#include "deployment/runtime.h"
#include "profiling.h"
#include "diagnostics.h"
#ifdef SHOOP_USB_MIDI
#include "usb/host.h"
#endif

extern void shoop_platform_init(void);
extern void shoop_rust_main(void);
extern int shoop_render_step(void *, size_t);
extern void shoop_render_xruns(void *, uint32_t);
#ifndef SHOOP_DEPLOYMENT
extern const unsigned char shoop_credential[32];
#endif
extern const char shoop_build_identity[];
static struct netif interface;
static TaskHandle_t render_task;
static _Atomic int render_done;
static uint32_t phase;
static uint32_t render_stack_remaining;
/* Fixed MIDI state copies and nested preparation/render calls need this budget.
 * FreeRTOS stack depths count StackType_t words, not bytes. */
#define SHOOP_ENGINE_STACK_WORDS (512 * 1024 / sizeof(StackType_t))
#ifdef SHOOP_FIXTURE
extern int shoop_fixture_paused(void);
extern unsigned shoop_fixture_frames(void);
extern void shoop_fixture_irq(void);
extern void shoop_fixture_start(void);
extern int shoop_fixture_step(void *, size_t, uint32_t, uint32_t);
extern uint32_t shoop_fixture_input(void);
extern uint32_t shoop_fixture_midi(void);
#endif

void shoop_halt(void) {
    portDISABLE_INTERRUPTS();
    for (;;) { __asm__ volatile("wfi"); }
}
void shoop_allocator_lock(void) { vTaskSuspendAll(); }
void shoop_allocator_unlock(void) { xTaskResumeAll(); }
int shoop_in_render(void) { return render_task && xTaskGetCurrentTaskHandle() == render_task; }
void shoop_delay(uint32_t millis) { vTaskDelay(pdMS_TO_TICKS(millis)); }
uint64_t shoop_millis(void) {
    XTime now; XTime_GetTime(&now);
    return now / (XPAR_CPU_CORTEXA9_0_CPU_CLK_FREQ_HZ / 2000);
}
uint64_t shoop_timer_ticks(void) { XTime now; XTime_GetTime(&now); return now; }
uint64_t shoop_timer_hz(void) { return XPAR_CPU_CORTEXA9_0_CPU_CLK_FREQ_HZ / 2; }
void shoop_token(unsigned char *output) {
#ifdef SHOOP_DEPLOYMENT
    const unsigned char *token = shoop_deploy_config()->credential;
#else
    const unsigned char *token = shoop_credential;
#endif
    for (size_t i = 0; i < 32; ++i) output[i] = token[i];
}
void shoop_control_progress(void) { shoop_deploy_control(); }
int shoop_deployment_ready(void) {
#ifdef SHOOP_DEPLOYMENT
    return shoop_deploy_confirm();
#else
    return 1;
#endif
}
int shoop_deployment_info(struct shoop_deployment_info *output) {
    if (!output) return 0;
#ifdef SHOOP_DEPLOYMENT
    return shoop_deploy_info(output);
#else
    (void)output; return 0;
#endif
}
uint64_t shoop_nonce(void) { static uint64_t next; configASSERT(next != UINT64_MAX); return ++next; }
int shoop_deployment_request(const uint8_t *client, uint64_t sequence) {
#ifdef SHOOP_DEPLOYMENT
    return shoop_deploy_request(client, sequence);
#else
    (void)client; (void)sequence; return 0;
#endif
}
void shoop_deployment_reset(void) {
#ifdef SHOOP_DEPLOYMENT
    shoop_deploy_reset();
#endif
    shoop_halt();
}
uint64_t shoop_boot_nonce(void) {
    const volatile uint32_t *words = (const volatile uint32_t *)0x8000;
    return (uint64_t)words[0] | (uint64_t)words[1] << 32;
}

void vApplicationTickHook(void) {
    shoop_deploy_tick();
#ifdef SHOOP_FIXTURE
    shoop_fixture_irq();
    if (shoop_fixture_paused()) return;
#endif
    if (!render_task || atomic_load(&render_done)) return;
    phase += 48000;
    if (phase >= 256000) {
        phase -= 256000;
        BaseType_t wake = pdFALSE;
        vTaskNotifyGiveFromISR(render_task, &wake);
        portYIELD_FROM_ISR(wake);
    }
}
#ifndef SHOOP_FIXTURE
/* Render outranks the control task, whose heartbeats keep app sessions alive.
   Under overload, cap render's share of each window and drop the excess quanta
   as xruns instead of starving control. */
#define RENDER_WINDOW_MS 100
#define RENDER_MAX_PERCENT 80
#endif
static void render(void *owner) {
    /* RunFast for this task's FPSCR: flush denormals to zero and use default NaNs,
       as NEON always does. Decaying ramps and filters otherwise produce denormals. */
    uint32_t fpscr;
    __asm__ volatile("vmrs %0, fpscr" : "=r"(fpscr));
    __asm__ volatile("vmsr fpscr, %0" :: "r"(fpscr | (1u << 24) | (1u << 25)));
#ifndef SHOOP_FIXTURE
    const uint64_t window = shoop_timer_hz() * RENDER_WINDOW_MS / 1000;
    uint64_t window_start = shoop_timer_ticks(), busy = 0;
#endif
    for (;;) {
        uint32_t count = ulTaskNotifyTake(pdTRUE, portMAX_DELAY);
        if (count > 8) { shoop_render_xruns(owner, count - 8); count = 8; }
#ifndef SHOOP_FIXTURE
        uint64_t started = shoop_timer_ticks();
        if (started - window_start >= window) { window_start = started; busy = 0; }
        if (busy * 100 >= window * RENDER_MAX_PERCENT) {
            shoop_render_xruns(owner, count);
            shoop_deploy_render();
            continue;
        }
#endif
        for (uint32_t i = 0; i < count; ++i) {
#ifdef SHOOP_FIXTURE
            unsigned frames = shoop_fixture_frames();
            if (!frames) continue;
            int result = shoop_fixture_step(owner, frames, shoop_fixture_input(), shoop_fixture_midi());
#else
            int result = shoop_render_step(owner, 256);
#endif
            shoop_deploy_render();
            if (result != 0) {
                atomic_store(&render_done, result == 1 ? 1 : -1);
                vTaskSuspend(NULL);
            }
        }
#ifndef SHOOP_FIXTURE
        busy += shoop_timer_ticks() - started;
#endif
    }
}
void shoop_start_render(void *owner) {
    configASSERT(!render_task);
    configASSERT(xTaskCreate(render, "render", SHOOP_ENGINE_STACK_WORDS, owner, 6, &render_task) == pdPASS);
#ifdef SHOOP_FIXTURE
    shoop_fixture_start();
#endif
}
void shoop_render_wake(void) { if (render_task) xTaskNotifyGive(render_task); }
int shoop_stop_render(void) {
#ifdef SHOOP_FIXTURE
    extern void shoop_fixture_stopping(void);
    shoop_fixture_stopping();
    shoop_render_wake();
#endif
    static uint64_t deadline;
    if (!deadline) deadline = shoop_millis() + 5000;
    if (!atomic_load(&render_done)) return shoop_millis() < deadline ? 2 : 0;
    if (atomic_load(&render_done) != 1) return 0;
    render_stack_remaining = uxTaskGetStackHighWaterMark(render_task);
    vTaskDelete(render_task); render_task = NULL;
    shoop_deploy_render_stopped();
    return 1;
}
int shoop_cleanup_allowed(void) {
#ifdef SHOOP_USB_MIDI
    int usb = shoop_usb_stop();
    if (usb != 1) return usb;
#endif
#if defined(SHOOP_DEPLOYMENT) && SHOOP_DEPLOY_FAULT == 6
    static unsigned failed;
    if (!failed++) { shoop_logf("SHOOP_DEPLOY_TEST_CLEANUP_REFUSED\r\n"); return 0; }
#endif
#ifdef SHOOP_FIXTURE
    extern int shoop_fixture_fail_cleanup(void);
    return !shoop_fixture_fail_cleanup();
#else
    return 1;
#endif
}
void shoop_memory_report(void) {
    extern void shoop_heap_stats(size_t *, size_t *);
    extern size_t shoop_c_heap_committed(void);
    extern unsigned shoop_socket_count(void);
    size_t used, peak;
    shoop_heap_stats(&used, &peak);
    shoop_logf("SHOOP_MEMORY used=%u peak=%u c_free=%u c_min_free=%u control_stack_free=%u render_stack_free=%u uptime_ms=%u\r\n",
        (unsigned)used, (unsigned)peak, (unsigned)xPortGetFreeHeapSize(), (unsigned)xPortGetMinimumEverFreeHeapSize(),
        (unsigned)uxTaskGetStackHighWaterMark(NULL), (unsigned)(render_task ? uxTaskGetStackHighWaterMark(render_task) : render_stack_remaining), (unsigned)shoop_millis());
    shoop_logf("SHOOP_C_MEMORY newlib_committed=%u lwip_used=%u lwip_peak=%u tcp_pcbs=%u netconns=%u sockets=%u\r\n",
        (unsigned)shoop_c_heap_committed(), (unsigned)lwip_stats.mem.used, (unsigned)lwip_stats.mem.max,
        (unsigned)lwip_stats.memp[MEMP_TCP_PCB]->used, (unsigned)lwip_stats.memp[MEMP_NETCONN]->used,
        shoop_socket_count());
}
static void stacks_report(void) {
    static TaskStatus_t tasks[32];
    unsigned count = uxTaskGetSystemState(tasks, 32, NULL);
    configASSERT(count > 0);
    for (unsigned i = 0; i < count; ++i) {
        shoop_logf("SHOOP_STACK name=%s free_words=%u\r\n", tasks[i].pcTaskName, (unsigned)tasks[i].usStackHighWaterMark);
    }
}

static void network(void *unused) {
    (void)unused;
    ip_addr_t ip, mask, gateway;
#ifdef SHOOP_DEPLOYMENT
    configASSERT(shoop_deploy_init());
    const struct shoop_config *config = shoop_deploy_config();
    memcpy(&ip, &config->ip, 4); memcpy(&mask, &config->netmask, 4); memcpy(&gateway, &config->gateway, 4);
    unsigned char mac[6]; memcpy(mac, config->mac, 6);
#else
    IP4_ADDR(&ip, 10, 0, 2, 15); IP4_ADDR(&mask, 255, 255, 255, 0); IP4_ADDR(&gateway, 10, 0, 2, 2);
    unsigned char mac[] = { 0x02, 0, 0, 0, 0, 1 };
#endif
    lwip_init();
    configASSERT(xemac_add(&interface, &ip, &mask, &gateway, mac, SHOOP_GEM_BASE));
    netif_set_default(&interface); netif_set_up(&interface);
    sys_thread_new("gem-input", (void (*)(void *))xemacif_input_thread, &interface, 4096, 4);
    shoop_rust_main();
    shoop_deploy_stopped();
    stacks_report();
    extern void shoop_network_stop(void);
    shoop_network_stop();
    shoop_memory_report();
    shoop_logf("SHOOP_HOST_RETURNED\r\n");
    vTaskDelete(NULL);
}
int main(void) {
    shoop_platform_init();
    shoop_logf("SHOOP_BOOT FreeRTOS AMD-lwIP board=%s ram_mib=%u\r\n",
               SHOOP_BOARD_PROFILE, (unsigned)(SHOOP_RAM_BYTES / (1024 * 1024)));
    shoop_logf("SHOOP_BUILD %s\r\n", shoop_build_identity);
#ifdef SHOOP_USB_MIDI
    shoop_usb_start();
#endif
    configASSERT(xTaskCreate(network, "control", SHOOP_ENGINE_STACK_WORDS, NULL, 3, NULL) == pdPASS);
    vTaskStartScheduler();
    vApplicationAssert("scheduler returned", 0);
    return 1;
}

int shoop_profile_tasks(struct shoop_profile_snapshot *output) {
    if (!output) return 0;
    static TaskStatus_t tasks[SHOOP_PROFILE_TASKS];
    uint64_t start = shoop_profile_counter();
    memset(output, 0, sizeof(*output));
    output->counter_hz = XPAR_CPU_CORTEXA9_0_CPU_CLK_FREQ_HZ / 2;
    output->counter_bits = 64;
    vTaskSuspendAll();
    output->count = uxTaskGetSystemState(tasks, SHOOP_PROFILE_TASKS, &output->total_runtime);
    output->complete = output->count > 0;
    for (unsigned i = 0; i < output->count; ++i) {
        const TaskStatus_t *task = &tasks[i];
        struct shoop_profile_task *out = &output->tasks[i];
        out->id = shoop_profile_task_identity((void *)task->xHandle);
        if (!out->id) output->complete = 0;
        out->state = task->eCurrentState;
        out->priority = task->uxCurrentPriority;
        out->base_priority = task->uxBasePriority;
        out->runtime = task->ulRunTimeCounter;
        out->stack_free_bytes = (uint64_t)task->usStackHighWaterMark * sizeof(StackType_t);
        for (unsigned j = 0; j < sizeof(out->name) - 1 && task->pcTaskName[j]; ++j)
            out->name[j] = (uint8_t)task->pcTaskName[j];
    }
    xTaskResumeAll();
    uint64_t end = shoop_profile_counter();
    output->captured_us = (start / output->counter_hz) * 1000000 + (start % output->counter_hz) * 1000000 / output->counter_hz;
    output->collection_us = ((end - start) / output->counter_hz) * 1000000 + ((end - start) % output->counter_hz) * 1000000 / output->counter_hz;
    return 1;
}
