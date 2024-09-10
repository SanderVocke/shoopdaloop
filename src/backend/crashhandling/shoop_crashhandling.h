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

SHOOP_EXPORT void shoop_init_crashhandling_with_cb(const char* dump_dir, CrashedCallback cb);
SHOOP_EXPORT void shoop_init_crashhandling(const char* dump_dir);

#ifdef __cplusplus
}
#endif