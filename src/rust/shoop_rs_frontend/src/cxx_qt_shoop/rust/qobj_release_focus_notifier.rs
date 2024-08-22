use crate::logging::macros::*;
shoop_log_unit!("Frontend.ReleaseFocusNotifier");

pub use crate::cxx_qt_shoop::qobj_release_focus_notifier_bridge::ffi::ReleaseFocusNotifier;
use crate::cxx_qt_shoop::qobj_release_focus_notifier_bridge::ffi::*;
use std::pin::Pin;

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
        debug!("Notified of focus release");
        self.focus_released();
    }
}