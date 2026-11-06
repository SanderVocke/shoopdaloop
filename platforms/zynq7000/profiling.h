#ifndef SHOOP_TASK_PROFILING_H
#define SHOOP_TASK_PROFILING_H
#include <stdint.h>
#define SHOOP_PROFILE_TASKS 32
struct shoop_profile_task {
    uint32_t state, priority, base_priority, reserved;
    uint64_t id;
    uint64_t runtime, stack_free_bytes;
    uint8_t name[16];
};
struct shoop_profile_snapshot {
    uint64_t captured_us, total_runtime, collection_us, counter_hz;
    uint32_t count, complete, counter_bits, reserved;
    struct shoop_profile_task tasks[SHOOP_PROFILE_TASKS];
};
_Static_assert(sizeof(struct shoop_profile_task) == 56, "task profile ABI");
_Static_assert(sizeof(struct shoop_profile_snapshot) == 1840, "task snapshot ABI");
int shoop_profile_tasks(struct shoop_profile_snapshot *output);
#endif
