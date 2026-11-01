//! Authoritative portable session service. UI and scripting are client responsibilities.
mod service;
pub use service::{ConnectionId, Service, ServiceOptions};
