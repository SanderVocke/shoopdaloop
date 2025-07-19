#pragma once 

#include <QMap>
#include <QVariant>
#include <QString>

inline bool qvariantCanConvertQMapQStringQVariant(QVariant const& variant) {
    return variant.canConvert<QMap<QString, QVariant>>();
}