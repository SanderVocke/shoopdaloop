//! Processor preparation is separate from bounded rendering.

use crate::buffer::{AudioPorts, MidiBuffer, MidiCapacity};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiBufferSettings {
    pub input: Option<MidiCapacity>,
    pub output: Option<MidiCapacity>,
    /// Forwarded MIDI outputs borrow this input after processing.
    pub forward_input: Option<usize>,
}
impl MidiBufferSettings {
    pub fn prepared_bytes(self, ports: PortCounts, default: MidiCapacity) -> Result<usize, Error> {
        if self
            .forward_input
            .is_some_and(|input| input >= ports.midi_inputs)
            || self.forward_input.is_some() && self.output.is_some()
        {
            return Err(Error::InvalidConfig);
        }
        let input = self.input.unwrap_or(default).prepared_bytes()?;
        let output = self.output.unwrap_or(default).prepared_bytes()?;
        let injected = default.prepared_bytes()?;
        ports
            .midi_inputs
            .checked_mul(input.checked_add(injected).ok_or(Error::Capacity)?)
            .and_then(|bytes| {
                ports
                    .midi_outputs
                    .checked_mul(output)
                    .and_then(|output| bytes.checked_add(output))
            })
            .filter(|bytes| *bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
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
    pub midi_forwarded: &'a mut [bool],
    pub resources: crate::resource::ResourceIo<'a>,
    pub domain: &'a mut L,
}

impl<L: Planner> ProcessBlock<'_, L> {
    pub fn forward_midi(&mut self, port: usize) -> Result<(), Error> {
        *self
            .midi_forwarded
            .get_mut(port)
            .ok_or(Error::InvalidGraph)? = true;
        Ok(())
    }
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
    fn midi_buffer_settings(&self) -> Result<MidiBufferSettings, Error> {
        Ok(MidiBufferSettings::default())
    }
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
    fn midi_buffer_settings(&self) -> Result<MidiBufferSettings, Error> {
        (**self).midi_buffer_settings()
    }
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
