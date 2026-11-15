//! Bounded logical-loop settlement, independent of signal execution and storage.
mod engine;
mod index;
mod plan;
mod types;
pub use engine::TransitionProcessor;
pub use plan::{
    LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan, TransitionPlanImage,
};
pub use types::*;
