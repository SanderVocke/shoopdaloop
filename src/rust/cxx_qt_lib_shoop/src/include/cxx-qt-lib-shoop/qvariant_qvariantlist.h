#pragma once
#include <QList>
#include <QVariant>

inline bool qvariantConvertibleToQvariantlist(QVariant const& v) {
    return v.canConvert<QList<QVariant>>();
}

inline QList<QVariant> qvariantAsQvariantList(QVariant const& v) {
    return v.toList();
}

inline QVariant qvariantlistAsQvariant(QList<QVariant> const& v) {
    return QVariant(v);
}