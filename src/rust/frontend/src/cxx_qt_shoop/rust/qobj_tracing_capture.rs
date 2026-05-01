use common::logging::macros::*;
use common::tracing_capture;
use cxx_qt_lib::QString;

shoop_log_unit!("Frontend.TracingCapture");

pub use crate::cxx_qt_shoop::qobj_tracing_capture_bridge::ffi::TracingCapture;

impl TracingCapture {
    pub fn restart_capture(self: &TracingCapture) -> bool {
        match tracing_capture::restart_tracing_capture() {
            Ok(()) => {
                debug!("Tracing capture restarted successfully");
                true
            }
            Err(e) => {
                error!("Failed to restart tracing capture: {}", e);
                false
            }
        }
    }

    pub fn restart_capture_to(self: &TracingCapture, filename: QString) -> bool {
        let filename = filename.to_string();
        match tracing_capture::restart_tracing_capture_to(&filename) {
            Ok(()) => {
                debug!("Tracing capture restarted to '{}'", filename);
                true
            }
            Err(e) => {
                error!("Failed to restart tracing capture to '{}': {}", filename, e);
                false
            }
        }
    }
}

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_tracing_capture(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
