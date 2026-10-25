//! Generic planning metadata, independent of signal ports and domain semantics.

use crate::{Error, NodeId, PlannerId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannerSpec {
    pub id: PlannerId,
    pub decision_slots: usize,
    pub observation_capacity: usize,
    pub operation_bytes: usize,
}

impl PlannerSpec {
    pub(crate) fn validate(self) -> Result<(), Error> {
        for count in [
            self.decision_slots,
            self.observation_capacity,
            self.operation_bytes,
        ] {
            if count > i32::MAX as usize {
                return Err(Error::Capacity);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConsumerAccess {
    pub decision_required: bool,
    pub observations: usize,
    pub operation_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumerSpec {
    pub node: NodeId,
    pub access: ConsumerAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanningStep {
    pub planner: PlannerSpec,
    /// The serial lifecycle requires a planning prefix, before signal step zero.
    pub before_step: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DecisionBinding {
    pub planner: PlannerId,
    pub consumer: NodeId,
    pub decision_slot: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanningDescriptor {
    pub step: Option<PlanningStep>,
    pub bindings: Vec<DecisionBinding>,
}
