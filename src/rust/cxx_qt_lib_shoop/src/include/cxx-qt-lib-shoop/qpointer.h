#pragma once
#include <QPointer>
#include <QObject>
#include <QVariant>
#include <memory>

typedef QPointer<QObject> QPointerQObject;

inline QObject * qpointerToQObject(QPointerQObject const& p) {
    return p.data();
}

inline std::unique_ptr<QPointerQObject> qpointerFromQObject(QObject* obj) {
    return std::make_unique<QPointerQObject>(obj);
}

inline QVariant qvariantFromQPointer(QPointerQObject const& p) {
    if (p.isNull()) {
        return QVariant::fromValue(nullptr);
    }
    return QVariant::fromValue(p);
}

inline QPointerQObject qpointerFromQVariant(QVariant const& p) {
    return p.value<QPointerQObject>();
}