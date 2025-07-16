#pragma once
#include <QQmlApplicationEngine>
#include <QObject>
#include <QPointer>
#include <QThread>
#include <QString>
#include <vector>
#include <rust/cxx.h>

// Note: not thread-safe
extern QPointer<QQmlApplicationEngine> g_registered_qml_engine;

inline QQmlApplicationEngine* qQmlApplicationengineMakeRaw(QObject *parent) {
    auto rval = new QQmlApplicationEngine(parent);
    g_registered_qml_engine = rval;
    return rval;
}

inline QQmlApplicationEngine* getRegisteredQmlEngine() {
    return g_registered_qml_engine.data();
}

inline ::rust::String getQmlEngineStackTrace(QQmlApplicationEngine &engine) {
    if(engine.thread() != QThread::currentThread()) {
        // Can only retrieve the stack if on the same thread.
        return ::rust::String("unknown (JS engine not on querying thread)\n");
    }

    engine.throwError(QString("Crash handler stack retrieval helper error"));
    auto err = engine.catchError();
    if (err.isUndefined()) {
        return ::rust::String("unknown (couldn't catch helper error)\n");
    }

    auto stack = err.property("stack");
    if (stack.isUndefined()) {
        return ::rust::String("unknown (stack is undefined)\n");
    }

    auto stack_str = stack.toString() + "\n";
    return ::rust::String(stack_str.toStdString());
}