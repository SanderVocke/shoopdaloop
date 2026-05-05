#[cfg(not(feature = "prebuild"))]
mod lib_impl;

// Force linkage of frontend and cxx_qt_lib_shoop crates
// These are needed for cxx_qt::init_crate! to work
#[cfg(not(feature = "prebuild"))]
extern crate cxx_qt_lib_shoop;
#[cfg(not(feature = "prebuild"))]
extern crate frontend;

#[cfg(not(feature = "prebuild"))]
pub use lib_impl::*;

#[cfg(not(feature = "prebuild"))]
pub mod cli_args;

#[cfg(not(feature = "prebuild"))]
pub mod global_qml_settings;

#[cfg(not(feature = "prebuild"))]
pub mod audio_driver_names;
