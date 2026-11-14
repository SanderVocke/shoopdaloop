//! Processor preparation is separate from bounded rendering.

use crate::buffer::{AudioPorts, MidiBuffer, MidiCapacity};
use crate::graph::PortCounts;
use crate::planner::{NoPlanner, Planner};
use crate::Sample;
use crate::{Error, Frame, NodeId, PreparedConfig};
use alloc::boxed::Box;

/// Construction and disposal of processors belong to non-render contexts.
/// Implementations must preallocate and pre-touch their working storage here.
pub trait PrepareProcessor<L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32> {
    type Prepared: Processor<L, Q, S>;
    fn prepare(self, config: PreparedConfig) -> Result<Self::Prepared, Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiPortCapacity {
    pub input: bool,
    pub port: usize,
    pub capacity: MidiCapacity,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiBufferSettings {
    /// Bounded overrides for independently routed ports; other ports use defaults.
    pub ports: [Option<MidiPortCapacity>; 8],
    pub input: Option<MidiCapacity>,
    pub output: Option<MidiCapacity>,
    /// Forwarded MIDI outputs borrow this input after processing.
    pub forward_input: Option<usize>,
}
impl MidiBufferSettings {
    pub fn capacity(self, input: bool, port: usize, default: MidiCapacity) -> MidiCapacity {
        self.ports
            .iter()
            .flatten()
            .find(|entry| entry.input == input && entry.port == port)
            .map_or_else(
                || if input { self.input } else { self.output }.unwrap_or(default),
                |entry| entry.capacity,
            )
    }
    pub fn prepared_bytes(self, ports: PortCounts, default: MidiCapacity) -> Result<usize, Error> {
        if self
            .forward_input
            .is_some_and(|input| input >= ports.midi_inputs)
            || self.forward_input.is_some() && self.output.is_some()
        {
            return Err(Error::InvalidConfig);
        }
        for (index, entry) in self
            .ports
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.map(|entry| (index, entry)))
        {
            if entry.port
                >= if entry.input {
                    ports.midi_inputs
                } else {
                    ports.midi_outputs
                }
                || self.ports[..index]
                    .iter()
                    .flatten()
                    .any(|old| old.input == entry.input && old.port == entry.port)
            {
                return Err(Error::InvalidConfig);
            }
        }
        let mut bytes = ports
            .midi_inputs
            .checked_mul(default.prepared_bytes()?)
            .ok_or(Error::Capacity)?;
        for (input, count) in [(true, ports.midi_inputs), (false, ports.midi_outputs)] {
            for port in 0..count {
                bytes = bytes
                    .checked_add(self.capacity(input, port, default).prepared_bytes()?)
                    .ok_or(Error::Capacity)?;
            }
        }
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        Ok(bytes)
    }
}

pub struct ProcessBlock<'a, L: Planner<S> = NoPlanner, S: Sample = f32> {
    pub node: NodeId,
    pub graph_epoch: crate::Epoch,
    pub callback_start: Frame,
    pub frame: Frame,
    pub frames: usize,
    pub quantum: crate::Quantum,
    pub audio: AudioPorts<'a, S>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub midi_forwarded: &'a mut [bool],
    pub resources: crate::resource::ResourceIo<'a, S>,
    pub domain: &'a mut L,
}

impl<L: Planner<S>, S: Sample> ProcessBlock<'_, L, S> {
    pub fn forward_midi(&mut self, port: usize) -> Result<(), Error> {
        *self
            .midi_forwarded
            .get_mut(port)
            .ok_or(Error::InvalidGraph)? = true;
        Ok(())
    }
}

pub struct OperationContext<'a, S: Sample = f32> {
    pub quantum: crate::Quantum,
    pub graph_epoch: crate::Epoch,
    pub frame: Frame,
    pub requested_frame: Frame,
    pub sequence: crate::Sequence,
    pub payload: &'a [u8],
    pub resources: crate::resource::ResourceIo<'a, S>,
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
pub trait Processor<L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32>:
    Send
{
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
    /// Apply prepared state after graph activation, before snapshot capture.
    fn settle(&mut self, _epoch: crate::Epoch, _domain: &L) {}
    fn process(&mut self, block: &mut ProcessBlock<'_, L, S>) -> Result<(), Error>;
    /// Typed processor operations are immediate; only planners may retain receipts.
    fn operation(
        &mut self,
        _operation: Q,
        _context: OperationContext<'_, S>,
    ) -> Result<Q::Result, Error> {
        Err(Error::InvalidEvent)
    }
    /// Apply a bounded parameter mutation immediately before its target sample.
    fn set_parameter(&mut self, _parameter: u32, _value: f32) -> Result<(), Error> {
        Err(Error::InvalidParameter)
    }
}

impl<L: Planner<S>, Q: ProcessorOperation, P: Processor<L, Q, S> + ?Sized, S: Sample>
    Processor<L, Q, S> for Box<P>
{
    fn settle(&mut self, epoch: crate::Epoch, domain: &L) {
        (**self).settle(epoch, domain);
    }
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
        context: OperationContext<'_, S>,
    ) -> Result<Q::Result, Error> {
        (**self).operation(operation, context)
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        (**self).set_parameter(parameter, value)
    }
    fn ports(&self) -> PortCounts {
        (**self).ports()
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, L, S>) -> Result<(), Error> {
        (**self).process(block)
    }
}
