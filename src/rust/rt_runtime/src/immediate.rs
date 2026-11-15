//! Complete variable-length groups with a callback-entry publication cutoff.
use crate::{Epoch, Frame, NodeId};

#[derive(Clone, Copy)]
enum Record<H, C> {
    Header(H, usize),
    Command(C),
}
pub struct Producer<H: Copy, C: Copy>(rtrb::Producer<Record<H, C>>);
pub struct Consumer<H: Copy, C: Copy> {
    queue: rtrb::Consumer<Record<H, C>>,
    remaining: usize,
}
pub struct Group<'a, H: Copy, C: Copy> {
    pub header: H,
    chunk: Option<rtrb::chunks::ReadChunk<'a, Record<H, C>>>,
}

pub fn channel<H: Copy, C: Copy>(capacity: usize) -> (Producer<H, C>, Consumer<H, C>) {
    assert!(capacity >= 2);
    let (producer, queue) = rtrb::RingBuffer::new(capacity);
    (
        Producer(producer),
        Consumer {
            queue,
            remaining: 0,
        },
    )
}
impl<H: Copy, C: Copy> Producer<H, C> {
    pub fn push(&mut self, header: H, commands: &[C]) -> Result<(), crate::Error> {
        let count = commands
            .len()
            .checked_add(1)
            .ok_or(crate::Error::Capacity)?;
        if commands.is_empty() || count > self.0.buffer().capacity() {
            return Err(crate::Error::Capacity);
        }
        self.0
            .write_chunk_uninit(count)
            .map_err(|_| crate::Error::Full)?
            .fill_from_iter(
                core::iter::once(Record::Header(header, commands.len()))
                    .chain(commands.iter().copied().map(Record::Command)),
            );
        Ok(())
    }
}
impl<H: Copy, C: Copy> Consumer<H, C> {
    pub fn begin_callback(&mut self) {
        self.remaining = self.queue.slots();
    }
    pub fn pop(&mut self) -> Option<Group<'_, H, C>> {
        if self.remaining == 0 {
            return None;
        }
        let Record::Header(header, commands) = *self.queue.peek().ok()? else {
            unreachable!("complete group header")
        };
        let records = commands + 1;
        assert!(records <= self.remaining);
        let chunk = self
            .queue
            .read_chunk(records)
            .expect("complete published group");
        self.remaining -= records;
        Some(Group {
            header,
            chunk: Some(chunk),
        })
    }
}
impl<H: Copy, C: Copy> Group<'_, H, C> {
    pub fn commands(&self) -> impl Iterator<Item = &C> {
        let (first, second) = self.chunk.as_ref().expect("readable group").as_slices();
        first
            .iter()
            .chain(second)
            .skip(1)
            .map(|record| match record {
                Record::Command(command) => command,
                Record::Header(..) => unreachable!("command record"),
            })
    }
}
impl<H: Copy, C: Copy> Drop for Group<'_, H, C> {
    fn drop(&mut self) {
        self.chunk.take().expect("owned group").commit_all();
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    extern crate std;
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    use std::sync::{Arc, Barrier};

    #[shoop_test(no_wasm = "paused native producer")]
    fn unfinished_group_is_invisible_until_its_single_publication() {
        let (mut tx, mut rx) = channel::<u32, u32>(8);
        let written = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let (producer_written, producer_resume) = (written.clone(), resume.clone());
        let producer = std::thread::spawn(move || {
            tx.0.write_chunk_uninit(4).unwrap().fill_from_iter(
                core::iter::once(Record::Header(9, 3)).chain((0..3).map(|command| {
                    if command == 1 {
                        producer_written.wait();
                        producer_resume.wait();
                    }
                    Record::Command(command)
                })),
            );
        });
        written.wait();
        rx.begin_callback();
        assert!(rx.pop().is_none());
        resume.wait();
        producer.join().unwrap();
        assert!(
            rx.pop().is_none(),
            "publication after cutoff waits for another callback"
        );
        rx.begin_callback();
        let group = rx.pop().unwrap();
        assert_eq!(group.header, 9);
        assert!(group.commands().copied().eq([0, 1, 2]));
    }
}
