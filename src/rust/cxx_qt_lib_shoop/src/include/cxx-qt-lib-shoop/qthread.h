#pragma once
#include <Qt>
#include <QThread>
#include "connect.h"

inline void qthreadStart(QThread &thread) {
    thread.start();
}

inline void qthreadExit(QThread &thread) {
    thread.exit();
}

inline bool qthreadIsRunning(QThread &thread) {
    return thread.isRunning();
}

template<typename A>
inline void qthreadConnectStarted(QThread &timer, A *receiver, ::rust::String member) {
    connect(&timer, "started()", receiver, member, Qt::DirectConnection);
}