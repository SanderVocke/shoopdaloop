//! Shared protocol connections and carriers, independent of engine and environment.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod buffer;
pub mod carrier;
pub mod client;
#[cfg(feature = "contracts")]
pub mod contracts;
pub mod framing;

pub mod in_process {
    pub use crate::carrier::InProcess;
}

pub mod records {
    pub use crate::carrier::{RecordClient, RecordPort, RecordServer};
}
