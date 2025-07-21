#pragma once
#include <QQmlEngine>
#include <string>
#include "rust/cxx.h"

template<typename T>
inline void register_qml_type(
                       T const& inference_example,
                       ::rust::String& module_name,
                       ::std::int64_t version_major, ::std::int64_t version_minor,
                       ::rust::String& type_name)
{
    (void)inference_example;
    qmlRegisterType<T>(module_name.c_str(), version_major, version_minor, type_name.c_str());
}

template<typename T>
inline void register_qml_singleton(
                       T const& inference_example,
                       ::rust::String& module_name,
                       ::std::int64_t version_major, ::std::int64_t version_minor,
                       ::rust::String& type_name)
{
    (void)inference_example;
    qmlRegisterType<T>(module_name.c_str(), version_major, version_minor, type_name.c_str());
    qmlRegisterSingletonType<T>(module_name.c_str(), version_major, version_minor, type_name.c_str(), [](auto*, auto*) { return new T(); });
}

inline void register_qml_singleton_instance(
                       QObject * instance,
                       ::rust::String& module_name,
                       ::std::int64_t version_major, ::std::int64_t version_minor,
                       ::rust::String& type_name)
{
    qmlRegisterSingletonInstance(module_name.c_str(), version_major, version_minor, type_name.c_str(), instance);
}
