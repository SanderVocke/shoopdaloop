#pragma once
#include <QThread>

template<typename QObjectType>
inline bool is_called_from_qobj_thread(QObjectType& object) {
    return (object.thread() == QThread::currentThread());
}