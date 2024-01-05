#pragma once

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

SHOOP_EXPORT void shoop_init_crashhandling(const char* dump_dir);
SHOOP_EXPORT void shoop_test_crash_segfault();
SHOOP_EXPORT void shoop_test_crash_exception();
SHOOP_EXPORT void shoop_test_crash_abort();

#ifdef __cplusplus
}
#endif