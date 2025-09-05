use crate::ffi;
use anyhow;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::ffi::CString;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum LogLevel {
    AlwaysTrace = ffi::shoop_log_level_t_log_level_always_trace,
    DebugTrace = ffi::shoop_log_level_t_log_level_debug_trace,
    Debug = ffi::shoop_log_level_t_log_level_debug,
    Info = ffi::shoop_log_level_t_log_level_info,
    Warn = ffi::shoop_log_level_t_log_level_warning,
    Err = ffi::shoop_log_level_t_log_level_error,
}

pub fn set_global_logging_level(level: &LogLevel) {
    unsafe {
        ffi::set_global_logging_level(*level as ffi::shoop_log_level_t);
    }
}

#[repr(C)]
pub struct Logger {
    logger: *mut ffi::shoopdaloop_logger_t,
}

impl Logger {
    pub fn new(name: &str) -> Result<Self, anyhow::Error> {
        let c_name = CString::new(name).expect("CString::new failed");
        let logger = unsafe { ffi::get_logger(c_name.as_ptr()) };
        if logger.is_null() {
            Err(anyhow::anyhow!("Failed to create logger"))
        } else {
            Ok(Logger { logger })
        }
    }

    pub fn log(&self, level: LogLevel, msg: &str) {
        let c_msg = CString::new(msg).expect("CString::new failed");
        unsafe {
            ffi::shoopdaloop_log(self.logger, level as ffi::shoop_log_level_t, c_msg.as_ptr());
        }
    }

    pub fn should_log(&self, level: LogLevel) -> bool {
        unsafe { ffi::shoopdaloop_should_log(self.logger, level as ffi::shoop_log_level_t) != 0 }
    }
}

unsafe impl Send for Logger {}
unsafe impl Sync for Logger {}

impl Drop for Logger {
    fn drop(&mut self) {
        unsafe {
            ffi::destroy_logger(self.logger);
        }
    }
}
