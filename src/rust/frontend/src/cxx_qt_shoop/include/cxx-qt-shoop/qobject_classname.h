#pragma once
#include <QObject>
#include <exception>
#include "rust/cxx.h"

template<typename SomeQObject>
inline rust::cxxbridge1::Str qobject_class_name(SomeQObject const& obj) {
    auto meta = obj.metaObject();
    if (!meta) {
        throw std::runtime_error("Can't determine class name for object without meta-object.");
    }
    auto name = meta->className();
    if (!name) {
        throw std::runtime_error("null classname for qobject");
    }
    return name;
}