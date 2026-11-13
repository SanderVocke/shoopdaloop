#ifndef SHOOP_PROFILING_IDENTITY_H
#define SHOOP_PROFILING_IDENTITY_H
#include <stdint.h>
void shoop_profile_task_created(void *handle);
void shoop_profile_task_deleted(void *handle);
uint64_t shoop_profile_task_identity(void *handle);
#endif
