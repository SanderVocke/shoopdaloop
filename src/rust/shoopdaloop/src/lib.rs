#[cfg(not(feature = "prebuild"))]
pub mod py_config;

#[cfg(not(feature = "prebuild"))]
pub mod shoop_rust_py;

#[cfg(not(feature = "prebuild"))]
mod lib_impl;

#[cfg(not(feature = "prebuild"))]
pub use lib_impl::*;

#[cfg(not(feature = "prebuild"))]
pub mod cli_args;
