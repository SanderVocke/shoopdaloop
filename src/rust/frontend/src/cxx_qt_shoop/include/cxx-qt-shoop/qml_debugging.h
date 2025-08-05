#pragma once

#include <qqmldebug.h>

inline void enableQmlDebugging(bool wait, int port) {
    QQmlDebuggingEnabler::StartMode mode =
        wait ? QQmlDebuggingEnabler::StartMode::WaitForClient : 
        QQmlDebuggingEnabler::StartMode::DoNotWaitForClient;
    
        QQmlDebuggingEnabler::enableDebugging(false);
        QQmlDebuggingEnabler::startTcpDebugServer(port, mode);
}