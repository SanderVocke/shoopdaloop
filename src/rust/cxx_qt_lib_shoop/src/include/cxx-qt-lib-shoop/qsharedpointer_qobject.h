#pragma once
#include <QSharedPointer>
#include <QObject>
#include <memory>

using QSharedPointer_QObject = QSharedPointer<QObject>;

inline void doDeleteLater(QObject *obj) {
    obj->deleteLater();
}

inline std::unique_ptr<QSharedPointer_QObject> qSharedPointerFromPtrDeleteLater(QObject *ptr) {
    return std::make_unique<QSharedPointer_QObject>(ptr, doDeleteLater);
}

inline std::unique_ptr<QSharedPointer_QObject> qSharedPointerCopy(QSharedPointer_QObject const& other) {
    return std::make_unique<QSharedPointer_QObject>(other);
}

inline QObject* qSharedPointerData(QSharedPointer_QObject const& ptr) {
    return ptr.data();
}