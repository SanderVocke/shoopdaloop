#[allow(unused_macros)]
macro_rules! shoop_trace { ( $($msg:tt)* ) => { log::trace!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[allow(unused_macros)]
macro_rules! shoop_debug { ( $($msg:tt)* ) => { log::debug!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[allow(unused_macros)]
macro_rules! shoop_info { ( $($msg:tt)* ) => { log::info!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[allow(unused_macros)]
macro_rules! shoop_warn { ( $($msg:tt)* ) => { log::warn!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[allow(unused_macros)]
macro_rules! shoop_error { ( $($msg:tt)* ) => { log::error!(target: SHOOP_LOG_UNIT, $($msg)*); } }

#[allow(unused_imports)]
pub(crate) use shoop_error as error;
#[allow(unused_imports)]
pub(crate) use shoop_warn as warn;
#[allow(unused_imports)]
pub(crate) use shoop_info as info;
#[allow(unused_imports)]
pub(crate) use shoop_debug as debug;
#[allow(unused_imports)]
pub(crate) use shoop_trace as trace;

#[allow(unused_macros)]
macro_rules! shoop_log_unit {
    ( $name:tt ) => {
        pub const SHOOP_LOG_UNIT : &str = $name;
    }
}
pub(crate) use shoop_log_unit;