//! Bounded MIDI 1.0 components for functional reference execution.
mod emission;
pub(crate) mod engine;
mod state;
mod storage;

pub use emission::{Emission, EmissionKind};
pub use engine::{Engine, ProcessError, Report};
use state::BasicUpdate;
pub use state::{ChannelState, MidiState, NoteState};
pub use storage::{Cursor, Event, Record, Take, TakeStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidEvent,
    Unsupported,
    Capacity,
    InvalidState,
    Busy,
    Overflow,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    bytes: [u8; 3],
    len: u8,
}
impl Message {
    pub fn parse(data: &[u8], max_bytes: usize) -> Result<Self, Error> {
        let status = *data.first().ok_or(Error::InvalidEvent)?;
        if matches!(status, 0xf0..=0xfe) {
            return Err(Error::Unsupported);
        }
        if data.len() > max_bytes {
            return Err(Error::Capacity);
        }
        let len = match status {
            0x80..=0xbf | 0xe0..=0xef => 3,
            0xc0..=0xdf => 2,
            0xff => 1,
            0xf0..=0xfe => return Err(Error::Unsupported),
            _ => return Err(Error::InvalidEvent),
        };
        if data.len() != len || data[1..].iter().any(|&byte| byte >= 128) {
            return Err(Error::InvalidEvent);
        }
        if status & 0xf0 == 0xb0 && matches!(data[1], 6 | 38 | 96..=101) {
            return Err(Error::Unsupported);
        }
        let mut bytes = [0; 3];
        bytes[..len].copy_from_slice(data);
        Ok(Self {
            bytes,
            len: len as u8,
        })
    }
    pub fn managed(data: &[u8]) -> Result<Self, Error> {
        Self::parse(data, 3)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
    pub fn data(&self) -> &[u8] {
        self.as_bytes()
    }
    pub fn kind(self) -> MessageKind {
        let [status, key, velocity] = self.bytes;
        let channel = status & 15;
        match status & 0xf0 {
            0x80 => MessageKind::NoteOff {
                channel,
                key,
                velocity,
            },
            0x90 if velocity == 0 => MessageKind::NoteOff {
                channel,
                key,
                velocity,
            },
            0x90 => MessageKind::NoteOn {
                channel,
                key,
                velocity,
            },
            0xb0 => MessageKind::ControlChange {
                channel,
                controller: key,
                value: velocity,
            },
            _ => MessageKind::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub update_steps: u64,
    pub emission_steps: u64,
    pub record_reads: u64,
    pub accepted: u64,
}

/// State is observable only between complete updates.
pub struct Interpreter {
    state: MidiState,
    pending: Option<BasicUpdate>,
    steps: u64,
}
impl Default for Interpreter {
    fn default() -> Self {
        Self::new(MidiState::default())
    }
}
impl Interpreter {
    pub fn new(state: MidiState) -> Self {
        Self {
            state,
            pending: None,
            steps: 0,
        }
    }
    pub fn state(&self) -> Result<&MidiState, Error> {
        if self.pending.is_some() {
            Err(Error::Busy)
        } else {
            Ok(&self.state)
        }
    }
    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn begin(&mut self, message: Message) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.pending = Some(BasicUpdate::begin(message)?);
        Ok(())
    }
    /// Visit at most one note/controller cell; true means the update is published.
    pub fn step(&mut self) -> bool {
        let Some(update) = &mut self.pending else {
            return true;
        };
        self.steps = self.steps.saturating_add(1);
        if update.step(&mut self.state) {
            self.pending = None;
            true
        } else {
            false
        }
    }
    /// Complete a bounded update, exposing its actual step count through `steps`.
    pub fn apply(&mut self, message: Message) -> Result<(), Error> {
        self.begin(message)?;
        while !self.step() {}
        Ok(())
    }
}
