#pragma once
#include <QtGui/QGuiApplication>
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

class ShoopApplication : public QGuiApplication {
    public:
    ShoopApplication(QObject *parent = nullptr) : QGuiApplication(dummy_argc(), dummy_argv()) { (void) parent; }
};