//! Shared RTL target encoding and native simulation ownership primitives.
#![cfg_attr(target_os = "none", no_std)]
extern crate alloc;

pub mod bits;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod clock;
pub mod component;
pub mod host;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod model;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod pipeline;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod report;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod sim;
pub mod target;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod toolchain;
pub type Result<T> = core::result::Result<T, alloc::boxed::Box<dyn core::error::Error>>;
#[cfg(not(target_os = "none"))]
pub fn root() -> std::path::PathBuf {
    let path = std::env::var_os("SHOOP_FPGA_SOURCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    path.canonicalize().unwrap_or(path)
}
