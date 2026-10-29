use crate::{ports, QUANTUM};
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{Cursor, Format, Snapshot};
use rt_runtime::{Error, ResourceId, Revision};

fn recording(result: Result<Revision, Error>) -> Result<(), Error> {
    match result {
        Ok(_) | Err(Error::Capacity | Error::Budget | Error::Exhausted) => Ok(()),
        Err(error) => Err(error),
    }
}

pub struct Node {
    pub playback: ResourceId,
    pub audio_capture: ResourceId,
    pub midi_capture: ResourceId,
    pub snapshot: Snapshot,
    pub cursor: Cursor,
    pub frames: u64,
    pub gain: f32,
}
impl Processor for Node {
    fn ports(&self) -> PortCounts {
        ports()
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 || !value.is_finite() {
            return Err(Error::InvalidParameter);
        }
        self.gain = value;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let info = block.resources.info(self.playback)?;
        if info.description.format != (Format::Audio { channels: 1 }) {
            return Err(Error::InvalidShape);
        }
        if info.revision != self.snapshot.revision {
            if let Ok(snapshot) = block.resources.snapshot(self.playback, block.frame) {
                let cursor = block.resources.cursor(snapshot.id)?;
                block.resources.release_snapshot(self.snapshot.id)?;
                self.snapshot = snapshot;
                self.cursor = cursor;
            }
        }
        let mut samples = [0f32; QUANTUM];
        for sample in &mut samples[..block.frames] {
            let mut bytes = [0; 4];
            let read = block.resources.read(&mut self.cursor, &mut bytes)?;
            if read != 0 && read != 4 {
                return Err(Error::InvalidShape);
            }
            if read == 0 {
                self.cursor = block.resources.cursor(self.snapshot.id)?;
                if block.resources.read(&mut self.cursor, &mut bytes)? != 4 {
                    return Err(Error::Incomplete);
                }
            }
            *sample = f32::from_le_bytes(bytes) * self.gain;
        }
        block
            .audio
            .output(0)
            .ok_or(Error::InvalidShape)?
            .copy_from_slice(&samples[..block.frames]);
        for (output, sample) in block
            .audio
            .output(1)
            .ok_or(Error::InvalidShape)?
            .iter_mut()
            .zip(&samples)
        {
            *output = -*sample;
        }
        if let Ok(capture) = block.resources.active_capture(self.audio_capture) {
            recording(block.resources.capture_audio(
                capture,
                block.frame,
                &samples[..block.frames],
            ))?;
        }
        if let Ok(capture) = block.resources.active_capture(self.midi_capture) {
            recording(block.resources.capture_midi(
                capture,
                block.frame,
                block.frames,
                &block.midi_inputs[0],
            ))?;
        }
        self.frames = self
            .frames
            .checked_add(block.frames as u64)
            .ok_or(Error::Exhausted)?;
        for index in 0..block.midi_inputs[0].len() {
            let (offset, data) = block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
            block.midi_outputs[0].push(offset, block.frames, data)?;
        }
        Ok(())
    }
}
