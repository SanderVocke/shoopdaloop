//! Session connections driven by a host-provided carrier and monotonic elapsed time.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
mod server;
mod upload;
pub use server::{Management, Session};
pub use upload::{Error, Upload};
