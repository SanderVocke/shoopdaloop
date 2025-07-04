#pragma once
#include <QVariant>
#include <QObject>
#include <QSharedPointer>
#include <memory>

inline std::unique_ptr<QSharedPointer<QObject>> qvariantToQSharedPointerQObject(const QVariant& variant) {
    return std::make_unique<QSharedPointer<QObject>>(variant.value<QSharedPointer<QObject>>());
}

inline QVariant qSharedPointerQObjectToQVariant(QSharedPointer<QObject> const& obj) {
    return QVariant::fromValue(obj);
}