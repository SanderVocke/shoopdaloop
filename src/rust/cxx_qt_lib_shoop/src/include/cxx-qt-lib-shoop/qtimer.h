#pragma once
#include <QTimer>
#include <Qt>
#include <QMetaObject>
#include "connect.h"

inline void qtimerSetSingleShot(QTimer &timer, bool singleShot) {
    timer.setSingleShot(singleShot);
}

inline void qtimerStart(QTimer &timer) {
    timer.start();
}

inline void qtimerSetInterval(QTimer &timer, int interval) {
    timer.setInterval(interval);
}

inline void qtimerStop(QTimer &timer) {
    timer.stop();
}

inline void qtimerStopQueued(QTimer &timer) {
    QMetaMethod method = timer.metaObject()->method(timer.metaObject()->indexOfMethod("stop()"));
    method.invoke(&timer, Qt::QueuedConnection);
}

inline bool qtimerIsActive(QTimer const& timer) {
    return timer.isActive();
}

template<typename A>
inline void qtimerConnectTimeout(QTimer &timer, A *receiver, ::rust::String member) {
    connect(&timer, "timeout()", receiver, member, Qt::DirectConnection);
}