//! Revisioned channel-content snapshots for realtime writers and non-realtime readers.
//!
//! # Ownership and thread model
//!
//! ```text
//! frontend/control thread                   realtime process thread
//! ┌────────────────────────┐                ┌──────────────────────────┐
//! │ SnapshotControl        │ prepare ─────▶ │ ProcessSnapshotWriter    │
//! │ ManifestReader         │                │ preallocated block pool  │
//! └──────────┬─────────────┘                └────────────┬─────────────┘
//!            │ immutable Arc snapshots                    │ bounded SPSC
//!            ▼                                            ▼
//!       stale/exact reads                    session snapshot publisher thread
//!                                             ┌─────────────────────────┐
//!                                             │ mutable private builder │
//!                                             │ immutable manifests     │
//!                                             │ returned block reclaim  │
//!                                             └─────────────────────────┘
//! ```
//!
//! One [`ContentSnapshotRuntime`] worker services all channels in a session. The process endpoint
//! owns only fixed-capacity update blocks and atomics; it never constructs or destroys manifests.
//! Prepared loads are allocated on the control thread and installed by a bounded process command.
//! The publisher performs chunk cloning, manifest allocation, `Arc` replacement, and reclamation.
//!
//! # Publication invariants
//!
//! * A manifest is immutable and contains content plus its length/duration and revision.
//! * Readers either retain the previous complete manifest or observe a complete newer manifest;
//!   private recording/replacement builders are never visible.
//! * `latest()` is nonblocking and may be stale. `try_current()` is nonblocking and returns a
//!   typed reason while a mutation, publication, or saturation is unsettled.
//! * Dirty acknowledgement names the revision actually consumed and cannot acknowledge a newer
//!   generation accidentally.
//! * The session epoch brackets every mutation so persistence can reject cross-channel mixtures.
//!
//! Channel implementations use opaque content stores and mutation operations. Pool ownership,
//! publication transport, immutable manifests, revision state, and deferred reclamation remain
//! inside this module tree.

mod audio;
mod contracts;
mod manifest;
mod midi;
mod runtime;
mod status;
mod transport;

pub(crate) use audio::{audio_snapshot_channel, AudioSnapshotPublisher};
pub use audio::{
    AudioProcessSnapshotWriter, AudioSnapshotControl, AudioSnapshotReader, PreparedAudioSnapshot,
};
pub use contracts::{
    AudioContentSnapshot, AudioSnapshotMetadata, ContentMutation, ContentRevision,
    CurrentDataError, MidiContentSnapshot, MidiSnapshotMetadata, SnapshotCurrentness, SnapshotRead,
    StaleReason,
};
pub(crate) use midi::{midi_snapshot_channel, MidiSnapshotPublisher};
pub use midi::{
    MidiProcessSnapshotWriter, MidiSnapshotControl, MidiSnapshotReader, PreparedMidiSnapshot,
};
pub use runtime::ContentSnapshotRuntime;
pub(crate) use status::{ContentStatus, SessionContentEpoch};
