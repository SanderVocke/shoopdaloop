//! Processor preparation is separate from bounded rendering.

use crate::buffer::{AudioPorts, MidiBuffer};
use crate::graph::PortCounts;
use crate::planner::{ConsumerAccess, DecisionView, NoPlanner, ObservationWriter, Planner};
use crate::{Error, Frame, PreparedConfig};

/// Construction and disposal of processors belong to non-render contexts.
/// Implementations must preallocate and pre-touch their working storage here.
pub trait PrepareProcessor<L: Planner = NoPlanner> {
    type Prepared: Processor<L>;
    fn prepare(self, config: PreparedConfig) -> Result<Self::Prepared, Error>;
}

pub struct ProcessBlock<'a, L: Planner = NoPlanner> {
    pub frame: Frame,
    pub frames: usize,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: crate::resource::ResourceIo<'a>,
    pub decision: Option<DecisionView<'a, L::Decision>>,
    pub observations: ObservationWriter<'a, L::Observation>,
}

pub struct OperationContext<'a> {
    pub frame: Frame,
    pub requested_frame: Frame,
    pub sequence: crate::Sequence,
    pub payload: &'a [u8],
    pub resources: crate::resource::ResourceIo<'a>,
}

/// Implementations must not allocate, deallocate, lock, wait, perform I/O, or panic
/// in normal processing. Return a compact error if a bounded output writer fills.
/// No type-level promise is made about untrusted third-party implementations.
pub trait Processor<L: Planner = NoPlanner>: Send {
    /// Queried during preparation, never by the render callback.
    fn ports(&self) -> PortCounts;
    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error>;
    /// Queried during preparation and retained across same-signature replacement.
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess::default()
    }
    /// Typed processor operations are immediate; only planners may retain receipts.
    fn operation(
        &mut self,
        _operation: L::Operation,
        _context: OperationContext<'_>,
    ) -> Result<L::Result, Error> {
        Err(Error::InvalidEvent)
    }
    /// Apply a bounded parameter mutation immediately before its target sample.
    fn set_parameter(&mut self, _parameter: u32, _value: f32) -> Result<(), Error> {
        Err(Error::InvalidParameter)
    }
}

impl<L: Planner, P: Processor<L> + ?Sized> Processor<L> for Box<P> {
    fn operation(
        &mut self,
        operation: L::Operation,
        context: OperationContext<'_>,
    ) -> Result<L::Result, Error> {
        (**self).operation(operation, context)
    }
    fn planning(&self) -> ConsumerAccess {
        (**self).planning()
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
