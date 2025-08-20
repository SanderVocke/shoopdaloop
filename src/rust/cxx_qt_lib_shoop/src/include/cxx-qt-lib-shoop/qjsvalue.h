#pragma once
#include <QJSValue>
#include <QVariant>
#include <stdexcept>

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