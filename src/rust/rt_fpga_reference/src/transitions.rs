//! Bounded logical-loop settlement, independent of signal execution and storage.
mod bindings;
mod engine;
mod index;
mod plan;
mod signal;
mod types;
pub use bindings::{LoopBinding, PlaybackBinding, StateBindings};
pub use engine::TransitionProcessor;
pub use plan::{
    LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan, TransitionPlanImage,
};
pub use signal::{OnWrap, SignalTransitions};
pub use types::*;
