//! Portable, single-owner loop timing and media execution.
//!
//! Preparation may allocate. Render-side state and scratch are prepared before
//! execution. [`Resolver`] is the musical authority; low-level renderers execute
//! supplied positions/discontinuities without advancing another clock.
//!
//! [`runtime::Engine`] coordinates prepared signal consumers, capture preflight,
//! typed requests and settled state/export. Timing-only, fixed-role routing,
//! stopped-owner configuration and pending program/graph publication are distinct
//! transactions over prepared physical templates, not arbitrary live construction.
//! [`media::Ledger`] manages explicit leases/references but cannot certify a musical
//! cut by itself. Copied handles do not acquire ownership; caller-routed LoopId and
//! RequestId values are not globally scoped endpoint capabilities.
//!
//! Final teardown is off-render and explicit. Permanent quiescence recovers actual
//! owners without rollback, callback replay or device cleanup. Devices, browser
//! bindings, unrestricted port resizing and hard-deadline guarantees are excluded.
#![forbid(unsafe_code)]

pub mod audio;
pub mod command;
pub mod config;
pub mod decision;
pub mod ids;
pub mod media;
pub mod midi;
pub mod midi_restore;
pub mod program;
pub mod resolver;
pub mod runtime;
pub mod state;
pub mod topology;

pub use command::{CancelResult, Completion, Outcome, Request, Timing, Transition};
pub use config::Limits;
pub use decision::{Decision, Event, EventKind, Interval, Observation};
pub use ids::{LoopId, RequestId};
pub use program::{Entry, Mode, ProgramDescription, ProgramKind, Section};
pub use resolver::{ProgramBoundary, ProgramChangeResult, Resolver};
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
