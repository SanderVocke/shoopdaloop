//! Whole value batches with a callback-entry cutoff, independent of scheduled work.
use crate::{Epoch, Frame, NodeId};

pub struct Producer<T: Copy>(rtrb::Producer<T>);
pub struct Consumer<T: Copy> {
    queue: rtrb::Consumer<T>,
    remaining: usize,
}

pub fn channel<T: Copy>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let (producer, queue) = rtrb::RingBuffer::new(capacity);
    (
        Producer(producer),
        Consumer {
            queue,
            remaining: 0,
        },
    )
}
impl<T: Copy> Producer<T> {
    pub fn push(&mut self, batch: T) -> Result<(), T> {
        self.0
            .push(batch)
            .map_err(|rtrb::PushError::Full(value)| value)
    }
}
impl<T: Copy> Consumer<T> {
    pub fn begin_callback(&mut self) {
        self.remaining = self.queue.slots();
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.remaining == 0 {
            return None;
        }
        let value = self.queue.pop().ok()?;
        self.remaining -= 1;
        Some(value)
    }
}

pub trait Processors<P> {
    fn get(&self, id: NodeId) -> Option<&P>;
    fn get_mut(&mut self, id: NodeId) -> Option<&mut P>;
}
pub struct Boundary<'a, P, L> {
    pub frame: Frame,
    pub graph: Epoch,
    pub planner: &'a mut L,
    pub processors: &'a mut dyn Processors<P>,
}
pub trait Handler<P, L>: Send {
    fn begin_callback(&mut self);
    fn apply(&mut self, boundary: Boundary<'_, P, L>);
}
