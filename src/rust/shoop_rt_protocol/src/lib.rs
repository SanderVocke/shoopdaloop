//! Protocol between an application adapter and the new real-time engine.
//!
//! The protocol is plain data and independent of where the engine runs. An adapter
//! talks to an engine [`carrier::Endpoint`] through a [`carrier::Carrier`]: the
//! [`carrier::InProcess`] carrier passes typed messages by direct call, while
//! [`carrier::RecordClient`] and [`carrier::RecordServer`] exchange bounded binary
//! records (see [`codec`]) over any record port, such as a `MessagePort`.
//!
//! Messages travel in three lanes with separate capacity semantics: reliable
//! requests, results and state; latest-value telemetry, which may be dropped; and
//! chunked bulk reads. A connection starts with a version handshake that announces
//! capabilities and [`schema::Limits`]; limits are current values that can change.
//! [`client::ProtocolClient`] enforces the announced credits on the client side.
#![forbid(unsafe_code)]

pub mod carrier;
pub mod client;
pub mod codec;
pub mod ids;
pub mod message;
pub mod schema;

pub use ids::{BusId, HostPortId, LoopSlotId, SlotId, StripId};
pub use message::{ClientMessage, DisconnectReason, EngineMessage, Lane};
pub use schema::*;

/// Version spoken by this crate; both sides must agree exactly.
pub const PROTOCOL_VERSION: u16 = 1;
/// Longest host port or device name, in UTF-8 bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// Most audio channels per strip direction reported in telemetry.
pub const MAX_STRIP_CHANNELS: usize = 8;
/// Longest inline MIDI message.
pub const MAX_MIDI_BYTES: usize = 16;
/// Samples per bulk audio chunk.
pub const BULK_CHUNK_SAMPLES: usize = 256;
/// Bytes per bulk MIDI chunk.
pub const BULK_CHUNK_BYTES: usize = 1024;
/// Largest encoded record.
pub const MAX_RECORD_BYTES: usize = 1280;
