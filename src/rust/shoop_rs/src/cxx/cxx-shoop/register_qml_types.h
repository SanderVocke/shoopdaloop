#include <QQmlEngine>
#include <string>

template<typename T>
void register_qml_type(std::string const& module_name,
                       int version_major, int version_minor,
                       std::string const& type_name)
{
    qmlRegisterType<T>(module_name.c_str(), 1, 0, type_name.c_str());
}

template<typename T>
void register_qml_singleton(std::string const& module_name,
                       int version_major, int version_minor,
                       std::string const& type_name)
{
    qmlRegisterType<T>(module_name.c_str(), 1, 0, type_name.c_str());
    qmlRegisterSingletonType<T>(module_name.c_str(), 1, 0, type_name.c_str(), [](auto*, auto*) { return new T(); });
}