use super::{ContentMutation, ContentRevision, CurrentDataError, SnapshotCurrentness, StaleReason};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct SessionContentEpoch {
    epoch: AtomicU64,
    active_mutations: AtomicU32,
}

impl SessionContentEpoch {
    pub fn capture(&self) -> Option<u64> {
        (self.active_mutations.load(Ordering::Acquire) == 0)
            .then(|| self.epoch.load(Ordering::Acquire))
    }

    pub fn validate(&self, captured: u64) -> bool {
        self.active_mutations.load(Ordering::Acquire) == 0
            && self.epoch.load(Ordering::Acquire) == captured
    }

    fn mutation_started(&self) {
        self.active_mutations.fetch_add(1, Ordering::AcqRel);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn mutation_finished(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.active_mutations.fetch_sub(1, Ordering::AcqRel);
    }
}

const NO_MUTATION: u8 = 0;

#[derive(Debug)]
pub struct ContentStatus {
    epoch: Arc<SessionContentEpoch>,
    mutation: AtomicU8,
    next_revision: AtomicU64,
    settled_revision: AtomicU64,
    published_revision: AtomicU64,
    saturated: AtomicBool,
}

impl ContentStatus {
    pub fn new(epoch: Arc<SessionContentEpoch>) -> Self {
        Self {
            epoch,
            mutation: AtomicU8::new(NO_MUTATION),
            next_revision: AtomicU64::new(1),
            settled_revision: AtomicU64::new(0),
            published_revision: AtomicU64::new(0),
            saturated: AtomicBool::new(false),
        }
    }

    pub fn begin_mutation(&self, mutation: ContentMutation) -> bool {
        if self
            .mutation
            .compare_exchange(
                NO_MUTATION,
                mutation as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        self.epoch.mutation_started();
        true
    }

    pub fn next_revision(&self) -> ContentRevision {
        ContentRevision(self.next_revision.fetch_add(1, Ordering::Relaxed))
    }

    pub fn finish_mutation(&self, settled: ContentRevision) {
        self.settled_revision.store(settled.0, Ordering::Release);
        let previous = self.mutation.swap(NO_MUTATION, Ordering::AcqRel);
        if previous != NO_MUTATION {
            self.epoch.mutation_finished();
        }
    }

    pub fn cancel_mutation(&self) {
        let previous = self.mutation.swap(NO_MUTATION, Ordering::AcqRel);
        if previous != NO_MUTATION {
            self.epoch.mutation_finished();
        }
    }

    pub fn mark_published(&self, revision: ContentRevision) {
        self.published_revision
            .fetch_max(revision.0, Ordering::Release);
    }

    pub fn mark_saturated(&self) {
        self.saturated.store(true, Ordering::Release);
    }

    pub fn clear_saturated(&self) {
        self.saturated.store(false, Ordering::Release);
    }

    pub fn settled_revision(&self) -> ContentRevision {
        ContentRevision(self.settled_revision.load(Ordering::Acquire))
    }

    pub fn published_revision(&self) -> ContentRevision {
        ContentRevision(self.published_revision.load(Ordering::Acquire))
    }

    pub fn currentness(&self) -> SnapshotCurrentness {
        if self.saturated.load(Ordering::Acquire) {
            return SnapshotCurrentness::Stale(StaleReason::PublicationSaturated);
        }
        if let Some(mutation) = ContentMutation::from_raw(self.mutation.load(Ordering::Acquire)) {
            return SnapshotCurrentness::Stale(StaleReason::MutationActive(mutation));
        }
        let settled = self.settled_revision();
        let published = self.published_revision();
        if published < settled {
            SnapshotCurrentness::Stale(StaleReason::PublicationPending { settled, published })
        } else {
            SnapshotCurrentness::Current
        }
    }

    pub fn require_current(&self) -> Result<ContentRevision, CurrentDataError> {
        match self.currentness() {
            SnapshotCurrentness::Current => Ok(self.settled_revision()),
            SnapshotCurrentness::Stale(reason) => Err(reason.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_and_publication_state_control_exact_reads() {
        let epoch = Arc::new(SessionContentEpoch::default());
        let status = ContentStatus::new(epoch);
        assert_eq!(status.require_current(), Ok(ContentRevision(0)));

        assert!(status.begin_mutation(ContentMutation::Recording));
        assert!(!status.begin_mutation(ContentMutation::Clearing));
        assert_eq!(
            status.require_current(),
            Err(CurrentDataError::MutationActive(ContentMutation::Recording))
        );

        let revision = status.next_revision();
        status.finish_mutation(revision);
        assert_eq!(
            status.require_current(),
            Err(CurrentDataError::PublicationPending {
                settled: revision,
                published: ContentRevision(0),
            })
        );

        status.mark_published(revision);
        assert_eq!(status.require_current(), Ok(revision));
    }

    #[test]
    fn saturation_retains_revisions_but_blocks_exact_reads_until_recovered() {
        let status = ContentStatus::new(Arc::new(SessionContentEpoch::default()));
        let revision = status.next_revision();
        assert!(status.begin_mutation(ContentMutation::Loading));
        status.finish_mutation(revision);
        status.mark_published(revision);
        status.mark_saturated();
        assert_eq!(
            status.require_current(),
            Err(CurrentDataError::PublicationSaturated)
        );
        assert_eq!(status.published_revision(), revision);
        status.clear_saturated();
        assert_eq!(status.require_current(), Ok(revision));
    }

    #[test]
    fn session_epoch_rejects_overlap_and_changes_completed_between_reads() {
        let epoch = Arc::new(SessionContentEpoch::default());
        let a = ContentStatus::new(Arc::clone(&epoch));
        let b = ContentStatus::new(Arc::clone(&epoch));

        let before = epoch.capture().expect("initially stable");
        assert!(a.begin_mutation(ContentMutation::Recording));
        assert!(epoch.capture().is_none());
        a.cancel_mutation();
        assert!(!epoch.validate(before));

        let stable = epoch.capture().expect("stable after cancellation");
        assert!(b.begin_mutation(ContentMutation::Clearing));
        let revision = b.next_revision();
        b.finish_mutation(revision);
        assert!(!epoch.validate(stable));
        let after = epoch.capture().expect("stable after clear");
        assert!(epoch.validate(after));
    }
}
