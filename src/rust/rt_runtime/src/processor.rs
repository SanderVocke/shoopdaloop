//! Processor preparation is separate from bounded rendering.

use crate::buffer::{AudioPorts, MidiBuffer};
use crate::graph::PortCounts;
use crate::planner::{NoPlanner, Planner};
use crate::{Error, Frame, NodeId, PreparedConfig};
use alloc::boxed::Box;

/// Construction and disposal of processors belong to non-render contexts.
/// Implementations must preallocate and pre-touch their working storage here.
pub trait PrepareProcessor<L: Planner = NoPlanner, Q: ProcessorOperation = ()> {
    type Prepared: Processor<L, Q>;
    fn prepare(self, config: PreparedConfig) -> Result<Self::Prepared, Error>;
}

pub struct ProcessBlock<'a, L: Planner = NoPlanner> {
    pub node: NodeId,
    pub graph_epoch: crate::Epoch,
    pub callback_start: Frame,
    pub frame: Frame,
    pub frames: usize,
    pub quantum: crate::Quantum,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: crate::resource::ResourceIo<'a>,
    pub domain: &'a mut L,
}

pub struct OperationContext<'a> {
    pub quantum: crate::Quantum,
    pub graph_epoch: crate::Epoch,
    pub frame: Frame,
    pub requested_frame: Frame,
    pub sequence: crate::Sequence,
    pub payload: &'a [u8],
    pub resources: crate::resource::ResourceIo<'a>,
}

/// A Copy operation family accepted by processors, independent of any planner's
/// operations. `()` is the family of processors without typed operations.
pub trait ProcessorOperation: Copy + Send + 'static {
    type Result: Copy + Send + 'static;
}
impl ProcessorOperation for () {
    type Result = ();
}

/// Implementations must not allocate, deallocate, lock, wait, perform I/O, or panic
/// in normal processing. Return a compact error if a bounded output writer fills.
/// No type-level promise is made about untrusted third-party implementations.
pub trait Processor<L: Planner = NoPlanner, Q: ProcessorOperation = ()>: Send {
    /// Validate the actual node identity and domain before installation.
    /// Admission may run on-render and must obey the processing contract.
    fn admit(&self, _node: NodeId, _domain: &L) -> Result<(), Error> {
        Ok(())
    }
    /// Maximum additional operation payload, cached during preparation.
    fn operation_bytes(&self) -> usize {
        0
    }
    /// Queried during preparation, never by the render callback.
    fn ports(&self) -> PortCounts;
    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error>;
    /// Typed processor operations are immediate; only planners may retain receipts.
    fn operation(
        &mut self,
        _operation: Q,
        _context: OperationContext<'_>,
    ) -> Result<Q::Result, Error> {
        Err(Error::InvalidEvent)
    }
    /// Apply a bounded parameter mutation immediately before its target sample.
    fn set_parameter(&mut self, _parameter: u32, _value: f32) -> Result<(), Error> {
        Err(Error::InvalidParameter)
    }
}

impl<L: Planner, Q: ProcessorOperation, P: Processor<L, Q> + ?Sized> Processor<L, Q> for Box<P> {
    fn admit(&self, node: NodeId, domain: &L) -> Result<(), Error> {
        (**self).admit(node, domain)
    }
    fn operation_bytes(&self) -> usize {
        (**self).operation_bytes()
    }
    fn operation(
        &mut self,
        operation: Q,
        context: OperationContext<'_>,
    ) -> Result<Q::Result, Error> {
        (**self).operation(operation, context)
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        (**self).set_parameter(parameter, value)
    }
    fn ports(&self) -> PortCounts {
        (**self).ports()
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        (**self).process(block)
    }
}
