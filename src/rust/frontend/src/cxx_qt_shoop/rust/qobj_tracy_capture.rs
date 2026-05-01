use common::logging::macros::*;
use common::tracy_capture;
use cxx_qt_lib::QString;

shoop_log_unit!("Frontend.TracyCapture");

pub use crate::cxx_qt_shoop::qobj_tracy_capture_bridge::ffi::TracyCapture;

impl TracyCapture {
    pub fn restart_capture(self: &TracyCapture) -> bool {
        match tracy_capture::restart_tracy_capture() {
            Ok(()) => {
                debug!("Tracy capture restarted successfully");
                true
            }
            Err(e) => {
                error!("Failed to restart Tracy capture: {}", e);
                false
            }
        }
    }

    pub fn restart_capture_to(self: &TracyCapture, filename: QString) -> bool {
        let filename = filename.to_string();
        match tracy_capture::restart_tracy_capture_to(&filename) {
            Ok(()) => {
                debug!("Tracy capture restarted to '{}'", filename);
                true
            }
            Err(e) => {
                error!("Failed to restart Tracy capture to '{}': {}", filename, e);
                false
            }
        }
    }
}

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_tracy_capture(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
