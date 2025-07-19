#pragma once
#include <QMap>
#include <QVariant>
#include <QString>

inline bool qvariantConvertibleToQvariantmap(QVariant const& v) {
    return v.canConvert<QMap<QString, QVariant>>();
}

inline QMap<QString, QVariant> qvariantAsQvariantMap(QVariant const& v) {
    return v.toMap();
}

inline QVariant qvariantmapAsQvariant(QMap<QString, QVariant> const& v) {
    return QVariant(v);
}