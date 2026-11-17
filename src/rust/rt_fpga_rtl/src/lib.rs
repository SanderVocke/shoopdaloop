//! Shared RTL target encoding and native simulation ownership primitives.
pub mod bits;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod clock;
pub mod compiler;
pub mod component;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod model;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod pipeline;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod report;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod sim;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod toolchain;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn root() -> std::path::PathBuf {
    let path = std::env::var_os("SHOOP_FPGA_SOURCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    path.canonicalize().unwrap_or(path)
}
