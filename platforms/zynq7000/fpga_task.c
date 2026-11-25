#include "FreeRTOS.h"
#include "task.h"
#include "deployment/runtime.h"
#include <stdatomic.h>
#include <stdint.h>

extern uint64_t shoop_millis(void);
static TaskHandle_t fpga_task;
static int (*fpga_entry)(void *);
static _Atomic int fpga_done, fpga_stop;
static uint64_t fpga_deadline;

static void fpga_owner(void *owner) {
    int result = fpga_entry(owner);
    shoop_deploy_render_stopped();
    atomic_store(&fpga_done, result == 1 ? 1 : -1);
    vTaskSuspend(NULL);
}
int shoop_start_fpga_task(int (*entry)(void *), void *owner) {
    configASSERT(!fpga_task);
    fpga_entry = entry;
    fpga_deadline = 0;
    atomic_store(&fpga_done, 0);
    atomic_store(&fpga_stop, 0);
    shoop_deploy_render_started();
    int started = xTaskCreate(fpga_owner, "fpga-owner", 32768, owner, 5, &fpga_task) == pdPASS;
    if (!started) shoop_deploy_render_stopped();
    return started;
}
int shoop_fpga_stop_requested(void) { return atomic_load(&fpga_stop); }
void shoop_fpga_task_progress(void) { shoop_deploy_render(); }
int shoop_stop_fpga_task(void) {
    configASSERT(fpga_task);
    if (!fpga_deadline) {
        fpga_deadline = shoop_millis() + 5000;
        atomic_store(&fpga_stop, 1);
        shoop_deploy_render_stopped();
        xTaskNotifyGive(fpga_task);
    }
    int done = atomic_load(&fpga_done);
    if (!done) return shoop_millis() < fpga_deadline ? 2 : 0;
    vTaskDelete(fpga_task);
    fpga_task = NULL;
    shoop_deploy_render_stopped();
    return done;
}
