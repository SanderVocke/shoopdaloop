//! Functional counterparts of the FPGA execution and memory components.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(test)]
extern crate self as rt_fpga_reference;
#[cfg(test)]
extern crate std;
pub mod chunk_memory;
pub mod fixed;
pub mod jobs;
pub mod midi;
pub mod pipeline;
pub mod records;
pub mod resolver;
#[cfg(feature = "rt-interface")]
pub mod rt_interface;
#[cfg(feature = "serde")]
mod serde_index;
pub mod state;
pub mod storage;
pub mod target;
pub mod transitions;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Error {
    Transition(transitions::TransitionError),
    InvalidState,
    InvalidBuffer,
    InvalidView,
    InvalidShape,
    QueueFull,
    MemoryFull,
    UnsupportedJob,
}
