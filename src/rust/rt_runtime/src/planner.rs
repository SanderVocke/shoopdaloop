//! Render-owned domain lifecycle, independent of signal ports and domain semantics.

use crate::{Error, Frame, NodeId, PlannerId, Quantum};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interval {
    pub start: Frame,
    pub end: Frame,
}

/// One tick cannot renew callback-wide ingress, validation or recovery allowances.
pub struct PlanContext<'a, O = (), R = ()> {
    pub graph_epoch: crate::Epoch,
    pub graphs: crate::graph::activation::GraphAccess<'a>,
    pub callback_start: Frame,
    pub frame: Frame,
    pub ceiling: Frame,
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Disposition<R> {
    Complete(R),
    /// Retain the receipt token and resolve it exactly once later.
    Pending,
}

pub struct OperationContext<'a, O, R> {
    pub graph_epoch: crate::Epoch,
    pub graphs: crate::graph::activation::GraphAccess<'a>,
    pub callback_start: Frame,
    pub token: crate::schedule::ReceiptToken,
    pub requested_frame: Frame,
    pub frame: Frame,
    pub recovery: bool,
    pub payload: &'a [u8],
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

pub struct FinishContext<'a, O = (), R = ()> {
    pub interval: Interval,
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

/// Callback methods obey the processor nonblocking/no-heap contract.
/// Same-frame settlement must terminate within prepared work bounds.
pub trait Planner: Send {
    type Operation: Copy + Send + 'static;
    type Result: Copy + Send + 'static;

    fn id(&self) -> Option<PlannerId> {
        None
    }
    fn operation_bytes(&self) -> usize {
        0
    }
    fn validate_quantum(&self, _quantum: Quantum) -> Result<(), Error> {
        Ok(())
    }
    fn begin_tick(
        &mut self,
        _context: PlanContext<'_, Self::Operation, Self::Result>,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn finish_tick(
        &mut self,
        _context: FinishContext<'_, Self::Operation, Self::Result>,
    ) -> Result<(), Error> {
        Ok(())
    }
    /// Timing/transition settlement is separate from signal-node execution.
    fn begin_transitions(
        &mut self,
        context: PlanContext<'_, Self::Operation, Self::Result>,
    ) -> Result<(), Error> {
        self.begin_tick(context)
    }
    /// Commit physical node/capture results before advancing transition time.
    fn finish_node_state(
        &mut self,
        _context: FinishContext<'_, Self::Operation, Self::Result>,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn finish_transitions(
        &mut self,
        context: FinishContext<'_, Self::Operation, Self::Result>,
    ) -> Result<(), Error> {
        self.finish_tick(context)
    }
    fn recovery_safe(_operation: Self::Operation) -> bool {
        false
    }
    fn operation(
        &mut self,
        _operation: Self::Operation,
        _context: OperationContext<'_, Self::Operation, Self::Result>,
    ) -> Result<Disposition<Self::Result>, Error> {
        Err(Error::InvalidEvent)
    }
    fn admit_graph(&self, _candidate: &crate::graph::PreparedPlan) -> Result<(), Error> {
        Ok(())
    }
    fn admit_processor_change(&self, _node: NodeId) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NoPlanner;
impl Planner for NoPlanner {
    type Operation = ();
    type Result = ();
}
