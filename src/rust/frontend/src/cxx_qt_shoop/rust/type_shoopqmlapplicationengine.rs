use anyhow;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib-shoop/qqmlengine_helpers.h");

        #[rust_name = "qqmlapplicationengine_make_raw"]
        unsafe fn qQmlApplicationengineMakeRaw(
            parent: *mut QObject,
        ) -> Result<*mut QQmlApplicationEngine>;

        #[rust_name = "get_registered_qml_engine"]
        unsafe fn getRegisteredQmlEngine() -> Result<*mut QQmlApplicationEngine>;

        #[rust_name = "get_qml_engine_stack_trace"]
        unsafe fn getQmlEngineStackTrace(engine: &QQmlApplicationEngine) -> String;
    }
}

type ShoopQmlApplicationEngine = ffi::QQmlApplicationEngine;

pub fn make_shoop_qml_application_engine(
    parent: *mut ffi::QObject,
) -> Result<*mut ShoopQmlApplicationEngine, anyhow::Error> {
    unsafe { Ok(ffi::qqmlapplicationengine_make_raw(parent)?) }
}

// Get the globally registered QML engine.
// NOTE: this is a thread-unsafe construct. The assumption is that there is always at
// most one engine and that this function is only called on the same thread.
pub fn get_registered_qml_engine() -> Result<*mut ShoopQmlApplicationEngine, anyhow::Error> {
    unsafe { Ok(ffi::get_registered_qml_engine()?) }
}

pub fn get_qml_engine_stack_trace(engine: &ShoopQmlApplicationEngine) -> String {
    unsafe { ffi::get_qml_engine_stack_trace(engine) }
}