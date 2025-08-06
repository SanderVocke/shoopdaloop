#pragma once
#include <QMetaType>
#include <rust/cxx.h>

// Note: safe to use this with nullptr. The argument is needed
// for Rust type bindings
template<typename T>
inline ::rust::String meta_type_name(T *t) {
    (void) t;
    const char* name = QMetaType::fromType<T>().name();
    return rust::String(name);
}

// Note: safe to use this with nullptr. The argument is needed
// for Rust type bindings
template<typename T>
inline int meta_type_id(T* v)
{
    (void) v;
    return qMetaTypeId<T>();
}