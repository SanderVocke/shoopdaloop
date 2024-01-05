#pragma once

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*CrashedCallback)(const char *dumped_filename);

SHOOP_EXPORT void shoop_init_crashhandling(const char* dump_dir, CrashedCallback cb);
SHOOP_EXPORT void shoop_test_crash_segfault();
SHOOP_EXPORT void shoop_test_crash_exception();
SHOOP_EXPORT void shoop_test_crash_abort();

#ifdef __cplusplus
}
#endif