#[cfg(not(feature = "prebuild"))]
mod lib_impl;

// Force linkage of frontend and cxx_qt_lib_shoop crates
// These are needed for cxx_qt::init_crate! to work
#[cfg(not(feature = "prebuild"))]
extern crate cxx_qt_lib_shoop;
#[cfg(not(feature = "prebuild"))]
extern crate frontend;

// Static initializer to call cxx-qt init functions.
// This ensures the init functions are called even in test builds,
// which creates the necessary linker references to pull in definitions from rlibs.
#[cfg(not(feature = "prebuild"))]
#[ctor::ctor]
fn __shoopdaloop_cxx_qt_init() {
    cxx_qt::init_crate!(frontend);
    cxx_qt::init_crate!(cxx_qt_lib_shoop);
}

#[cfg(not(feature = "prebuild"))]
pub use lib_impl::*;

#[cfg(not(feature = "prebuild"))]
pub mod cli_args;

#[cfg(not(feature = "prebuild"))]
pub mod global_qml_settings;

#[cfg(not(feature = "prebuild"))]
pub mod audio_driver_names;
