//! Portable, single-owner loop timing and media execution.
//!
//! Preparation may allocate. Render-side state and scratch are prepared before
//! execution; host transport and devices are not part of this library.
#![forbid(unsafe_code)]

pub mod command;
pub mod config;
pub mod decision;
pub mod ids;
pub mod program;
pub mod resolver;
pub mod state;
pub mod topology;

pub use command::{CancelResult, Completion, Outcome, Request, Timing, Transition};
pub use config::Limits;
pub use decision::{Decision, Event, EventKind, Interval, Observation};
pub use ids::{LoopId, RequestId};
pub use program::{Entry, Mode, ProgramDescription, ProgramKind, Section};
pub use resolver::Resolver;
pub use rt_runtime::{Epoch, Frame, Revision, Sequence};
pub use state::{CompositeState, State};
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
