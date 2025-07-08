#pragma once
#include <QVariant>
#include <QObject>
#include <rust/cxx.h>

inline ::rust::Str qvariantTypeName(const QVariant& variant) {
    return ::rust::Str(variant.typeName());
}