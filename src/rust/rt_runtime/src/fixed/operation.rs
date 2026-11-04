use super::{CommandControl, GraphCompiler, Receipt, Tick, Ticket};
use crate::processor::ProcessorOperation;
use crate::resource::ResourceIo;
use crate::{Epoch, Error, Frame, NodeId};

pub struct OperationContext<'a> {
    pub graph_epoch: Epoch,
    pub tick: Tick,
    pub resources: ResourceIo<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeOperation<O> {
    pub target: NodeId,
    pub value: O,
    pub(super) slot: usize,
}
impl<O: Default> Default for NodeOperation<O> {
    fn default() -> Self {
        Self {
            target: NodeId::from_parts(0, 1).expect("initial node identity"),
            value: O::default(),
            slot: 0,
        }
    }
}

pub struct OperationControl<O: ProcessorOperation> {
    pub(super) commands: CommandControl<NodeOperation<O>, O::Result>,
    pub(super) compiler: GraphCompiler,
}
impl<O: ProcessorOperation> OperationControl<O> {
    pub fn submit(&mut self, requested: Frame, target: NodeId, value: O) -> Result<Ticket, Error> {
        let slot = self.compiler.node_slot(target)?;
        self.commands.submit(
            requested,
            NodeOperation {
                target,
                value,
                slot,
            },
        )
    }
    pub fn cancel(&mut self, target: Ticket) -> Result<Ticket, Error> {
        self.commands.cancel(target)
    }
    pub fn try_receipt(&mut self) -> Option<Receipt<NodeOperation<O>, O::Result>> {
        self.commands.try_receipt()
    }
    pub fn outstanding(&self) -> usize {
        self.commands.outstanding()
    }
}
