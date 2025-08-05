use common::logging::macros::*;
shoop_log_unit!("Frontend.GlobalUtils");

pub use crate::cxx_qt_shoop::qobj_global_utils_bridge::ffi::GlobalUtils;
use crate::cxx_qt_shoop::{fn_window_icons::set_window_icon_path_if_window, qobj_global_utils_bridge::ffi::*};
use cxx_qt_lib::QString;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);

    unsafe {
        register_qml_singleton_globalutils(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

#[allow(unreachable_code)]
impl GlobalUtils {
    pub unsafe fn set_window_icon_path(self: &GlobalUtils, window: *mut QObject, path: QString) {
        set_window_icon_path_if_window(window, path);
    }
}
