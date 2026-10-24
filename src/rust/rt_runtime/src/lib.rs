//! Generic, bounded audio/MIDI processing primitives.
//!
//! Preparation and final teardown belong outside rendering. Processing receives
//! borrowed buffers and must not allocate, free, lock, wait, or perform I/O.
//! Capacity and work-budget failures are explicit errors, not permission to grow.
//!
//! [`executor::GraphRuntime`] executes typed DAGs with stable processor slots and
//! bounded plan validation/activation. The low-level [`RenderRuntime`] provides
//! host-block validation and the empty-graph clock. Scheduled controls and resource
//! exchange are still under development.

#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod buffer;
pub mod config;
pub mod error;
pub mod executor;
pub mod graph;
pub mod ids;
pub mod processor;
pub mod render;
pub mod transport;

pub use config::{Budgets, Config, PreparedConfig};
pub use error::Error;
pub use ids::{Epoch, Frame, NodeId, ResourceId, Revision, Sequence};
pub use render::RenderRuntime;
