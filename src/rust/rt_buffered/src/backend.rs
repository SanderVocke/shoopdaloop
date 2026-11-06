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

/// Caller-owned observations at exact source boundaries. The next boundary must
/// advance after observation; it excludes controls at later samples. Both methods
/// follow the same bounded realtime contract as the backend.
pub trait SourceObserver {
    fn next(&self, start: Frame, end: Frame) -> Option<Frame>;
    fn observe(&mut self, frame: Frame, backend: &dyn Backend) -> Result<(), Error>;
}

pub struct Batch<'a, 'observer> {
    pub tag: BatchTag,
    pub frames: usize,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_controls: Option<crate::MidiControls<'a>>,
    pub midi_outputs: &'a mut [MidiBuffer],
    pub parameters: &'a [ParameterEvent],
    pub observer: Option<&'observer mut dyn SourceObserver>,
}

impl<'a> Batch<'a, '_> {
    pub fn midi_input(&self, port: usize) -> Option<crate::MidiInput<'a>> {
        self.midi_inputs.get(port).map(|ordinary| {
            crate::MidiInput::new(ordinary, self.midi_controls.filter(|_| port == 0))
        })
    }
}

/// A prepared synchronous implementation. Every method must be bounded and must
/// not allocate, deallocate, lock, wait, perform I/O or panic during normal use.
/// Processing fills every audio output and consumes events in source-time order.
pub trait Backend: Send {
    fn ports(&self) -> PortCounts;
    /// Whether processing honors source observations after equal-time controls.
    fn supports_observation(&self) -> bool {
        false
    }
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
    /// Apply a control at an inactive source boundary, without rendering audio.
    fn control(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        let _ = (parameter, value);
        Err(Error::InvalidParameter)
    }
    /// Update explicit MIDI mappings without rendering audio or creating voices.
    fn control_midi(&mut self, _data: &[u8]) -> Result<(), Error> {
        Ok(())
    }
    /// Clear stream-local history while retaining applied settings and mappings.
    /// Called only after buffered output has been aborted on an inactive owner.
    fn suspend(&mut self) -> Result<(), Error> {
        Err(Error::InvalidState)
    }
    /// One cleanup unit on an inactive owner. Return true after the final unit.
    /// The caller supplies consecutive steps beginning at zero.
    fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
        if step != 0 {
            return Err(Error::InvalidState);
        }
        self.reset()?;
        Ok(true)
    }
    fn process(&mut self, batch: &mut Batch<'_, '_>) -> Result<(), Error>;
    /// Reset only an inactive backend; implementations must bound their work.
    fn reset(&mut self) -> Result<(), Error>;
}

impl<B: Backend + ?Sized> Backend for Box<B> {
    fn ports(&self) -> PortCounts {
        (**self).ports()
    }
    fn supports_observation(&self) -> bool {
        (**self).supports_observation()
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
    fn control(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        (**self).control(parameter, value)
    }
    fn control_midi(&mut self, data: &[u8]) -> Result<(), Error> {
        (**self).control_midi(data)
    }
    fn suspend(&mut self) -> Result<(), Error> {
        (**self).suspend()
    }
    fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
        (**self).reset_step(step)
    }
    fn process(&mut self, batch: &mut Batch<'_, '_>) -> Result<(), Error> {
        (**self).process(batch)
    }
    fn reset(&mut self) -> Result<(), Error> {
        (**self).reset()
    }
}
