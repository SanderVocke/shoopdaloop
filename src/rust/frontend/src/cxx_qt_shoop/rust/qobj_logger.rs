use std::pin::Pin;

use crate::cxx_qt_shoop::qobj_logger_bridge::ffi::{register_qml_type_logger, Logger};
use cxx_qt_lib::QString;

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_logger(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

#[derive(Clone, Copy)]
enum FrontendLogLevel {
    AlwaysTrace,
    Debug,
    Info,
    Warn,
    Err,
}

fn should_log(level: FrontendLogLevel) -> bool {
    matches!(
        level,
        FrontendLogLevel::Info | FrontendLogLevel::Warn | FrontendLogLevel::Err
    )
}

fn log(name: &str, level: FrontendLogLevel, msg: &str) {
    if !should_log(level) {
        return;
    }
    let level = match level {
        FrontendLogLevel::AlwaysTrace => "trace",
        FrontendLogLevel::Debug => "debug",
        FrontendLogLevel::Info => "info",
        FrontendLogLevel::Warn => "warning",
        FrontendLogLevel::Err => "error",
    };
    use std::io::Write;
    let _ = writeln!(std::io::stdout(), "[{name}] [{level}] {msg}");
}

impl Logger {
    pub fn initialize_impl(mut self: Pin<&mut Logger>) {
        self.as_mut()
            .on_name_changed(|s| {
                let name = s.name.clone();
                s.create_logger(name);
            })
            .release();
    }

    pub fn create_logger(self: Pin<&mut Logger>, _name: QString) {}

    pub fn trace(self: &Logger, msg: QString) {
        log(
            &self.name.to_string(),
            FrontendLogLevel::AlwaysTrace,
            &msg.to_string(),
        );
    }

    pub fn debug(self: &Logger, msg: QString) {
        log(
            &self.name.to_string(),
            FrontendLogLevel::Debug,
            &msg.to_string(),
        );
    }

    pub fn info(self: &Logger, msg: QString) {
        log(
            &self.name.to_string(),
            FrontendLogLevel::Info,
            &msg.to_string(),
        );
    }

    pub fn warning(self: &Logger, msg: QString) {
        log(
            &self.name.to_string(),
            FrontendLogLevel::Warn,
            &msg.to_string(),
        );
    }

    pub fn error(self: &Logger, msg: QString) {
        log(
            &self.name.to_string(),
            FrontendLogLevel::Err,
            &msg.to_string(),
        );
    }

    pub fn should_trace(self: &Logger) -> bool {
        should_log(FrontendLogLevel::AlwaysTrace)
    }

    pub fn should_debug(self: &Logger) -> bool {
        should_log(FrontendLogLevel::Debug)
    }
}
