//! Functional counterparts of the FPGA execution and memory components.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod chunk_memory;
pub mod fixed;
pub mod jobs;
pub mod records;
pub mod resolver;
#[cfg(feature = "serde")]
mod serde_index;
pub mod state;
pub mod storage;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidState,
    InvalidBuffer,
    InvalidView,
    InvalidShape,
    QueueFull,
    MemoryFull,
    UnsupportedJob,
}
