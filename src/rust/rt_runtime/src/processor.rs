//! Processor preparation is separate from bounded rendering.

use crate::buffer::{AudioPorts, MidiBuffer};
use crate::graph::PortCounts;
use crate::{Error, Frame, PreparedConfig};

/// Construction and disposal of processors belong to non-render contexts.
/// Implementations must preallocate and pre-touch their working storage here.
pub trait PrepareProcessor {
    type Prepared: Processor;
    fn prepare(self, config: PreparedConfig) -> Result<Self::Prepared, Error>;
}

pub struct ProcessBlock<'a> {
    pub frame: Frame,
    pub frames: usize,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: crate::resource::ResourceIo<'a>,
}

/// Implementations must not allocate, deallocate, lock, wait, perform I/O, or panic
/// in normal processing. Return a compact error if a bounded output writer fills.
/// No type-level promise is made about untrusted third-party implementations.
pub trait Processor: Send {
    /// Queried during preparation, never by the render callback.
    fn ports(&self) -> PortCounts;
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error>;
    /// Apply a bounded parameter mutation immediately before its target sample.
    fn set_parameter(&mut self, _parameter: u32, _value: f32) -> Result<(), Error> {
        Err(Error::InvalidParameter)
    }
}

impl<P: Processor + ?Sized> Processor for Box<P> {
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        (**self).set_parameter(parameter, value)
    }
    fn ports(&self) -> PortCounts {
        (**self).ports()
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        (**self).process(block)
    }
}
