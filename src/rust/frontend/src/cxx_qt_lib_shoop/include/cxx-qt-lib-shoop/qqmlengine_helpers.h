#pragma once
#include <QQmlApplicationEngine>
#include <QObject>
#include <QPointer>
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

inline ::rust::String getQmlEngineStackTrace(QQmlApplicationEngine const& engine) {
    return ::rust::String("test");
}