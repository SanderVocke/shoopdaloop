//! Bounded logical-loop settlement, independent of signal execution and storage.
mod engine;
mod index;
mod plan;
mod signal;
mod types;
pub use engine::{CommittedLoops, TransitionProcessor};
pub use plan::{
    LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan, TransitionPlanImage,
};
pub use signal::{OnWrap, SignalTransitions};
pub use types::*;
