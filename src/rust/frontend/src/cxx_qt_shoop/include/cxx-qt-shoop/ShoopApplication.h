#pragma once
#include <QtWidgets/QApplication>

inline int &dummy_argc() {
    static int argc = 0;
    return argc;
}

class ShoopApplication : public QApplication {
    public:
    ShoopApplication(QObject *parent = nullptr) : QApplication(dummy_argc(), nullptr) { (void) parent; }
};