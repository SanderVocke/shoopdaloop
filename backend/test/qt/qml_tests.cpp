#include <QtQuickTest>
#include "QtProxyAudioSystem.h"

_QtProxyAudioSystem* g_test_audio_system;

extern "C" {

int run_quick_test_main_with_setup(int argc, char** argv, const char *name, const char *sourceDir, void *setup_qobject) {
    return quick_test_main_with_setup(argc, argv, name, sourceDir, (QObject*)setup_qobject);
}

void *get_proxy_audio_system() {
    return (void*)g_test_audio_system;
}

}