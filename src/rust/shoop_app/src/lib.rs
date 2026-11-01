//! Authoritative portable session service and transitional application fixtures.
#[cfg(feature = "legacy")]
mod legacy;
#[cfg(feature = "legacy")]
pub use legacy::*;

mod service;
pub use service::{ConnectionId, Service, ServiceOptions};
