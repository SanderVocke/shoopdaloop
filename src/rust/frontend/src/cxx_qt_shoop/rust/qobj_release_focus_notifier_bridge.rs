use common::logging::macros::*;
shoop_log_unit!("Frontend.ReleaseFocusNotifier");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "RustQt" {
        #[qobject]
        type ReleaseFocusNotifier = super::ReleaseFocusNotifierRust;
    }

    unsafe extern "RustQt" {
        #[qsignal]
        fn focus_released(self: Pin<&mut ReleaseFocusNotifier>);

        #[qinvokable]
        fn notify(self: Pin<&mut ReleaseFocusNotifier>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_releasefocusnotifier"]
        fn make_unique() -> UniquePtr<ReleaseFocusNotifier>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_releasefocusnotifier"]
        fn register_qml_singleton(inference_example: &ReleaseFocusNotifier,
                                  module_name : &mut String,
                                  version_major : i64, version_minor : i64,
                                  type_name : &mut String);
    }
}

#[derive(Default)]
pub struct ReleaseFocusNotifierRust {}