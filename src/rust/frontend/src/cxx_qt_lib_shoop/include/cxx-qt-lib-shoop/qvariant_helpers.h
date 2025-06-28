#pragma once
#include <QVariant>
#include <QObject>
#include <rust/cxx.h>

inline ::rust::Str qvariantTypeName(const QVariant& variant) {
    return ::rust::Str(variant.typeName());
}

inline QObject* qvariantToQObjectPtr(const QVariant& variant) {
    return variant.value<QObject*>();
}

inline QVariant qobjectPtrToQVariant(QObject* obj) {
    return QVariant::fromValue(obj);
}