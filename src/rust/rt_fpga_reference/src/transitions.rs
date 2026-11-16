//! Command admission and retained request bookkeeping for SP-owned loop plans.
mod engine;
mod plan;
mod types;
pub use engine::TransitionProcessor;
pub use plan::{
    LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan, TransitionPlanImage,
};
pub use types::*;
