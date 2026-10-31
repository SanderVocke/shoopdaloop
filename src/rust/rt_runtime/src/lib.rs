//! Generic, bounded audio/MIDI processing primitives.
//!
//! Preparation and final teardown belong outside rendering. Processing receives
//! borrowed buffers and must not allocate, free, lock, wait, or perform I/O.
//! Capacity and work-budget failures are explicit errors, not permission to grow.
//!
//! [`executor::GraphRuntime`] executes typed DAGs with stable processor slots and
//! bounded plan validation/activation. An optional [`planner::Planner`] settles
//! internal intervals and supplies borrowed decisions; consumers report observations
//! without owning a second clock. The low-level [`RenderRuntime`] validates host
//! blocks and provides the empty-graph clock.
//!
//! Normal scheduled work and unread terminal results share
//! `min(scheduled_events, budgets.events)` slots; a declared planner adds one
//! independent recovery receipt. Internal intervals have no subdivision quota.
//! Command ownership, cancellation flags and execution results have separate limits.
//!
//! Resource exchange uses fixed pages and explicit revision/prefix leases. Legacy
//! resource access and optional transfers use callback-wide background allowances;
//! required [`resource::RenderIo`] data access is independently counted and checks
//! physical capacity, shape and time without consuming or renewing those allowances.
//! Only planner-lent authority manages coordinated capture lifecycle or validated
//! staged graph publication. Permits are not rollback, and copied IDs acquire no
//! lease. These finite storage/work contracts do not certify callback deadlines.

#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod buffer;
pub mod config;
pub mod error;
pub mod executor;
pub mod graph;
pub mod ids;
pub mod midi;
pub mod midi_state;
pub mod planner;
pub mod processor;
pub mod render;
pub mod resource;
pub mod schedule;
pub mod transport;

pub use config::{Budgets, Config, PreparedConfig};
pub use error::Error;
pub use ids::{Epoch, Frame, NodeId, PlannerId, ResourceId, Revision, Sequence};
pub use render::RenderRuntime;
