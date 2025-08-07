#pragma once
#include <QPointer>
#include <QObject>
#include <QVariant>
#include <memory>

typedef QPointer<QObject> QPointerQObject;

QObject * qpointerToQObject(QPointerQObject const& p) {
    return p.data();
}

std::unique_ptr<QPointerQObject> qpointerFromQObject(QObject* obj) {
    return std::make_unique<QPointerQObject>(obj);
}

QVariant qvariantFromQPointer(QPointerQObject const& p) {
    if (p.isNull()) {
        return QVariant::fromValue(nullptr);
    }
    return QVariant::fromValue(p);
}

QPointerQObject qpointerFromQVariant(QVariant const& p) {
    return p.value<QPointerQObject>();
}