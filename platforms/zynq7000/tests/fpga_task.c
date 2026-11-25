#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include "../fpga_task.c"

static uint64_t now;
static unsigned created, deleted, notifications;
static int create_ok = 1, result;
static TaskFunction_t run;
static void *argument;
uint64_t shoop_millis(void) { return now; }
int xTaskCreate(TaskFunction_t task, const char *name, unsigned depth, void *owner,
                unsigned priority, TaskHandle_t *handle) {
    (void)name;
    assert(depth == 32768 && priority == 5);
    if (!create_ok) return 0;
    ++created;
    run = task;
    argument = owner;
    *handle = (void *)(uintptr_t)created;
    return pdPASS;
}
void vTaskDelete(TaskHandle_t task) { assert(task != NULL); ++deleted; }
void vTaskSuspend(TaskHandle_t task) { assert(task == NULL); }
void xTaskNotifyGive(TaskHandle_t task) { assert(task != NULL); ++notifications; }
static int entry(void *context) {
    assert(context == &result);
    assert(shoop_fpga_stop_requested());
    return result;
}
int main(void) {
    for (unsigned iteration = 0; iteration < 32; ++iteration) {
        now = 100 + iteration * 20000;
        assert(shoop_start_fpga_task(entry, &result));
        assert(!shoop_fpga_stop_requested());
        assert(atomic_load(&fpga_done) == 0 && fpga_deadline == 0);
        unsigned before = deleted;
        assert(shoop_stop_fpga_task() == 2);
        assert(deleted == before);
        assert(fpga_deadline == now + 5000);
        result = iteration % 2 ? 0 : 1;
        run(argument);
        assert(shoop_stop_fpga_task() == (result ? 1 : -1));
        assert(deleted == before + 1 && fpga_task == NULL);
    }
    create_ok = 0;
    assert(!shoop_start_fpga_task(entry, &result));
    assert(fpga_task == NULL);
    create_ok = 1;
    now += 20000;
    assert(shoop_start_fpga_task(entry, &result));
    unsigned before = deleted;
    assert(shoop_stop_fpga_task() == 2);
    now += 5001;
    assert(shoop_stop_fpga_task() == 0);
    assert(deleted == before && fpga_task != NULL);
    run(argument);
    assert(shoop_stop_fpga_task() == -1);
    assert(notifications == created);
    puts("FPGA cooperative task restart: 32 lifetimes, failure, and timeout passed");
}
