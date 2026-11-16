//! Latest complete snapshots. Only the producer writes shared metadata and data.

use crate::{Epoch, Error, Frame, NodeId};
use alloc::{boxed::Box, sync::Arc};
use core::sync::atomic::{AtomicU32, Ordering::SeqCst};

struct Slot {
    sequence: AtomicU32,
    len: AtomicU32,
    bytes: Box<[AtomicU32]>,
}
struct Shared {
    slots: Box<[Slot]>,
    latest: AtomicU32,
    error: AtomicU32,
    capacity: usize,
}

pub struct Publisher {
    shared: Arc<Shared>,
    sequence: u32,
}
pub struct Reader {
    shared: Arc<Shared>,
}

/// Allocate outside rendering. Sequence exhaustion stops publication permanently;
/// a new channel is required, so a paused reader can never encounter an ABA wrap.
pub fn channel(slots: usize, capacity: usize) -> Result<(Publisher, Reader), Error> {
    if slots < 2 || capacity == 0 || capacity > u32::MAX as usize {
        return Err(Error::InvalidConfig);
    }
    if slots
        .checked_mul(capacity.div_ceil(4).checked_mul(4).ok_or(Error::Capacity)? + 8)
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(Error::Capacity);
    }
    let shared = Arc::new(Shared {
        slots: (0..slots)
            .map(|_| Slot {
                sequence: AtomicU32::new(0),
                len: AtomicU32::new(0),
                bytes: (0..capacity.div_ceil(4))
                    .map(|_| AtomicU32::new(0))
                    .collect(),
            })
            .collect(),
        latest: AtomicU32::new(0),
        error: AtomicU32::new(0),
        capacity,
    });
    Ok((
        Publisher {
            shared: shared.clone(),
            sequence: 0,
        },
        Reader { shared },
    ))
}

impl Publisher {
    pub fn publish(&mut self, payload: &[u8]) -> Result<u32, Error> {
        if payload.len() > self.shared.capacity {
            self.fail();
            return Err(Error::Capacity);
        }
        let Some(sequence) = self.sequence.checked_add(2) else {
            self.shared.error.store(2, SeqCst);
            return Err(Error::Exhausted);
        };
        self.sequence = sequence;
        let slot = &self.shared.slots[(sequence as usize / 2 - 1) % self.shared.slots.len()];
        slot.sequence.store(sequence - 1, SeqCst);
        slot.len.store(payload.len() as u32, SeqCst);
        for (cell, chunk) in slot.bytes.iter().zip(payload.chunks(4)) {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            cell.store(u32::from_le_bytes(word), SeqCst);
        }
        slot.sequence.store(sequence, SeqCst);
        self.shared.latest.store(sequence, SeqCst);
        Ok(sequence)
    }
    pub fn fail(&self) {
        self.shared.error.store(1, SeqCst);
    }
}

impl Reader {
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
    pub fn error(&self) -> Option<Error> {
        match self.shared.error.load(SeqCst) {
            0 => None,
            2 => Some(Error::Exhausted),
            _ => Some(Error::Capacity),
        }
    }
    /// One bounded attempt. The caller must discard scratch on None, and keep its
    /// previously accepted snapshot in separate storage. No payload is interpreted here.
    pub fn read(&self, scratch: &mut [u8]) -> Option<(u32, usize)> {
        let sequence = self.shared.latest.load(SeqCst);
        if sequence == 0 {
            return None;
        }
        let slot = &self.shared.slots[(sequence as usize / 2 - 1) % self.shared.slots.len()];
        if slot.sequence.load(SeqCst) != sequence {
            return None;
        }
        let len = slot.len.load(SeqCst) as usize;
        if len > self.shared.capacity || len > scratch.len() {
            return None;
        }
        for (chunk, word) in scratch[..len].chunks_mut(4).zip(&slot.bytes) {
            chunk.copy_from_slice(&word.load(SeqCst).to_le_bytes()[..chunk.len()]);
        }
        (slot.sequence.load(SeqCst) == sequence).then_some((sequence, len))
    }
}

pub struct Capture<'a, P, L> {
    pub frame: Frame,
    pub graph_epoch: Epoch,
    pub planner: &'a L,
    pub nodes: &'a mut dyn Iterator<Item = (NodeId, &'a P)>,
}

/// Called once at the first settled tick of each callback that processes a tick.
/// Implementations obey the callback no-allocation/no-wait contract.
pub trait Observer<P, L>: Send {
    fn capture(&mut self, context: Capture<'_, P, L>);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn stalled_reader_capacity_and_exhaustion() {
        let (mut tx, rx) = channel(10, 8).unwrap();
        let mut bytes = [0; 8];
        assert!(rx.read(&mut bytes).is_none());
        for n in 0..100u64 {
            tx.publish(&n.to_le_bytes()).unwrap();
        }
        assert_eq!(rx.read(&mut bytes), Some((200, 8)));
        assert_eq!(u64::from_le_bytes(bytes), 99);
        assert_eq!(tx.publish(&[0; 9]), Err(Error::Capacity));
        assert_eq!(rx.read(&mut bytes), Some((200, 8)));
        tx.sequence = u32::MAX - 1;
        assert_eq!(tx.publish(&[]), Err(Error::Exhausted));
    }
    #[shoop_wasm_test_support::shoop_test]
    fn rejects_overwritten_slot() {
        let (mut tx, rx) = channel(2, 8).unwrap();
        tx.publish(&[1; 8]).unwrap();
        rx.shared.slots[0].sequence.store(5, SeqCst);
        assert!(rx.read(&mut [0; 8]).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn concurrent_wraps_never_accept_mixed_payloads() {
        let (mut tx, rx) = channel(2, 256).unwrap();
        let writer = std::thread::spawn(move || {
            for value in 1..=4000u32 {
                let mut bytes = [0; 256];
                for word in bytes.chunks_mut(4) {
                    word.copy_from_slice(&value.to_le_bytes());
                }
                tx.publish(&bytes).unwrap();
            }
        });
        let mut scratch = [0; 256];
        for _ in 0..8000 {
            if rx.read(&mut scratch).is_some() {
                assert!(scratch.chunks(4).all(|word| word == &scratch[..4]));
            }
            if writer.is_finished() {
                break;
            }
            std::thread::yield_now();
        }
        writer.join().unwrap();
        rx.read(&mut scratch).unwrap();
        assert!(scratch.chunks(4).all(|word| word == 4000u32.to_le_bytes()));
    }
}
