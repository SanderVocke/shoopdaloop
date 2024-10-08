#[cfg(not(feature = "prebuild"))]
mod lib_impl;

#[cfg(not(feature = "prebuild"))]
pub use lib_impl::*;