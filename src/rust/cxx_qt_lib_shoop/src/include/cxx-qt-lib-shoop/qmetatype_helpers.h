#pragma once

#include <QMetaType>

template<typename T>
inline int qmetatypeTypeId(T* v)
{
    (void) v;
    return qMetaTypeId<T>();
}