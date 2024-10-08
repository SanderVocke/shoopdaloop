#[cfg(not(feature = "prebuild"))]
pub use shoopdaloop::shoopdaloop_dev_impl::*;

#[cfg(feature = "prebuild")]
fn main() {}