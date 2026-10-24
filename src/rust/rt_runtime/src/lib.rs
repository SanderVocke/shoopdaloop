//! Generic, bounded audio/MIDI processing primitives.
//!
//! Preparation and final teardown belong outside rendering. Processing receives
//! borrowed buffers and must not allocate, free, lock, wait, or perform I/O.
//! Capacity and work-budget failures are explicit errors, not permission to grow.
//!
//! This crate is being implemented in stages. The initial render endpoint validates
//! host blocks and emits silence; graph, transport, and resource facilities follow.

#![forbid(unsafe_code)]

pub mod buffer;
pub mod config;
pub mod error;
pub mod ids;
pub mod processor;
pub mod render;

pub use config::{Budgets, Config, PreparedConfig};
pub use error::Error;
pub use ids::{Epoch, Frame, NodeId, ResourceId, Revision, Sequence};
pub use render::RenderRuntime;
