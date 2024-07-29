#include "cxx-qt-gen/qobject.cxxqt.h"
#include <QQmlEngine>
#include <memory>

// Singleton instances
std::unique_ptr<OSUtils> g_os_utils;

#ifdef _WIN32
  #define SHOOP_RUST_EXPORT __declspec(dllexport)
#else
  #define SHOOP_RUST_EXPORT __attribute__((visibility("default")))
#endif

extern "C" {

SHOOP_RUST_EXPORT void register_qml_types_and_singletons() {
    qmlRegisterType<OSUtils>("ShoopDaLoop.Rust", 1, 0, "ShoopOSUtils");
    qmlRegisterSingletonType<OSUtils>("ShoopDaLoop.Rust", 1, 0, "ShoopOSUtils", [](QQmlEngine*, QJSEngine*) { return new OSUtils(); });
}

}