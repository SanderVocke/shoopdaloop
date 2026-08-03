//! Revisioned channel-content snapshots for realtime writers and non-realtime readers.
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

pub use audio::{
    audio_snapshot_channel, AudioProcessSnapshotWriter, AudioSnapshotControl,
    AudioSnapshotPublisher, AudioSnapshotReader, PreparedAudioSnapshot,
};
pub use contracts::{
    AudioContentSnapshot, AudioSnapshotMetadata, ContentMutation, ContentRevision,
    CurrentDataError, MidiContentSnapshot, MidiSnapshotMetadata, SnapshotCurrentness, SnapshotRead,
    StaleReason,
};
pub use manifest::{manifest_pair, ContentSnapshot, ManifestPublisher, ManifestReader};
pub use midi::{
    midi_snapshot_channel, MidiProcessSnapshotWriter, MidiSnapshotControl, MidiSnapshotPublisher,
    MidiSnapshotReader, PreparedMidiSnapshot,
};
pub use runtime::ContentSnapshotRuntime;
pub use status::{ContentStatus, SessionContentEpoch};
