use common::logging::macros::*;
shoop_log_unit!("Frontend.OSUtils");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type OSUtils = super::OSUtilsRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn cause_abort(self: &OSUtils);

        #[qinvokable]
        fn cause_panic(self: &OSUtils);

        #[qinvokable]
        fn cause_segfault(self: &OSUtils);

        #[qinvokable]
        fn get_env_var(self: &OSUtils, var: &QString) -> QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_osutils"]
        fn make_unique() -> UniquePtr<OSUtils>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_osutils"]
        unsafe fn register_qml_singleton(
            inference_example: *mut OSUtils,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

#[derive(Default)]
pub struct OSUtilsRust {}
