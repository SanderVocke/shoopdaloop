//! Timestamped MIDI storage and data operations for the common job processor.
pub(crate) mod buffers;
mod content;
mod expression;
mod memory;
pub mod performance;
pub(crate) use buffers::Buffers;
pub use content::{ContentStore, Description, StoreLimits, StoreUsage, ViewId};
pub use expression::{Expression, StateBinding, StateLimits, StateWork};
pub use memory::MidiMemory;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Error {
    InvalidEvent,
    Unsupported,
    Capacity,
    InvalidState,
    Busy,
    Overflow,
    Incomplete,
    OutputRefused,
}
macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub struct $name(pub u16);
    };
}
id!(BufferId);
id!(SequenceId);
id!(CursorId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    bytes: [u8; 3],
    len: u8,
}
impl Message {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let status = *data.first().ok_or(Error::InvalidEvent)?;
        let length = match status {
            0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
            0xc0..=0xdf | 0xf1 | 0xf3 => 2,
            0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe..=0xff => 1,
            0xf0..=0xff => return Err(Error::Unsupported),
            _ => return Err(Error::InvalidEvent),
        };
        if data.len() != length || data[1..].iter().any(|byte| *byte >= 128) {
            return Err(Error::InvalidEvent);
        }
        let mut bytes = [0; 3];
        bytes[..length].copy_from_slice(data);
        Ok(Self {
            bytes,
            len: length as u8,
        })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
    pub fn channel(self) -> Option<u8> {
        (self.bytes[0] < 0xf0).then_some(self.bytes[0] & 15)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub frame: u64,
    pub order: u32,
    pub message: Message,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TakeStatus {
    Empty,
    Recording,
    Complete,
    Incomplete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub input: u16,
    pub offset: u32,
    pub message: Message,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputEvent {
    pub destination: u16,
    pub offset: u32,
    pub message: Message,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub offset: u32,
    pub source: u32,
    pub order: u32,
    pub message: Message,
}
impl Event {
    pub(crate) fn key(self) -> (u32, u32, u32) {
        (self.offset, self.source, self.order)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct BufferLimits {
    pub buffers: u16,
    pub cursors: u16,
    /// Prepared shared scratch storage, across all intermediate event lists.
    pub scratch_events: usize,
    pub input_ports: u16,
    pub output_ports: u16,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub record_reads: u64,
    pub captured_events: u64,
    pub merged_events: u64,
    pub output_events: u64,
}
pub(crate) struct Io<'a> {
    pub memory: &'a mut MidiMemory,
    pub inputs: &'a [InputEvent],
    pub sink: &'a mut dyn FnMut(OutputEvent) -> Result<(), Error>,
}
