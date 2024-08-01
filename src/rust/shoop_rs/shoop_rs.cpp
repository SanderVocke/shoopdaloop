#include "cxx-qt-gen/qobj_file_io.cxxqt.h"
#include "cxx-qt-gen/qobj_os_utils.cxxqt.h"

#include <QQmlEngine>

#ifdef _WIN32
  #define SHOOP_RUST_EXPORT __declspec(dllexport)
#else
  #define SHOOP_RUST_EXPORT __attribute__((visibility("default")))
#endif

extern "C" {

SHOOP_RUST_EXPORT void register_qml_types_and_singletons() {
    qmlRegisterType<OSUtils>("ShoopDaLoop.Rust", 1, 0, "ShoopOSUtils");
    qmlRegisterSingletonType<OSUtils>("ShoopDaLoop.Rust", 1, 0, "ShoopOSUtils", [](QQmlEngine*, QJSEngine*) { return new OSUtils(); });

    qmlRegisterType<FileIO>("ShoopDaLoop.Rust", 1, 0, "ShoopFileIO");
    qmlRegisterSingletonType<FileIO>("ShoopDaLoop.Rust", 1, 0, "ShoopFileIO", [](QQmlEngine*, QJSEngine*) { return new FileIO(); });
}

}