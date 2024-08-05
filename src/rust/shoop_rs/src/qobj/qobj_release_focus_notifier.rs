use crate::logging::macros::*;
const SHOOP_LOG_UNIT : &str = "Frontend.ReleaseFocusNotifier";

#[cxx_qt::bridge]
pub mod qobj_release_focus_notifier {
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
        include!("cxx-shoop/make_unique.h");

        #[rust_name = "make_unique_releasefocusnotifier"]
        fn make_unique() -> UniquePtr<ReleaseFocusNotifier>;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_releasefocusnotifier"]
        fn register_qml_singleton(inference_example: &ReleaseFocusNotifier,
                                  module_name : &mut String,
                                  version_major : i64, version_minor : i64,
                                  type_name : &mut String);
    }
}

use qobj_release_focus_notifier::*;

pub fn register_qml_singleton(module_name : &str, type_name : &str) {
    let obj = make_unique_releasefocusnotifier();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_singleton_releasefocusnotifier(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}

#[derive(Default)]
pub struct ReleaseFocusNotifierRust {}

impl ReleaseFocusNotifier {
    pub fn notify(self: Pin<&mut ReleaseFocusNotifier>) {
        debug!("Notified of focus release, propagating.");
        self.focus_released();
    }
}