#pragma once
#include <QVariant>
#include <QObject>
#include <rust/cxx.h>

inline ::rust::Str qvariantTypeName(const QVariant& v) {
    return ::rust::Str(v.typeName());
}

inline int qvariantTypeId(const QVariant& v) {
    return v.typeId();
}

template<typename T>
inline bool qvariantConvertibleTo(QVariant const& v, T* example) {
    return v.canConvert<T>();
}

template<typename T>
inline T qvariantAs(QVariant const& v, T* example) {
    return v.value<T>();
}

template<typename T>
inline QVariant asQVariant(T const& v) {
    return QVariant::fromValue(v);
}

template<typename T>
inline std::unique_ptr<T> qvariantAsUniquePtr(QVariant const& v, T* example) {
    return std::make_unique<T>(v.value<T>());

}