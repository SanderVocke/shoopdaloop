use common::logging::macros::*;
shoop_log_unit!("Frontend.Utils");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type GlobalUtils = super::GlobalUtilsRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        pub unsafe fn set_window_icon_path(self: &GlobalUtils, window: *mut QObject, path: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_globalutils"]
        unsafe fn register_qml_singleton(
            inference_example: *mut GlobalUtils,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

#[derive(Default)]
pub struct GlobalUtilsRust {}
