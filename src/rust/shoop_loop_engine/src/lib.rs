//! Portable, single-owner loop timing and media execution.
//!
//! Preparation may allocate. Render-side state and scratch are prepared before
//! execution; host transport and devices are not part of this library.
#![forbid(unsafe_code)]

pub mod config;
pub mod ids;
pub mod program;
pub mod topology;

pub use config::Limits;
pub use ids::{LoopId, RequestId};
pub use program::{Entry, Mode, ProgramDescription, ProgramKind, Section};
pub use rt_runtime::{Frame, Revision, Sequence};
pub use topology::{compile, LoopKind, LoopSpec, PreparedTopology, TopologyDescription, Trigger};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidConfig,
    Capacity,
    InvalidId,
    DuplicateId,
    InvalidState,
    InvalidFrame,
    InvalidProgram,
    MissingSync,
    InstantaneousDependencyCycle,
    RecursiveProgram,
    Exhausted,
    EmptyContent,
}
