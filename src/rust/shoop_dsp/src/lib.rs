#![no_std]

extern crate alloc;

use alloc::{vec, vec::Vec};

pub const MAX_CHANNELS: usize = 16;
pub const MAX_FRAMES: usize = 8192;
pub const MAX_MIDI_BYTES: usize = 64;
pub const MAX_MIDI_EVENTS: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    Capacity,
    Discontinuity,
    Unsupported,
    Algorithm,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "DSP {self:?}")
    }
}
impl core::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub sample_rate: u32,
    pub frames: usize,
    pub channels: usize,
}
impl Config {
    pub fn validate(self) -> Result<Self, Error> {
        if self.sample_rate == 0
            || self.sample_rate > 768_000
            || self.frames == 0
            || self.frames > MAX_FRAMES
            || self.channels == 0
            || self.channels > MAX_CHANNELS
        {
            return Err(Error::Shape);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latency {
    Known(u32),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub config: Config,
    pub latency: Latency,
    pub midi: bool,
    pub state: bool,
    pub ui: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transport {
    pub playing: bool,
    pub tempo: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub frame: u32,
    pub port: u16,
    pub len: u16,
    pub data: [u8; MAX_MIDI_BYTES],
}
impl Default for MidiEvent {
    fn default() -> Self {
        Self {
            frame: 0,
            port: 0,
            len: 0,
            data: [0; MAX_MIDI_BYTES],
        }
    }
}
impl MidiEvent {
    pub fn new(frame: u32, port: u16, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_MIDI_BYTES {
            return Err(Error::Capacity);
        }
        let mut event = Self {
            frame,
            port,
            len: bytes.len() as u16,
            ..Self::default()
        };
        event.data[..bytes.len()].copy_from_slice(bytes);
        Ok(event)
    }
    pub fn bytes(&self) -> Result<&[u8], Error> {
        if self.len == 0 {
            return Err(Error::Shape);
        }
        self.data.get(..self.len as usize).ok_or(Error::Capacity)
    }
}

#[derive(Debug)]
pub struct MidiBuffer {
    events: Vec<MidiEvent>,
    capacity: usize,
}
impl MidiBuffer {
    /// Construct prepared storage. Use try_new for an externally selected capacity.
    pub fn new(capacity: usize) -> Self {
        Self::try_new(capacity).expect("prepared MIDI capacity exceeds its bound")
    }
    pub fn try_new(capacity: usize) -> Result<Self, Error> {
        if capacity > MAX_MIDI_EVENTS {
            return Err(Error::Capacity);
        }
        Ok(Self {
            events: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn clear(&mut self) {
        self.events.clear();
    }
    pub fn push(&mut self, event: MidiEvent) -> Result<(), Error> {
        event.bytes()?;
        if self.events.len() == self.capacity {
            return Err(Error::Capacity);
        }
        self.events.push(event);
        Ok(())
    }
    pub fn as_slice(&self) -> &[MidiEvent] {
        &self.events
    }
}

pub struct Block<'a> {
    pub frame: u64,
    pub frames: usize,
    pub input: &'a [f32],
    pub output: &'a mut [f32],
    pub midi_input: &'a [MidiEvent],
    pub midi_output: &'a mut MidiBuffer,
    pub transport: Transport,
}
impl Block<'_> {
    pub fn validate(&self, config: Config) -> Result<(), Error> {
        if self.frames != config.frames
            || self.input.len() != self.frames * config.channels
            || self.output.len() != self.input.len()
            || self.frame.checked_add(self.frames as u64).is_none()
        {
            return Err(Error::Shape);
        }
        for event in self.midi_input {
            if event.frame as usize >= self.frames || event.port != 0 {
                return Err(Error::Shape);
            }
            event.bytes()?;
        }
        if self
            .transport
            .tempo
            .is_some_and(|t| !t.is_finite() || t <= 0.0)
        {
            return Err(Error::Shape);
        }
        Ok(())
    }
}

/// Prepare before handing exclusive ownership to a processing worker. Control
/// methods require that processing has been quiesced; borrowed buffers never escape.
pub trait Processor: Send {
    fn info(&self) -> Info;
    fn process(&mut self, block: Block<'_>) -> Result<(), Error>;
    fn reset(&mut self) -> Result<(), Error>;
    fn save_state(&mut self) -> Result<Vec<u8>, Error> {
        Err(Error::Unsupported)
    }
    fn restore_state(&mut self, _state: &[u8]) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
    fn set_visible(&mut self, _visible: bool) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}

pub struct DelayCopy {
    config: Config,
    delay: usize,
    history: Vec<f32>,
    cursor: usize,
    next_frame: Option<u64>,
}
impl DelayCopy {
    pub fn prepare(config: Config, delay: u32) -> Result<Self, Error> {
        let config = config.validate()?;
        if delay > config.sample_rate * 10 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            config,
            delay: delay as usize,
            history: vec![0.0; delay as usize * config.channels],
            cursor: 0,
            next_frame: None,
        })
    }
}
impl Processor for DelayCopy {
    fn info(&self) -> Info {
        Info {
            config: self.config,
            latency: Latency::Known(self.delay as u32),
            midi: true,
            state: false,
            ui: false,
        }
    }
    fn process(&mut self, block: Block<'_>) -> Result<(), Error> {
        block.validate(self.config)?;
        if self.next_frame.is_some_and(|frame| block.frame != frame) {
            return Err(Error::Discontinuity);
        }
        block.midi_output.clear();
        for event in block.midi_input {
            block.midi_output.push(*event)?;
        }
        for i in 0..block.frames {
            for channel in 0..self.config.channels {
                let offset = channel * block.frames + i;
                block.output[offset] = if self.delay == 0 {
                    block.input[offset]
                } else {
                    let h = channel * self.delay + self.cursor;
                    let result = self.history[h];
                    self.history[h] = block.input[offset];
                    result
                };
            }
            if self.delay > 0 {
                self.cursor = (self.cursor + 1) % self.delay;
            }
        }
        self.next_frame = Some(block.frame + block.frames as u64);
        Ok(())
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.history.fill(0.0);
        self.cursor = 0;
        self.next_frame = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn checked_midi_storage_refuses_excessive_sizes_before_allocation() {
        shoop_wasm_test_support::heap::assert_no_heap(|| {
            assert!(matches!(
                MidiBuffer::try_new(usize::MAX),
                Err(Error::Capacity)
            ));
        });
        let mut buffer = MidiBuffer::try_new(1).unwrap();
        let event = MidiEvent::new(0, 0, &[0x90, 60, 100]).unwrap();
        shoop_wasm_test_support::heap::assert_no_heap(|| {
            buffer.push(event).unwrap();
            assert_eq!(buffer.push(event), Err(Error::Capacity));
        });
    }
    #[global_allocator]
    static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
        shoop_wasm_test_support::heap::TrackingAllocator;

    #[shoop_wasm_test_support::shoop_test]
    fn delay_matches_independent_sample_reference_and_resets() {
        for frames in [1, 7, 64] {
            for delay in [0, 1, frames - 1, frames, frames + 3] {
                let config = Config {
                    sample_rate: 48_000,
                    frames,
                    channels: 3,
                };
                let mut dsp = DelayCopy::prepare(config, delay as u32).unwrap();
                let mut input = vec![0.; frames * 3];
                let mut output = input.clone();
                let mut midi = MidiBuffer::new(2);
                for _ in 0..2 {
                    for step in 0..10 {
                        for c in 0..3 {
                            for i in 0..frames {
                                input[c * frames + i] = (c * 10000 + step * frames + i + 1) as f32;
                            }
                        }
                        shoop_wasm_test_support::heap::assert_no_heap(|| {
                            dsp.process(Block {
                                frame: (step * frames) as u64,
                                frames,
                                input: &input,
                                output: &mut output,
                                midi_input: &[],
                                midi_output: &mut midi,
                                transport: Transport::default(),
                            })
                            .unwrap()
                        });
                        for c in 0..3 {
                            for i in 0..frames {
                                let n = step * frames + i;
                                let expected = if n < delay {
                                    0.
                                } else {
                                    (c * 10000 + n - delay + 1) as f32
                                };
                                assert_eq!(output[c * frames + i], expected);
                            }
                        }
                    }
                    dsp.reset().unwrap();
                }
            }
        }
    }
    #[shoop_wasm_test_support::shoop_test]
    fn validates_shapes_continuity_and_bounded_midi() {
        assert!(DelayCopy::prepare(
            Config {
                sample_rate: 0,
                frames: 8,
                channels: 2
            },
            0
        )
        .is_err());
        let mut midi = MidiBuffer::new(1);
        midi.push(MidiEvent::new(7, 0, &[0x90, 60, 100]).unwrap())
            .unwrap();
        assert_eq!(
            midi.push(MidiEvent::new(0, 0, &[0x80, 60, 0]).unwrap()),
            Err(Error::Capacity)
        );
        assert!(MidiEvent::new(0, 0, &[0; 65]).is_err());
    }
}
