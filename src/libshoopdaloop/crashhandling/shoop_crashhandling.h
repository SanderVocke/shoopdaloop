#pragma once

#ifdef __cplusplus
extern "C" {
#endif

void shoop_init_crashhandling(const char* dump_dir);
void shoop_test_crash_segfault();
void shoop_test_crash_exception();
void shoop_test_crash_abort();

#ifdef __cplusplus
}
#endif