use common::logging::macros::*;
use cxx_qt_lib_shoop::qobject::AsQObject;
shoop_log_unit!("Frontend.TracingCapture");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++" {
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_singleton]
        type TracingCapture = super::TracingCaptureRust;

        /// Restart the tracing capture process to write to a new trace file
        /// with an auto-generated timestamp-based filename.
        /// Returns true on success, false on failure.
        #[qinvokable]
        pub fn restart_capture(self: &TracingCapture) -> bool;

        /// Restart the tracing capture process to write to a specific filename.
        /// Returns true on success, false on failure.
        #[qinvokable]
        pub fn restart_capture_to(self: &TracingCapture, filename: QString) -> bool;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_tracing_capture"]
        pub fn make_unique() -> UniquePtr<TracingCapture>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_tracing_capture"]
        unsafe fn register_qml_singleton(
            inference_example: *mut TracingCapture,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "tracing_capture_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut TracingCapture) -> *mut QObject;

        #[rust_name = "tracing_capture_qobject_from_ref"]
        fn qobjectFromRef(obj: &TracingCapture) -> &QObject;
    }
}

#[derive(Default)]
pub struct TracingCaptureRust {}

impl AsQObject for ffi::TracingCapture {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::tracing_capture_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::tracing_capture_qobject_from_ref(self) as *const ffi::QObject
    }
}
