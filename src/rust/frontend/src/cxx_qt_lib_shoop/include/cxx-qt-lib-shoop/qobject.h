#pragma once
#include <QObject>
#include <QThread>
#include <QVariant>
#include <rust/cxx.h>
#include <iostream>

template<typename T>
inline QObject* qobjectFromPtr(T *obj) {
    return static_cast<QObject*>(obj);
}

template<typename T>
inline QObject const& qobjectFromRef(T const& obj)  {
    return static_cast<QObject const&>(obj);
}

inline QObject * qobjectParent(QObject const& obj) {
    return obj.parent();
}

inline void qobjectSetParent(QObject *obj, QObject *parent) {
    obj->setParent(parent);
}

inline rust::Str qobjectClassName(QObject const& obj) {
    return rust::Str(obj.metaObject()->className());
}

inline bool qobjectPropertyBool(QObject const& obj, ::rust::String name) {
    auto result = obj.property(name.c_str());
    if (!result.isValid()) {
        std::string err = "getting property ";
        err += name.operator std::string();
        err += " yielded invalid result";
        throw std::runtime_error(err);
    }
    if (!result.canConvert<bool>()) {
        throw std::runtime_error("not convertible to bool");
    }
    return result.toBool();
}

inline int qobjectPropertyInt(QObject const& obj, ::rust::String name) {
    auto result = obj.property(name.c_str());
    if (!result.isValid()) {
        std::string err = "getting property ";
        err += name.operator std::string();
        err += " yielded invalid result";
        throw std::runtime_error(err);
    }
    if (!result.canConvert<int>()) {
        throw std::runtime_error("not convertible to int");
    }
    return result.toInt();
}

inline QString qobjectPropertyString(QObject const& obj, ::rust::String name) {
    auto result = obj.property(name.c_str());
    if (!result.isValid()) {
        std::string err = "getting property ";
        err += name.operator std::string();
        err += " yielded invalid result";
        throw std::runtime_error(err);
    }
    if (!result.canConvert<QString>()) {
        throw std::runtime_error("not convertible to QString");
    }
    return result.toString();
}

inline bool qobjectHasSignal(QObject const& obj, ::rust::String name) {
    auto meta = obj.metaObject();
    int signalIndex = meta->indexOfSignal(name.c_str());
    return signalIndex >= 0;
}

inline bool qobjectHasSlot(QObject const& obj, ::rust::String name) {
    auto meta = obj.metaObject();
    int slotIndex = meta->indexOfSlot(name.c_str());
    return slotIndex >= 0;
}

inline bool qobjectHasProperty(QObject const& obj, ::rust::String name) {
    auto meta = obj.metaObject();
    int idx = meta->indexOfProperty(name.c_str());
    return idx >= 0;
}

inline rust::Str qobjectObjectName(QObject const& obj) {
    return rust::String(obj.objectName().toStdString());
}

inline void qobjectSetObjectName(QObject *obj, ::rust::String name) {
    obj->setObjectName(name.c_str());
}

inline bool qobjectMoveToThread(QObject *obj, QThread *targetThread) {
    return obj->moveToThread(targetThread);
}