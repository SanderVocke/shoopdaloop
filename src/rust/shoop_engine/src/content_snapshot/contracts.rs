use crate::midi_event::MidiEvent;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ContentRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentMutation {
    Recording = 1,
    PreRecording = 2,
    Replacing = 3,
    Loading = 4,
    Clearing = 5,
    RingbufferAdoption = 6,
}

impl ContentMutation {
    pub(super) fn from_raw(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Recording),
            2 => Some(Self::PreRecording),
            3 => Some(Self::Replacing),
            4 => Some(Self::Loading),
            5 => Some(Self::Clearing),
            6 => Some(Self::RingbufferAdoption),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    MutationActive(ContentMutation),
    PublicationPending {
        settled: ContentRevision,
        published: ContentRevision,
    },
    PublicationSaturated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCurrentness {
    Current,
    Stale(StaleReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CurrentDataError {
    #[error("channel content is changing: {0:?}")]
    MutationActive(ContentMutation),
    #[error(
        "settled content revision {settled:?} has not been published; latest is {published:?}"
    )]
    PublicationPending {
        settled: ContentRevision,
        published: ContentRevision,
    },
    #[error("channel content publication saturated")]
    PublicationSaturated,
}

impl From<StaleReason> for CurrentDataError {
    fn from(value: StaleReason) -> Self {
        match value {
            StaleReason::MutationActive(mutation) => Self::MutationActive(mutation),
            StaleReason::PublicationPending { settled, published } => {
                Self::PublicationPending { settled, published }
            }
            StaleReason::PublicationSaturated => Self::PublicationSaturated,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AudioSnapshotMetadata {
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct AudioContentSnapshot {
    pub revision: ContentRevision,
    pub metadata: AudioSnapshotMetadata,
    chunks: Arc<[Arc<[f32]>]>,
}

impl AudioContentSnapshot {
    pub fn new(
        revision: ContentRevision,
        metadata: AudioSnapshotMetadata,
        chunks: Arc<[Arc<[f32]>]>,
    ) -> Self {
        Self {
            revision,
            metadata,
            chunks,
        }
    }

    pub fn chunks(&self) -> &[Arc<[f32]>] {
        &self.chunks
    }

    pub fn samples(&self) -> impl Iterator<Item = f32> + '_ {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.iter().copied())
            .take(self.metadata.length)
    }

    pub fn contiguous(&self) -> Vec<f32> {
        self.samples().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MidiSnapshotMetadata {
    pub length: u32,
}

#[derive(Debug, Clone)]
pub struct MidiContentSnapshot {
    pub revision: ContentRevision,
    pub metadata: MidiSnapshotMetadata,
    chunks: Arc<[Arc<[MidiEvent]>]>,
}

impl MidiContentSnapshot {
    pub fn new(
        revision: ContentRevision,
        metadata: MidiSnapshotMetadata,
        chunks: Arc<[Arc<[MidiEvent]>]>,
    ) -> Self {
        Self {
            revision,
            metadata,
            chunks,
        }
    }

    pub fn chunks(&self) -> &[Arc<[MidiEvent]>] {
        &self.chunks
    }

    pub fn events(&self) -> impl Iterator<Item = &MidiEvent> {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }

    pub fn contiguous(&self) -> Vec<MidiEvent> {
        self.events().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotRead<T> {
    pub snapshot: Arc<T>,
    pub currentness: SnapshotCurrentness,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn audio_snapshot_trims_the_last_chunk_to_metadata_length() {
        let chunks = Arc::from([
            Arc::<[f32]>::from([1.0, 2.0]),
            Arc::<[f32]>::from([3.0, 99.0]),
        ]);
        let snapshot = AudioContentSnapshot::new(
            ContentRevision(4),
            AudioSnapshotMetadata { length: 3 },
            chunks,
        );
        assert_eq!(snapshot.contiguous(), vec![1.0, 2.0, 3.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_duration_is_independent_of_the_last_event() {
        let chunks = Arc::from([Arc::<[MidiEvent]>::from([MidiEvent {
            time: 2,
            data: vec![0x90, 60, 100],
        }])]);
        let snapshot = MidiContentSnapshot::new(
            ContentRevision(2),
            MidiSnapshotMetadata { length: 100 },
            chunks,
        );
        assert_eq!(snapshot.metadata.length, 100);
        assert_eq!(snapshot.events().next().map(|event| event.time), Some(2));
    }
}
