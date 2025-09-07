use std::pin::Pin;

use crate::cxx_qt_shoop::qobj_logger_bridge::{ffi::{register_qml_type_logger, Logger}, *};
use backend_bindings::{Logger as BackendLogger, LogLevel};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_logger(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl Logger {
    pub fn initialize_impl(mut self: Pin<&mut Logger>) {
        self.as_mut().on_name_changed(|s| {
            let name = s.name.clone();
            s.create_logger(name);
        }).release();
    }

    pub fn create_logger(mut self: Pin<&mut Logger>, name: QString) {
        if let Ok(b) = BackendLogger::new(self.name.to_string().as_str()) {
            self.rust_mut().backend.replace(b);
        };
    }

    pub fn trace(self: &Logger, msg: QString) {
        let _ = self.backend.as_ref().map(|logger| logger.log(LogLevel::AlwaysTrace, &msg.to_string()));
    }

    pub fn debug(self: &Logger, msg: QString) {
        let _ = self.backend.as_ref().map(|logger| logger.log(LogLevel::Debug, &msg.to_string()));
    }

    pub fn info(self: &Logger, msg: QString) {
        let _ = self.backend.as_ref().map(|logger| logger.log(LogLevel::Info, &msg.to_string()));
    }

    pub fn warning(self: &Logger, msg: QString) {
        let _ = self.backend.as_ref().map(|logger| logger.log(LogLevel::Warn, &msg.to_string()));
    }

    pub fn error(self: &Logger, msg: QString) {
        let _ = self.backend.as_ref().map(|logger| logger.log(LogLevel::Err, &msg.to_string()));
    }

    pub fn should_trace(self: &Logger) -> bool {
        self.backend.as_ref().map(|logger| logger.should_log(LogLevel::AlwaysTrace)).unwrap_or(false)
    }

    pub fn should_debug(self: &Logger) -> bool {
        self.backend.as_ref().map(|logger| logger.should_log(LogLevel::Debug)).unwrap_or(false)
    }
}
