#pragma once
#include <QList>
#include <QString>

inline bool qvariantConvertibleToQStringlist(QVariant const& v) {
    return v.canConvert<QList<QString>>();
}

inline QList<QString> qvariantAsQStringList(QVariant const& v) {
    return v.value<QList<QString>>();
}

inline QVariant qstringlistAsQvariant(QList<QString> const& v) {
    return QVariant(v);
}