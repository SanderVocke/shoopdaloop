#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
mod simulator;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub use simulator::*;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
mod version;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub use version::{SUPPORTED_VERILATOR, check_verilator};
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod differential;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod mono;
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
pub mod mono_trace;
