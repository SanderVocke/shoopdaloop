//! Both ends of the browser MessagePort record transport.
#![forbid(unsafe_code)]

pub mod client;
pub mod server;
pub use client::RecordBatches;
#[cfg(all(target_arch = "wasm32", feature = "browser"))]
pub mod browser;
