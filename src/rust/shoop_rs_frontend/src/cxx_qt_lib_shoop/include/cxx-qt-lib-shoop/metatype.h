#pragma once
#include <QMetaType>
#include <rust/cxx.h>

template<typename T>
::rust::Str meta_type_name(T const& t) {
    const char* name = t.metaObject()->metaType().name();
    return rust::Str(name);
}