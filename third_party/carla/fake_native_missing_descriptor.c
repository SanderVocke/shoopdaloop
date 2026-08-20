/* Negative-test library: exports the pinned entry points but no descriptors. */
#include <stddef.h>

#if defined(_WIN32)
# define EXPORT __declspec(dllexport)
#else
# define EXPORT __attribute__((visibility("default")))
#endif

EXPORT const void* carla_get_native_rack_plugin(void) { return NULL; }
EXPORT const void* carla_get_native_patchbay_plugin(void) { return NULL; }
#if !defined(OMIT_PATCHBAY16)
EXPORT const void* carla_get_native_patchbay16_plugin(void) { return NULL; }
#endif
