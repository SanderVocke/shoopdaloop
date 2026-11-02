use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use alloc::sync::Arc;
use shoop_backend::{BackendAudioChannelData, BackendAudioData, BackendMidiEvent};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PACKET: usize = 1024 * 1024;

pub(super) struct Audio {
    channels: Vec<Vec<f32>>,
    frames: usize,
    width: usize,
}
impl Audio {
    pub fn new(frames: u64, width: usize, channels: usize) -> Result<Self, String> {
        if width == 0
            || channels > width
            || frames
                .checked_mul(width as u64 * 4)
                .is_none_or(|n| n > MAX_BYTES)
        {
            return Err("audio read exceeds its shape/media budget".into());
        }
        let frames = frames as usize;
        let mut output = Vec::with_capacity(channels);
        for _ in 0..channels {
            let mut samples = Vec::new();
            samples
                .try_reserve_exact(frames)
                .map_err(|e| e.to_string())?;
            output.push(samples);
        }
        Ok(Self {
            channels: output,
            frames,
            width,
        })
    }
    pub fn push(&mut self, offset: u64, interleaved: &[f32]) -> Result<(), String> {
        if !interleaved.len().is_multiple_of(self.width) {
            return Err("ragged audio read".into());
        }
        let end = offset
            .checked_add((interleaved.len() / self.width) as u64)
            .filter(|end| *end <= self.frames as u64)
            .ok_or("audio chunk exceeds its declared frames")? as usize;
        for (index, samples) in self.channels.iter_mut().enumerate() {
            samples.resize(samples.len().max(end), 0.0);
            for (frame, value) in interleaved.chunks_exact(self.width).enumerate() {
                samples[offset as usize + frame] = value[index];
            }
        }
        Ok(())
    }
    pub fn finish(self) -> BackendAudioData {
        BackendAudioData {
            channels: self
                .channels
                .into_iter()
                .map(|samples| BackendAudioChannelData {
                    samples: Arc::new(samples),
                    ..Default::default()
                })
                .collect(),
        }
    }
}

pub(super) struct Midi {
    bytes: Vec<u8>,
    total: usize,
    at: usize,
    previous: u32,
    max_events: usize,
    events: Vec<BackendMidiEvent>,
}
impl Midi {
    pub fn new(total: u64, max_events: usize) -> Result<Self, String> {
        if total > MAX_BYTES {
            return Err("MIDI read exceeds its media budget".into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total as usize)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            bytes,
            total: total as usize,
            at: 0,
            previous: 0,
            max_events,
            events: Vec::new(),
        })
    }
    pub fn push(&mut self, offset: u64, bytes: &[u8]) -> Result<(), String> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|end| *end <= self.total as u64)
            .ok_or("MIDI chunk exceeds its declared bytes")? as usize;
        self.bytes.resize(self.bytes.len().max(end), 0);
        self.bytes[offset as usize..end].copy_from_slice(bytes);
        Ok(())
    }
    pub fn step(&mut self, budget: usize, frames: u32) -> Result<bool, String> {
        let start = self.at;
        while self.at < self.total && self.at - start < budget {
            let malformed = || "malformed MIDI content".to_owned();
            let rest = &self.bytes[self.at..];
            let frame =
                u64::from_le_bytes(rest.get(..8).ok_or_else(malformed)?.try_into().unwrap());
            let len = u32::from_le_bytes(rest.get(8..12).ok_or_else(malformed)?.try_into().unwrap())
                as usize;
            let frame = u32::try_from(frame).map_err(|_| malformed())?;
            if len == 0
                || len > MAX_PACKET
                || frame >= frames
                || frame < self.previous
                || self.events.len() >= self.max_events
            {
                return Err("MIDI content exceeds its packet/event/position budget".into());
            }
            let data = Arc::from(rest.get(12..12 + len).ok_or_else(malformed)?);
            self.events.push(BackendMidiEvent { frame, data });
            self.previous = frame;
            self.at += 12 + len;
        }
        Ok(self.at == self.total)
    }
    pub fn finish(self) -> Arc<Vec<BackendMidiEvent>> {
        Arc::new(self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn chunk_assembly_preserves_out_of_order_audio_and_yields_between_midi_packets() {
        let mut audio = Audio::new(3, 2, 2).unwrap();
        audio.push(2, &[0.5, -0.5]).unwrap();
        audio.push(0, &[-0.0, 0.25, 1.0, -1.0]).unwrap();
        let audio = audio.finish();
        assert_eq!(audio.channels[0].samples.as_slice(), [-0.0, 1.0, 0.5]);
        assert_eq!(audio.channels[0].samples[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(audio.channels[1].samples.as_slice(), [0.25, -1.0, -0.5]);
        let mut bytes = Vec::new();
        for (frame, packet) in [(1u64, [0x90, 60, 100]), (1, [0x80, 60, 0])] {
            bytes.extend_from_slice(&frame.to_le_bytes());
            bytes.extend_from_slice(&3u32.to_le_bytes());
            bytes.extend_from_slice(&packet);
        }
        let mut midi = Midi::new(bytes.len() as u64, 1_000_000).unwrap();
        midi.push(12, &bytes[12..]).unwrap();
        midi.push(0, &bytes[..12]).unwrap();
        assert!(!midi.step(1, 4).unwrap());
        assert!(midi.step(1, 4).unwrap());
        let events = midi.finish();
        assert_eq!(events[0].data.as_ref(), [0x90, 60, 100]);
        assert_eq!(events[1].data.as_ref(), [0x80, 60, 0]);
        assert_eq!(events[0].frame, events[1].frame);
    }
    #[shoop_wasm_test_support::shoop_test]
    fn assemblies_refuse_oversized_and_malformed_sources_before_publication() {
        assert!(Audio::new(MAX_BYTES / 4 + 1, 1, 1).is_err());
        assert!(Midi::new(MAX_BYTES + 1, 1_000_000).is_err());
        let mut audio = Audio::new(1, 2, 2).unwrap();
        assert!(audio.push(1, &[0.0, 0.0]).is_err());
        let mut midi = Midi::new(12, 1_000_000).unwrap();
        midi.push(0, &[0; 12]).unwrap();
        assert!(midi.step(64, 4).is_err());
    }
}
