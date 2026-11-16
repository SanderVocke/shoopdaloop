//! Complete variable-length groups with a callback-entry publication cutoff.
use crate::{Epoch, Frame, NodeId};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
/// The next unrendered processing frame in this channel's engine lifetime.
pub struct Clock(Arc<AtomicU64>);
impl Clock {
    pub fn frame(&self) -> Frame {
        Frame(self.0.load(Ordering::Acquire))
    }
}

#[derive(Clone, Copy)]
enum Record<H, C> {
    Header(H, usize),
    Command(C),
}
pub struct Producer<H: Copy, C: Copy> {
    queue: rtrb::Producer<Record<H, C>>,
    responses: Option<rtrb::Consumer<Record<H, C>>>,
    reserved: usize,
    clock: Clock,
}
pub struct Consumer<H: Copy, C: Copy> {
    queue: rtrb::Consumer<Record<H, C>>,
    responses: Option<rtrb::Producer<Record<H, C>>>,
    remaining: usize,
    clock: Clock,
}
pub struct Group<'a, H: Copy, C: Copy> {
    pub header: H,
    chunk: Option<rtrb::chunks::ReadChunk<'a, Record<H, C>>>,
    reply: Option<rtrb::chunks::WriteChunkUninit<'a, Record<H, C>>>,
    credit: Option<&'a mut usize>,
    count: usize,
}

pub fn channel<H: Copy, C: Copy>(capacity: usize) -> (Producer<H, C>, Consumer<H, C>) {
    assert!(capacity >= 2);
    let (producer, queue) = rtrb::RingBuffer::new(capacity);
    let clock = Clock(Arc::new(AtomicU64::new(0)));
    (
        Producer {
            queue: producer,
            responses: None,
            reserved: 0,
            clock: clock.clone(),
        },
        Consumer {
            queue,
            responses: None,
            remaining: 0,
            clock,
        },
    )
}
pub fn paired_channel<H: Copy, C: Copy>(capacity: usize) -> (Producer<H, C>, Consumer<H, C>) {
    let (mut control, mut render) = channel(capacity);
    let (tx, rx) = rtrb::RingBuffer::new(capacity);
    control.responses = Some(rx);
    render.responses = Some(tx);
    (control, render)
}
impl<H: Copy, C: Copy> Producer<H, C> {
    pub fn clock(&self) -> Clock {
        self.clock.clone()
    }

    pub fn push(&mut self, header: H, commands: &[C]) -> Result<(), crate::Error> {
        let count = commands
            .len()
            .checked_add(1)
            .ok_or(crate::Error::Capacity)?;
        if commands.is_empty() || count > self.queue.buffer().capacity() {
            return Err(crate::Error::Capacity);
        }
        if self.responses.is_some() && count > self.queue.buffer().capacity() - self.reserved {
            return Err(crate::Error::Full);
        }
        self.queue
            .write_chunk_uninit(count)
            .map_err(|_| crate::Error::Full)?
            .fill_from_iter(
                core::iter::once(Record::Header(header, commands.len()))
                    .chain(commands.iter().copied().map(Record::Command)),
            );
        if self.responses.is_some() {
            self.reserved += count;
        }
        Ok(())
    }

    pub fn response(&mut self) -> Option<Group<'_, H, C>> {
        let queue = self.responses.as_mut()?;
        let Record::Header(header, commands) = *queue.peek().ok()? else {
            unreachable!("complete response header")
        };
        let count = commands + 1;
        let chunk = queue.read_chunk(count).expect("complete response group");
        Some(Group {
            header,
            chunk: Some(chunk),
            reply: None,
            credit: Some(&mut self.reserved),
            count,
        })
    }
}
impl<H: Copy, C: Copy> Consumer<H, C> {
    pub fn publish_frame(&mut self, frame: Frame) {
        self.clock.0.store(frame.0, Ordering::Release);
    }
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
        let reply = self.responses.as_mut().map(|queue| {
            queue
                .write_chunk_uninit(records)
                .expect("response space reserved before publication")
        });
        self.remaining -= records;
        Some(Group {
            header,
            chunk: Some(chunk),
            reply,
            credit: None,
            count: records,
        })
    }
}
impl<H: Copy, C: Copy> Group<'_, H, C> {
    pub fn respond(mut self, header: H, mut process: impl FnMut(C) -> C) {
        let reply = self.reply.take().expect("paired command channel");
        reply.fill_from_iter(
            core::iter::once(Record::Header(header, self.count - 1)).chain(
                self.commands()
                    .copied()
                    .map(|command| Record::Command(process(command))),
            ),
        );
    }

    /// Keep a response queued when downstream delivery has no capacity.
    pub fn leave_unread(mut self) {
        self.credit = None;
        self.chunk.take();
    }

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
        if let Some(chunk) = self.chunk.take() {
            chunk.commit_all();
            if let Some(credit) = &mut self.credit {
                **credit -= self.count;
            }
        }
    }
}

pub trait Processors<P> {
    fn get(&self, id: NodeId) -> Option<&P>;
    fn get_mut(&mut self, id: NodeId) -> Option<&mut P>;
}
pub struct Boundary<'a, P, L, S: crate::Sample = f32> {
    pub frame: Frame,
    pub graph: Epoch,
    pub available: bool,
    pub planner: &'a mut L,
    pub processors: &'a mut dyn Processors<P>,
    pub resources: crate::resource::PlannerResourceIo<'a, S>,
}
pub trait Handler<P, L, S: crate::Sample = f32>: Send {
    fn begin_callback(&mut self, frame: Frame);
    fn apply(&mut self, boundary: Boundary<'_, P, L, S>);
    fn end_callback(&mut self, _next_frame: Frame) {}
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
            tx.queue.write_chunk_uninit(4).unwrap().fill_from_iter(
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
