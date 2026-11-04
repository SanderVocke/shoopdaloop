//! Authoritative portable session service. UI and scripting are client responsibilities.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[cfg(test)]
extern crate std;

mod host_graph;
mod service;
pub use host_graph::{HostGraphController, HostGraphError};
pub use service::{ConnectionId, Service, ServiceOptions};
