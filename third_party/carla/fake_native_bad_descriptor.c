/* Negative-test library: exports every getter with an incompatible descriptor. */
#include "CarlaNative.h"

#if defined(_WIN32)
# define EXPORT __declspec(dllexport)
#else
# define EXPORT __attribute__((visibility("default")))
#endif

static const NativePluginDescriptor bad_descriptor = {
    .hints = NATIVE_PLUGIN_HAS_UI | NATIVE_PLUGIN_USES_STATE,
    .audioIns = 99,
    .audioOuts = 99,
    .midiIns = 1,
    .midiOuts = 1,
    .label = "incompatible"
};

EXPORT const NativePluginDescriptor* carla_get_native_rack_plugin(void) { return &bad_descriptor; }
EXPORT const NativePluginDescriptor* carla_get_native_patchbay_plugin(void) { return &bad_descriptor; }
#if !defined(OMIT_PATCHBAY16)
EXPORT const NativePluginDescriptor* carla_get_native_patchbay16_plugin(void) { return &bad_descriptor; }
#endif
