#pragma once
#include <QWeakPointer>
#include <QSharedPointer>
#include <QObject>
#include <memory>
#include <stdexcept>

using QWeakPointer_QObject = QWeakPointer<QObject>;
using QSharedPointer_QObject = QSharedPointer<QObject>;

inline std::unique_ptr<QSharedPointer_QObject> qWeakPointerToStrong(QWeakPointer_QObject const& ptr) {
    auto strong = ptr.toStrongRef();
    return std::make_unique<QSharedPointer_QObject>(strong);
}

inline std::unique_ptr<QWeakPointer_QObject> qSharedPointerToWeak(QSharedPointer_QObject const& ptr) {
    return std::make_unique<QWeakPointer_QObject>(ptr);
}

inline std::unique_ptr<QWeakPointer_QObject> qWeakPointerCopy(QWeakPointer_QObject const& other) {
    return std::make_unique<QWeakPointer_QObject>(other);
}