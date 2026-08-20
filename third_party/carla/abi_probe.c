#include <stddef.h>
#include <stdio.h>
#include "CarlaNative.h"

#define OFFSET(type, field) printf(#type "." #field "=%zu\n", offsetof(type, field))

int main(void) {
    printf("NativeMidiEvent=%zu,%zu\n", sizeof(NativeMidiEvent), _Alignof(NativeMidiEvent));
    printf("NativeTimeInfo=%zu,%zu\n", sizeof(NativeTimeInfo), _Alignof(NativeTimeInfo));
    printf("NativeHostDescriptor=%zu,%zu\n", sizeof(NativeHostDescriptor), _Alignof(NativeHostDescriptor));
    printf("NativePluginDescriptor=%zu,%zu\n", sizeof(NativePluginDescriptor), _Alignof(NativePluginDescriptor));
    OFFSET(NativeHostDescriptor, dispatcher);
    OFFSET(NativePluginDescriptor, instantiate);
    OFFSET(NativePluginDescriptor, process);
    OFFSET(NativePluginDescriptor, get_state);
    OFFSET(NativePluginDescriptor, ui_width);
    return 0;
}
