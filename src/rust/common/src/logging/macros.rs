#[macro_export]
macro_rules! shoop_trace { ( $($msg:tt)* ) => { log::trace!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[macro_export]
macro_rules! shoop_debug { ( $($msg:tt)* ) => { log::debug!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[macro_export]
macro_rules! shoop_info { ( $($msg:tt)* ) => { log::info!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[macro_export]
macro_rules! shoop_warn { ( $($msg:tt)* ) => { log::warn!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[macro_export]
macro_rules! shoop_error { ( $($msg:tt)* ) => { log::error!(target: SHOOP_LOG_UNIT, $($msg)*); } }


pub use shoop_error as error;
pub use shoop_warn as warn;
pub use shoop_info as info;
pub use shoop_debug as debug;
pub use shoop_trace as trace;

#[macro_export]
macro_rules! shoop_log_unit {
    ( $name:tt ) => {
        use log;
        use ctor::ctor;
        const SHOOP_LOG_UNIT : &str = $name;
        #[ctor]
        fn init_logging() {
            common::logging::register_log_module(SHOOP_LOG_UNIT);
        }
    }
}

pub use shoop_log_unit;