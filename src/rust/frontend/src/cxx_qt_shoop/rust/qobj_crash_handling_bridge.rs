#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type CrashHandling = super::CrashHandlingRust;

        #[qinvokable]
        pub fn set_json_toplevel_field(self: &CrashHandling, key: QString, json: QString);

        #[qinvokable]
        pub fn set_json_tag(self: &CrashHandling, tag: QString, json: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_crash_handling"]
        fn make_unique() -> UniquePtr<CrashHandling>;

        include!("cxx-qt-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_crash_handling"]
        fn register_qml_singleton(
            inference_example: &CrashHandling,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub use ffi::CrashHandling;

pub struct CrashHandlingRust {}

impl Default for CrashHandlingRust {
    fn default() -> CrashHandlingRust {
        CrashHandlingRust {}
    }
}
