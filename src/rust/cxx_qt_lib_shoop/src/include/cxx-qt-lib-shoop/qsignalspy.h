#pragma once
#include <QtTest/QSignalSpy>
#include <QMetaMethod>
#include <rust/cxx.h>

inline QSignalSpy *qsignalspyCreate(QObject const* object, ::rust::String signal) {
    auto meta = object->metaObject();

    int signalIndex = meta->indexOfSignal(signal.c_str());

    if (signalIndex < 0) {
        throw std::runtime_error("Could not find signal");
    }

    QMetaMethod signalMethod = meta->method(signalIndex);
    return new QSignalSpy(object, signalMethod);
}

inline int qsignalspyCount(QSignalSpy const&spy) {
    return spy.count();
}