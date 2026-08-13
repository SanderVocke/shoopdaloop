use super::{
    AudioContentSnapshot, ContentRevision, ContentStatus, CurrentDataError, MidiContentSnapshot,
    SnapshotCurrentness, SnapshotRead,
};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub trait ContentSnapshot: Send + Sync + 'static {
    fn revision(&self) -> ContentRevision;
}

impl ContentSnapshot for AudioContentSnapshot {
    fn revision(&self) -> ContentRevision {
        self.revision
    }
}

impl ContentSnapshot for MidiContentSnapshot {
    fn revision(&self) -> ContentRevision {
        self.revision
    }
}

struct SharedManifest<T> {
    current: ArcSwap<T>,
    status: Arc<ContentStatus>,
    acknowledged_revision: AtomicU64,
}

pub struct ManifestPublisher<T> {
    shared: Arc<SharedManifest<T>>,
}

#[derive(Clone)]
pub struct ManifestReader<T> {
    shared: Arc<SharedManifest<T>>,
}

pub fn manifest_pair<T: ContentSnapshot>(
    initial: T,
    status: Arc<ContentStatus>,
) -> (ManifestPublisher<T>, ManifestReader<T>) {
    status.mark_published(initial.revision());
    let shared = Arc::new(SharedManifest {
        current: ArcSwap::from_pointee(initial),
        status,
        acknowledged_revision: AtomicU64::new(0),
    });
    (
        ManifestPublisher {
            shared: Arc::clone(&shared),
        },
        ManifestReader { shared },
    )
}

impl<T: ContentSnapshot> ManifestPublisher<T> {
    pub fn publish(&self, snapshot: T) {
        let revision = snapshot.revision();
        self.shared.current.store(Arc::new(snapshot));
        self.shared.status.mark_published(revision);
    }

    pub(crate) fn recover_saturation(&self) {
        self.shared.status.recover_saturation();
    }
}

impl<T: ContentSnapshot> ManifestReader<T> {
    pub fn latest(&self) -> SnapshotRead<T> {
        SnapshotRead {
            snapshot: self.shared.current.load_full(),
            currentness: self.shared.status.currentness(),
        }
    }

    pub fn try_current(&self) -> Result<Arc<T>, CurrentDataError> {
        let required = self.shared.status.require_current()?;
        let snapshot = self.shared.current.load_full();
        if snapshot.revision() < required {
            return Err(CurrentDataError::PublicationPending {
                settled: required,
                published: snapshot.revision(),
            });
        }
        Ok(snapshot)
    }

    pub fn acknowledge(&self, revision: ContentRevision) {
        self.shared
            .acknowledged_revision
            .fetch_max(revision.0, Ordering::Release);
    }

    pub fn acknowledged_revision(&self) -> ContentRevision {
        ContentRevision(self.shared.acknowledged_revision.load(Ordering::Acquire))
    }

    pub fn is_dirty(&self) -> bool {
        self.shared.current.load().revision().0 > self.acknowledged_revision().0
    }

    pub fn currentness(&self) -> SnapshotCurrentness {
        self.shared.status.currentness()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{
        AudioSnapshotMetadata, ContentMutation, SessionContentEpoch, StaleReason,
    };

    fn audio(revision: u64, samples: &[f32]) -> AudioContentSnapshot {
        AudioContentSnapshot::new(
            ContentRevision(revision),
            AudioSnapshotMetadata {
                length: samples.len(),
            },
            Arc::from([Arc::<[f32]>::from(samples)]),
        )
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn latest_retains_a_complete_older_manifest_during_mutation() {
        let status = Arc::new(ContentStatus::new(Arc::new(SessionContentEpoch::default())));
        let (publisher, reader) = manifest_pair(audio(0, &[]), Arc::clone(&status));
        publisher.publish(audio(1, &[1.0, 2.0]));
        status.mark_published(ContentRevision(1));
        assert!(status.begin_mutation(ContentMutation::Recording));

        let read = reader.latest();
        assert_eq!(read.snapshot.contiguous(), vec![1.0, 2.0]);
        assert_eq!(
            read.currentness,
            SnapshotCurrentness::Stale(StaleReason::MutationActive(ContentMutation::Recording))
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn acknowledging_a_delivered_revision_does_not_hide_a_newer_one() {
        let status = Arc::new(ContentStatus::new(Arc::new(SessionContentEpoch::default())));
        let (publisher, reader) = manifest_pair(audio(0, &[]), Arc::clone(&status));
        publisher.publish(audio(1, &[1.0]));
        let delivered = reader.latest();
        publisher.publish(audio(2, &[2.0]));

        reader.acknowledge(delivered.snapshot.revision);
        assert_eq!(reader.acknowledged_revision(), ContentRevision(1));
        assert!(reader.is_dirty());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn exact_read_requires_the_settled_revision_to_be_published() {
        let status = Arc::new(ContentStatus::new(Arc::new(SessionContentEpoch::default())));
        let (publisher, reader) = manifest_pair(audio(0, &[]), Arc::clone(&status));
        assert!(status.begin_mutation(ContentMutation::Loading));
        let revision = status.next_revision();
        status.finish_mutation(revision);
        assert_eq!(
            reader.try_current().unwrap_err(),
            CurrentDataError::PublicationPending {
                settled: revision,
                published: ContentRevision(0),
            }
        );

        publisher.publish(audio(revision.0, &[4.0]));
        assert_eq!(
            reader
                .try_current()
                .expect("published current")
                .contiguous(),
            vec![4.0]
        );
    }
}
