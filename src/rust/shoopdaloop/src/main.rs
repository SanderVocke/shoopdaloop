#[cfg(not(feature = "prebuild"))]
mod main_impl;

#[cfg(not(feature = "prebuild"))]
pub use main_impl::*;

#[cfg(feature = "prebuild")]
fn main() {}