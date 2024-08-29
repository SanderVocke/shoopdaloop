#pragma once
#include <QObject>
#include <QMetaObject>
#include <qobjectdefs.h>
#include <rust/cxx.h>
#include <string>
#include <exception>
#include <cxx-qt-lib-shoop/reporting.h>

template<typename Obj, typename RetVal>
RetVal invoke_with_return(Obj * object,
            ::rust::String method)
{
    auto qobj = static_cast<QObject*>(object);
    auto meta = qobj->metaObject();
    int index = meta->indexOfMethod(method.c_str());

    if (index < 0) {
        report_method_not_found(object, method.operator std::string());
    }
    QMetaMethod _method = meta->method(index);
    RetVal ret;
    QMetaMethodReturnArgument _ret{_method.returnMetaType().iface(), nullptr, std::addressof(ret)};
    _method.invoke(qobj, Qt::DirectConnection, _ret);
    return ret;
}

template<typename Obj,
         typename Arg1,
         typename RetVal>
RetVal invoke_one_arg_with_return(
            Obj * object,
            ::rust::String method,
            Arg1 arg1)
{
    auto qobj = static_cast<QObject*>(object);
    auto meta = qobj->metaObject();
    int index = meta->indexOfMethod(method.c_str());

    if (index < 0) {
        report_method_not_found(object, method.operator std::string());
    }
    QMetaMethod _method = meta->method(index);
    RetVal ret;
    QMetaMethodReturnArgument _ret{_method.returnMetaType().iface(), nullptr, std::addressof(ret)};
    _method.invoke(qobj, Qt::DirectConnection, _ret, arg1);
    return ret;
}

template<typename Obj,
         typename Arg1, typename Arg2,
         typename RetVal>
RetVal invoke_two_args_with_return(
            Obj * object,
            ::rust::String method,
            Arg1 arg1, Arg2 arg2)
{
    auto qobj = static_cast<QObject*>(object);
    auto meta = qobj->metaObject();
    int index = meta->indexOfMethod(method.c_str());

    if (index < 0) {
        report_method_not_found(object, method.operator std::string());
    }
    QMetaMethod _method = meta->method(index);
    RetVal ret;
    QMetaMethodReturnArgument _ret{_method.returnMetaType().iface(), nullptr, std::addressof(ret)};
    _method.invoke(qobj, Qt::DirectConnection, _ret,
        arg1,
        arg2
    );
    return ret;
}

template<typename Obj,
         typename Arg1, typename Arg2, typename Arg3,
         typename RetVal>
RetVal invoke_three_args_with_return(
            Obj * object,
            ::rust::String method,
            Arg1 arg1, Arg2 arg2, Arg3 arg3)
{
    auto qobj = static_cast<QObject*>(object);
    auto meta = qobj->metaObject();
    int index = meta->indexOfMethod(method.c_str());

    if (index < 0) {
        report_method_not_found(object, method.operator std::string());
    }
    QMetaMethod _method = meta->method(index);
    RetVal ret;
    QMetaMethodReturnArgument _ret{_method.returnMetaType().iface(), nullptr, std::addressof(ret)};
    _method.invoke(qobj, Qt::DirectConnection, _ret,
        arg1,
        arg2,
        arg3);
    return ret;
}