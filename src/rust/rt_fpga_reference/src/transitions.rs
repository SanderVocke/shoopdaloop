//! Loop graph compilation and transition values used by the state processor.
mod plan;
mod types;
pub use plan::{
    LoopKind, LoopSpec, PlanDescription, TransitionLimits, TransitionPlan, TransitionPlanImage,
};
pub use types::*;
