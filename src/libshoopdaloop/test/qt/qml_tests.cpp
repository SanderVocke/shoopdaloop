#include <QtQuickTest>

extern "C" {

#include "qml_tests.h"

int run_quick_test_main_with_setup(int argc, char** argv, const char *name, const char *sourceDir, void *setup_qobject) {
    return quick_test_main_with_setup(argc, argv, name, sourceDir, (QObject*)setup_qobject);
}

}