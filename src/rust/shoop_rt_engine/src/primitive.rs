//! Production loop nodes alongside the shared sample-generic gain processor.
use crate::dsp::{GainOperation, GainProcessor};
use rt_runtime::{
    graph::PortCounts,
    processor::{OperationContext, ProcessBlock, Processor},
    NodeId, Sample,
};
use shoop_loops::runtime::{LoopNode, LoopPlanner};
pub enum Node<S: Sample> {
    Loop(LoopNode<S>),
    Gain(GainProcessor<S>),
}
impl<S: Sample> Processor<LoopPlanner<S>, GainOperation<S>, S> for Node<S> {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::Gain(node) => PortCounts {
                audio_inputs: node.channels(),
                audio_outputs: node.channels(),
                ..Default::default()
            },
        }
    }
    fn admit(
        &self,
        id: NodeId,
        planner: &LoopPlanner<S>,
    ) -> core::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Loop(node) => node.admit(id, planner),
            _ => Ok(()),
        }
    }
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner<S>, S>,
    ) -> core::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::Gain(node) => node.process(block),
        }
    }
    fn operation(
        &mut self,
        operation: GainOperation<S>,
        _: OperationContext<'_, S>,
    ) -> core::result::Result<(), rt_runtime::Error> {
        match self {
            Self::Gain(node) => {
                node.apply(operation);
                Ok(())
            }
            _ => Err(rt_runtime::Error::InvalidEvent),
        }
    }
}
