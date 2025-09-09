#pragma once
#include <QJSValue>
#include <QVariant>
#include <stdexcept>
#include <memory>

inline void callQVariantAsCallableQJSValue(QVariant const& v) {
    if (!v.canConvert<QJSValue>()) {
        throw std::runtime_error("not convertible to QJSValue");
    }
    auto qjsvalue = v.value<QJSValue>();
    if (!qjsvalue.isCallable()) {
        throw std::runtime_error("not callable");
    }
    auto result = qjsvalue.call();
    (void) result;
}

inline void callQVariantAsCallableQJSValueBoolArg(QVariant const& v, bool arg) {
    QList<QJSValue> args;
    args.append(QJSValue(arg));

    if (!v.canConvert<QJSValue>()) {
        throw std::runtime_error("not convertible to QJSValue");
    }
    auto qjsvalue = v.value<QJSValue>();
    if (!qjsvalue.isCallable()) {
        throw std::runtime_error("not callable");
    }
    auto result = qjsvalue.call(args);
    (void) result;
}

inline bool qvariantQJSValueConvertJSObjects(QVariant &v) {
    v = v.value<QJSValue>().toVariant(QJSValue::ConvertJSObjects);
    if (v.typeId() == qMetaTypeId<QJSValue>()) {
        return false;
    }
    return true;
}