#pragma once
#include <QJsonObject>
#include <QJsonDocument>
#include <QVariant>
#include <QByteArray>
#include <QMap>

#include <rust/cxx.h>
#include <memory>

inline QVariantMap qjsonobjectToVariantMap(QJsonObject const& o) {
    return o.toVariantMap();
}

inline std::unique_ptr<QJsonObject> qjsonobjectFromVariantMap(QVariantMap const& m) {
    auto rval = QJsonObject::fromVariantMap(m);
    return std::make_unique<QJsonObject>(std::move(rval));
}

inline ::rust::String qjsonobjectToJson(QJsonObject const& o) {
    QJsonDocument doc(o);
    return ::rust::String(doc.toJson().toStdString());
}

inline std::unique_ptr<QJsonObject> qjsonobjectFromJson(::rust::String const& json) {
    auto bytes = QByteArray::fromStdString(json.operator std::string());
    auto doc = QJsonDocument::fromJson(bytes);
    auto obj = doc.object();
    return std::make_unique<QJsonObject>(std::move(obj));
}

inline void dummy(std::unique_ptr<QJsonObject> o) {
    (void) o;
}

inline QVariant qjsonobjectToVariant(QJsonObject const& o) {
    return QJsonDocument(o).toVariant();
}