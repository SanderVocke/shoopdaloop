use super::ContentStatus;
use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::sync::Arc;

pub struct ProcessSender<T> {
    producer: Producer<T>,
    status: Arc<ContentStatus>,
}

pub struct PublisherReceiver<T> {
    consumer: Consumer<T>,
}

pub fn bounded_transport<T>(
    capacity: usize,
    status: Arc<ContentStatus>,
) -> (ProcessSender<T>, PublisherReceiver<T>) {
    let (producer, consumer) = RingBuffer::new(capacity.max(1));
    (
        ProcessSender { producer, status },
        PublisherReceiver { consumer },
    )
}

impl<T> ProcessSender<T> {
    pub fn try_send(&mut self, item: T) -> Result<(), T> {
        match self.producer.push(item) {
            Ok(()) => Ok(()),
            Err(PushError::Full(item)) => {
                self.status.mark_saturated();
                Err(item)
            }
        }
    }

    pub fn slots(&self) -> usize {
        self.producer.slots()
    }
}

impl<T> PublisherReceiver<T> {
    pub fn try_recv(&mut self) -> Option<T> {
        match self.consumer.pop() {
            Ok(item) => Some(item),
            Err(PopError::Empty) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{
        CurrentDataError, SessionContentEpoch, SnapshotCurrentness, StaleReason,
    };

    #[shoop_wasm_test_support::shoop_test]
    fn fifo_transport_is_bounded_and_marks_saturation() {
        let status = Arc::new(ContentStatus::new(Arc::new(SessionContentEpoch::default())));
        let (mut sender, mut receiver) = bounded_transport(2, Arc::clone(&status));
        assert_eq!(sender.try_send(1), Ok(()));
        assert_eq!(sender.try_send(2), Ok(()));
        assert_eq!(sender.try_send(3), Err(3));
        assert_eq!(
            status.currentness(),
            SnapshotCurrentness::Stale(StaleReason::PublicationSaturated)
        );
        assert_eq!(
            status.require_current(),
            Err(CurrentDataError::PublicationSaturated)
        );
        assert_eq!(receiver.try_recv(), Some(1));
        assert_eq!(receiver.try_recv(), Some(2));
        assert_eq!(receiver.try_recv(), None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_failed_push_returns_ownership_to_the_process_side() {
        let status = Arc::new(ContentStatus::new(Arc::new(SessionContentEpoch::default())));
        let (mut sender, _receiver) = bounded_transport(1, status);
        let first = Box::new(1);
        let second = Box::new(2);
        assert!(sender.try_send(first).is_ok());
        let returned = sender.try_send(second).expect_err("queue is full");
        assert_eq!(*returned, 2);
    }
}
