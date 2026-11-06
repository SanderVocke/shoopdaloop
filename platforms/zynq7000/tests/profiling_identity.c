#include <assert.h>
#include "profiling_identity.h"
int main(void) {
    char handles[33];
    shoop_profile_task_created(&handles[0]);
    uint64_t before = shoop_profile_task_identity(&handles[0]);
    assert(before && shoop_profile_task_identity(&handles[0]) == before);
    shoop_profile_task_deleted(&handles[0]);
    shoop_profile_task_created(&handles[0]);
    assert(shoop_profile_task_identity(&handles[0]) > before);
    for (unsigned i = 1; i < 33; ++i) shoop_profile_task_created(&handles[i]);
    assert(shoop_profile_task_identity(&handles[32]) == 0);
    shoop_profile_task_deleted(&handles[1]);
    assert(shoop_profile_task_identity(&handles[32]) > before);
    for (unsigned i = 0; i < 100; ++i) {
        uint64_t last = shoop_profile_task_identity(&handles[0]);
        shoop_profile_task_deleted(&handles[0]);
        shoop_profile_task_created(&handles[0]);
        assert(shoop_profile_task_identity(&handles[0]) > last);
    }
    return 0;
}
