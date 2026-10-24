//! Generic, bounded audio/MIDI processing primitives.
//!
//! Preparation and final teardown belong outside rendering. Processing receives
//! borrowed buffers and must not allocate, free, lock, wait, or perform I/O.
//! Capacity and work-budget failures are explicit errors, not permission to grow.
//!
//! The render endpoint currently validates host blocks and emits silence. A separate
//! ownership transport provides bounded native control/completion queues. Graph and
//! resource execution are not yet implemented.

#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod buffer;
pub mod config;
pub mod error;
pub mod ids;
pub mod processor;
pub mod render;
pub mod transport;

pub use config::{Budgets, Config, PreparedConfig};
pub use error::Error;
pub use ids::{Epoch, Frame, NodeId, ResourceId, Revision, Sequence};
pub use render::RenderRuntime;
