#pragma once
#include <QtWidgets/QApplication>
#include <QtQuick/QQuickWindow>
#include <QtGui/QIcon>

inline int &dummy_argc() {
    static int argc = 1;
    return argc;
}

inline char **dummy_argv() {
    static const char* app_name = "ShoopDaLoop";
    return (char**) &app_name;
}

class ShoopApplication : public QApplication {
    public:
    ShoopApplication(QObject *parent = nullptr) : QApplication(dummy_argc(), dummy_argv()) { (void) parent; }
};