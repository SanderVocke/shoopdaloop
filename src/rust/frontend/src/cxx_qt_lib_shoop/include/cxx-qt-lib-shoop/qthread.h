#pragma once
#include <QThread>

inline void qthreadStart(QThread &thread) {
    thread.start();
}

inline void qthreadExit(QThread &thread) {
    thread.exit();
}

inline bool qthreadIsRunning(QThread &thread) {
    return thread.isRunning();
}