//! Shared protocol connections and carriers, independent of engine and environment.
#![forbid(unsafe_code)]

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
