#include <stdint.h>
#include <stddef.h>
#include "profiling_identity.h"

struct identity { void *handle; uint64_t id; unsigned live; };
static struct identity identities[32];
static uint64_t next_identity;

void shoop_profile_task_created(void *handle) {
    struct identity *free = NULL;
    for (unsigned i = 0; i < 32; ++i) {
        if (identities[i].handle == handle) { free = &identities[i]; break; }
        if (!identities[i].live && !free) free = &identities[i];
    }
    if (!free || next_identity == UINT64_MAX) return;
    free->handle = handle;
    free->id = ++next_identity;
    free->live = 1;
}
void shoop_profile_task_deleted(void *handle) {
    for (unsigned i = 0; i < 32; ++i)
        if (identities[i].handle == handle) { identities[i].live = 0; return; }
}
uint64_t shoop_profile_task_identity(void *handle) {
    for (unsigned i = 0; i < 32; ++i)
        if (identities[i].handle == handle) return identities[i].id;
    shoop_profile_task_created(handle);
    for (unsigned i = 0; i < 32; ++i)
        if (identities[i].handle == handle) return identities[i].id;
    return 0;
}
