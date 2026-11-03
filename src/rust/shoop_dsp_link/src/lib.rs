#![no_std]

extern crate alloc;

pub mod memory;
pub mod returns;
pub mod service;

use alloc::{vec, vec::Vec};
pub use shoop_dsp::{Latency, MidiEvent, Transport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    Capacity,
    Deadline,
    Stale,
    Duplicate,
    Future,
    Discontinuity,
    Unavailable,
    Dsp,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "DSP link {self:?}")
    }
}
impl core::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub generation: u64,
    pub start_frame: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub quantum: u16,
    pub dsp_frames: u16,
    pub max_callback: u16,
    pub service_allowance: u32,
    pub latency: u32,
    pub slots: u16,
    pub midi_capacity: u16,
    pub result_budget: u16,
    pub algorithm_latency: Latency,
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            generation: 1,
            start_frame: 0,
            sample_rate: 48_000,
            channels: 2,
            quantum: 16,
            dsp_frames: 64,
            max_callback: 128,
            service_allowance: 128,
            latency: 480,
            slots: 64,
            midi_capacity: 16,
            result_budget: 64,
            algorithm_latency: Latency::Unknown,
        }
    }
}
impl Descriptor {
    pub fn minimum_latency(self) -> Result<u32, Error> {
        let q = u32::from(self.quantum);
        if q == 0 {
            return Err(Error::Shape);
        }
        let n = u32::from(self.max_callback)
            .checked_mul(2)
            .and_then(|n| n.checked_add(u32::from(self.dsp_frames)))
            .and_then(|n| n.checked_add(q * 2))
            .and_then(|n| n.checked_add(self.service_allowance))
            .ok_or(Error::Capacity)?;
        n.checked_add(q - 1)
            .map(|n| n / q * q)
            .ok_or(Error::Capacity)
    }
    pub fn validate(self) -> Result<Self, Error> {
        self.dsp_config().validate().map_err(|_| Error::Shape)?;
        if self.generation == 0
            || self.quantum == 0
            || self.quantum as usize > shoop_dsp::MAX_FRAMES
            || self.max_callback == 0
            || self.max_callback as usize > shoop_dsp::MAX_FRAMES
            || self.slots == 0
            || self.slots > 4096
            || self.midi_capacity > 1024
            || self.result_budget == 0
            || self.result_budget > self.slots
        {
            return Err(Error::Shape);
        }
        if self.latency < self.minimum_latency()? || self.latency % u32::from(self.quantum) != 0 {
            return Err(Error::Deadline);
        }
        let slots = self.return_slots()?;
        let service_events = u64::from(self.midi_capacity)
            * (u64::from(self.dsp_frames).div_ceil(u64::from(self.quantum)) + 1)
            + 32;
        let block_bytes = self.samples() as u64 * 4
            + u64::from(self.midi_capacity) * core::mem::size_of::<MidiEvent>() as u64;
        if slots > 8192
            || service_events > 65536
            || (slots as u64 + u64::from(self.slots) * 4 + 8) * block_bytes > 256 * 1024 * 1024
            || self
                .start_frame
                .checked_add(u64::from(self.latency) + u64::from(self.quantum))
                .is_none()
        {
            return Err(Error::Capacity);
        }
        Ok(self)
    }
    pub fn samples(self) -> usize {
        self.quantum as usize * self.channels as usize
    }
    pub fn return_slots(self) -> Result<usize, Error> {
        if self.quantum == 0 {
            return Err(Error::Shape);
        }
        Ok((self.latency / u32::from(self.quantum)) as usize + self.slots as usize + 2)
    }
    pub fn dsp_config(self) -> shoop_dsp::Config {
        shoop_dsp::Config {
            sample_rate: self.sample_rate,
            channels: self.channels as usize,
            frames: self.dsp_frames as usize,
        }
    }
    pub fn total_latency(self) -> Option<u64> {
        match self.algorithm_latency {
            Latency::Known(n) => Some(u64::from(self.latency) + u64::from(n)),
            Latency::Unknown => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Header {
    pub generation: u64,
    pub frame: u64,
    pub transport: Transport,
    pub concealed: bool,
    pub midi_reset: bool,
}

/// Views remain inside the owning process; a mapped lease supplies its own layout.
pub trait Buffer: Send {
    fn header(&self) -> Header;
    fn set_header(&mut self, header: Header);
    fn audio(&self) -> &[f32];
    fn audio_mut(&mut self) -> &mut [f32];
    fn midi(&self) -> &[MidiEvent];
    fn clear_midi(&mut self);
    fn push_midi(&mut self, event: MidiEvent) -> Result<(), Error>;
}

pub struct Block {
    header: Header,
    audio: Vec<f32>,
    midi: shoop_dsp::MidiBuffer,
}
impl Block {
    pub fn prepare(descriptor: Descriptor) -> Result<Self, Error> {
        let d = descriptor.validate()?;
        Ok(Self {
            header: Header {
                generation: d.generation,
                frame: d.start_frame,
                ..Header::default()
            },
            audio: vec![0.; d.samples()],
            midi: shoop_dsp::MidiBuffer::new(d.midi_capacity as usize),
        })
    }
    pub fn copy_from(&mut self, other: &impl Buffer) -> Result<(), Error> {
        if other.audio().len() != self.audio.len() {
            return Err(Error::Shape);
        }
        self.header = other.header();
        self.audio.copy_from_slice(other.audio());
        self.midi.clear();
        for event in other.midi() {
            self.push_midi(*event)?;
        }
        Ok(())
    }
}
impl Buffer for Block {
    fn header(&self) -> Header {
        self.header
    }
    fn set_header(&mut self, header: Header) {
        self.header = header;
    }
    fn audio(&self) -> &[f32] {
        &self.audio
    }
    fn audio_mut(&mut self) -> &mut [f32] {
        &mut self.audio
    }
    fn midi(&self) -> &[MidiEvent] {
        self.midi.as_slice()
    }
    fn clear_midi(&mut self) {
        self.midi.clear();
    }
    fn push_midi(&mut self, event: MidiEvent) -> Result<(), Error> {
        self.midi.push(event).map_err(|_| Error::Capacity)
    }
}
impl<T: Buffer + ?Sized> Buffer for alloc::boxed::Box<T> {
    fn header(&self) -> Header {
        (**self).header()
    }
    fn set_header(&mut self, h: Header) {
        (**self).set_header(h);
    }
    fn audio(&self) -> &[f32] {
        (**self).audio()
    }
    fn audio_mut(&mut self) -> &mut [f32] {
        (**self).audio_mut()
    }
    fn midi(&self) -> &[MidiEvent] {
        (**self).midi()
    }
    fn clear_midi(&mut self) {
        (**self).clear_midi();
    }
    fn push_midi(&mut self, e: MidiEvent) -> Result<(), Error> {
        (**self).push_midi(e)
    }
}

/// Every call is bounded and memory-only. Release has a reserved credit and
/// must accept each lease received from this endpoint exactly once, without
/// allocating, freeing, waiting, or performing I/O.
pub trait Endpoint: Send {
    type Lease: Buffer;
    fn descriptor(&self) -> Descriptor;
    fn available(&self) -> bool;
    fn acquire_input(&mut self) -> Option<Self::Lease>;
    fn submit(&mut self, lease: Self::Lease) -> Result<(), Self::Lease>;
    fn release_input(&mut self, lease: Self::Lease);
    fn receive(&mut self) -> Option<Self::Lease>;
    fn release_output(&mut self, lease: Self::Lease);
}

pub fn validate_block(d: Descriptor, block: &impl Buffer) -> Result<(), Error> {
    let h = block.header();
    if h.generation != d.generation {
        return Err(Error::Stale);
    }
    if h.frame < d.start_frame || (h.frame - d.start_frame) % u64::from(d.quantum) != 0 {
        return Err(Error::Shape);
    }
    if h.frame.checked_add(u64::from(d.quantum)).is_none()
        || block.audio().len() != d.samples()
        || block.midi().len() > d.midi_capacity as usize
        || h.transport.tempo.is_some_and(|t| !t.is_finite() || t <= 0.)
    {
        return Err(Error::Shape);
    }
    let mut previous = 0;
    for event in block.midi() {
        if event.frame >= u32::from(d.quantum) || event.port != 0 || event.bytes().is_err() {
            return Err(Error::Shape);
        }
        if event.frame < previous {
            return Err(Error::Shape);
        }
        previous = event.frame;
    }
    Ok(())
}
