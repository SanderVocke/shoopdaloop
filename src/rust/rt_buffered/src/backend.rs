use alloc::boxed::Box;
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer},
    graph::PortCounts,
    Error, Frame,
};

/// Source identity remains unchanged when a synchronous batch completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchTag {
    pub generation: u64,
    pub sequence: u64,
    pub source: Frame,
}

/// A validated parameter change at a source-frame offset inside a batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterEvent {
    pub offset: u32,
    pub parameter: u32,
    pub value: f32,
}

pub struct Batch<'a> {
    pub tag: BatchTag,
    pub frames: usize,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub parameters: &'a [ParameterEvent],
}

/// A prepared synchronous implementation. Every method must be bounded and must
/// not allocate, deallocate, lock, wait, perform I/O or panic during normal use.
/// Processing fills every audio output and consumes events in source-time order.
pub trait Backend: Send {
    fn ports(&self) -> PortCounts;
    /// Delay already introduced by the DSP implementation, excluding batching.
    fn latency_frames(&self) -> usize {
        0
    }
    fn validate_parameter(&self, _parameter: u32, _value: f32) -> Result<(), Error> {
        Err(Error::InvalidParameter)
    }
    /// Read an applied scalar or encoded mapping without allocating a snapshot.
    fn parameter(&self, _parameter: u32) -> Option<f32> {
        None
    }
    fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error>;
    /// Reset only an inactive backend; implementations must bound their work.
    fn reset(&mut self) -> Result<(), Error>;
}

impl<B: Backend + ?Sized> Backend for Box<B> {
    fn ports(&self) -> PortCounts {
        (**self).ports()
    }
    fn latency_frames(&self) -> usize {
        (**self).latency_frames()
    }
    fn validate_parameter(&self, parameter: u32, value: f32) -> Result<(), Error> {
        (**self).validate_parameter(parameter, value)
    }
    fn parameter(&self, parameter: u32) -> Option<f32> {
        (**self).parameter(parameter)
    }
    fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error> {
        (**self).process(batch)
    }
    fn reset(&mut self) -> Result<(), Error> {
        (**self).reset()
    }
}
