//! Logical engine messages, identifiers, limits and their canonical binary codec.
//! Connections and carriers are provided by `shoop_rt_transport`.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod codec;
pub mod content;
pub mod host;
pub mod ids;
pub mod message;
pub mod replacement;
pub mod schema;

pub use content::{ContentChunk, ContentDescription, ContentFamilyEdit};
pub use ids::{BusId, HostPortId, LoopSlotId, SlotId, StripId};
pub use message::{ClientMessage, DisconnectReason, EngineMessage, Lane};
pub use replacement::{ReplacementEdit, StripPreset};
pub use schema::*;

/// Version spoken by this crate; both sides must agree exactly.
pub const PROTOCOL_VERSION: u16 = 14;
/// Reserved names for prepared I/O slots that are addressable by the host only.
pub const HOST_IO_PREFIX: &str = "@host-io/";
/// Longest host port or device name, in UTF-8 bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// Most audio channels per strip direction reported in telemetry.
pub const MAX_STRIP_CHANNELS: usize = 8;
/// Prepared bus slots; route masks cover all bus channels.
pub const MAX_BUSES: usize = 8;
/// Longest inline MIDI message.
pub const MAX_MIDI_BYTES: usize = 16;
/// Samples per bulk audio chunk.
pub const BULK_CHUNK_SAMPLES: usize = 256;
/// Bytes per bulk MIDI chunk.
pub const BULK_CHUNK_BYTES: usize = 1024;
/// Largest encoded record.
pub use shoop_transport::MAX_RECORD_BYTES;

pub use shoop_processor_protocol::{
    Kind as ProcessorKind, Policy as ProcessorPolicy, Settings as ProcessorSettings,
};

pub use shoop_processor_protocol as processors;
