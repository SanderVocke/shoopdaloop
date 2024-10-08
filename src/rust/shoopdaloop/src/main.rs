#[cfg(not(feature = "prebuild"))]
pub use main_impl::*;

#[cfg(feature = "prebuild")]
fn main() {}