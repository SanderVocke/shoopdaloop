#pragma once

#include <QMetaObject>
#include <QString>
#include <thread>
#include <string>
#include <atomic>

// Invoke an invokable from a new thread and return true if this was done
// successfully.
template<typename QObjectType>
inline bool invoke_from_new_thread(QObjectType &object, QString method) {
    std::atomic<bool> exception = false;
    std::thread t([&object, &exception, method](){
        try {
            QMetaObject::invokeMethod(
                (QObject*) &object,
                method.toStdString().c_str(),
                Qt::DirectConnection
            );
        } catch (...) {
            exception = true;
        }
    });
    t.join();

    return !exception.load();
}