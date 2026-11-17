//! Device-independent, borrowed audio/MIDI rendering contract.
use crate::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Error, Frame,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Running,
    /// The control endpoint is gone: output is silent after a MIDI reset.
    Disconnected,
    Faulted(Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub next_frame: Frame,
    pub phase: Phase,
    /// Packets waiting in the MIDI recovery outputs.
    pub recovery_packets: usize,
}

/// One owner processes blocks serially. Preparation and teardown happen outside rendering.
pub trait Renderer {
    fn next_frame(&self) -> Frame;
    fn process(
        &mut self,
        audio: AudioBlock<'_, '_, f32>,
        midi: MidiBlock<'_>,
    ) -> Result<Report, Error>;
    fn midi_recovery(&self, _port: usize) -> Option<&MidiBuffer> {
        None
    }
}
