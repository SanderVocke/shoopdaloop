#pragma once
#include <QVariant>
#include <QObject>

inline QObject* qvariantToQObjectPtr(const QVariant& variant) {
    return variant.value<QObject*>();
}

inline QVariant qobjectPtrToQVariant(QObject* obj) {
    return QVariant::fromValue(obj);
}