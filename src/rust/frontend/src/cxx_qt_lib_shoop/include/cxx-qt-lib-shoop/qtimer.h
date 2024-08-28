#pragma once
#include <QTimer>
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

template<typename A>
inline void qtimerConnectTimeout(QTimer &timer, A *receiver, ::rust::String member) {
    connect(&timer, "timeout()", receiver, member);
}